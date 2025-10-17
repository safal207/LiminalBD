use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::cluster_field::ClusterField;
use crate::morph_mind::{analyze, hints as gather_hints};
use crate::types::{Hint, Metrics};
use serde_json::json;

pub async fn run_loop<F, G, H>(
    field: Arc<Mutex<ClusterField>>,
    mut tick_ms: u64,
    on_metrics: F,
    on_event: G,
    on_hint: H,
) where
    F: Fn(&Metrics) + Send + Sync + 'static,
    G: Fn(&str) + Send + Sync + 'static,
    H: Fn(&Hint) + Send + Sync + 'static,
{
    let on_metrics = Arc::new(on_metrics);
    let on_event = Arc::new(on_event);
    let on_hint = Arc::new(on_hint);

    let mut elapsed_since_metrics = 0u64;
    let mut elapsed_since_partial = 0u64;
    let mut last_metrics: Option<Metrics> = None;
    struct DreamState {
        last_start_ms: u64,
        restore_tick: Option<u64>,
        slowed_once: bool,
    }
    let mut dream_state = DreamState {
        last_start_ms: 0,
        restore_tick: None,
        slowed_once: false,
    };

    loop {
        if let Some(original) = dream_state.restore_tick {
            if dream_state.slowed_once {
                tick_ms = original;
                dream_state.restore_tick = None;
                dream_state.slowed_once = false;
            }
        }
        sleep(Duration::from_millis(tick_ms)).await;
        if dream_state.restore_tick.is_some() && !dream_state.slowed_once {
            dream_state.slowed_once = true;
        }
        let mut guard = field.lock().await;
        let mut events = guard.tick_all(tick_ms);

        elapsed_since_metrics += tick_ms;
        elapsed_since_partial += tick_ms;
        let mut partial_payload: Option<(Vec<u8>, usize, u64)> = None;
        if elapsed_since_partial >= 5_000 {
            elapsed_since_partial = 0;
            let important = guard.important_cells().clone();
            if !important.is_empty() {
                let bytes = guard.partial_snapshot(&important);
                let ts = guard.now_ms;
                partial_payload = Some((bytes, important.len(), ts));
            }
        }

        if elapsed_since_metrics >= 1000 {
            let metrics = analyze(&guard);
            let observed = metrics.observed_load();
            let target = metrics.suggest_target();
            let now_ms = guard.now_ms;
            guard.trs.set_target(target);
            let trs_output = guard.trs.step(now_ms, observed);
            let new_tick = (tick_ms as i64 + trs_output.tick_adjust_ms as i64).clamp(80, 450);
            tick_ms = new_tick as u64;
            let trs_events = guard.apply_trs_output(now_ms, observed, &trs_output);
            for event in trs_events {
                on_event(&event);
            }
            let advice = gather_hints(&metrics);
            for hint in &advice {
                on_hint(hint);
            }
            apply_hints(&mut *guard, &mut tick_ms, &advice);
            on_metrics(&metrics);
            last_metrics = Some(metrics.clone());
            elapsed_since_metrics = 0;
        }
        if let Some(metrics) = last_metrics.as_ref() {
            let cfg = guard.dream_config();
            let idle_elapsed = guard
                .now_ms
                .saturating_sub(dream_state.last_start_ms);
            if metrics.sleeping_pct > 0.6
                && metrics.live_load < 0.4
                && idle_elapsed > (cfg.min_idle_s as u64) * 1_000
            {
                if let Some(report) = guard.run_dream_cycle() {
                    dream_state.last_start_ms = guard.now_ms;
                    let original_tick = dream_state.restore_tick.unwrap_or(tick_ms);
                    dream_state.restore_tick = Some(original_tick);
                    dream_state.slowed_once = false;
                    let mut slower = ((tick_ms as f32) * 1.15).round() as u64;
                    if slower <= tick_ms {
                        slower = tick_ms.saturating_add(5);
                    }
                    slower = slower.clamp(80, 800);
                    tick_ms = slower;
                    events.push(format!(
                        "DREAM strengthened={} weakened={} pruned={} rewired={} protected={} took={}ms",
                        report.strengthened,
                        report.weakened,
                        report.pruned,
                        report.rewired,
                        report.protected,
                        report.took_ms
                    ));
                    events.push(
                        json!({
                            "ev": "dream",
                            "meta": {
                                "strengthened": report.strengthened,
                                "weakened": report.weakened,
                                "pruned": report.pruned,
                                "rewired": report.rewired,
                                "protected": report.protected,
                                "took_ms": report.took_ms
                            }
                        })
                        .to_string(),
                    );
                }
            }
        }
        let partial_payload = partial_payload;
        drop(guard);
        if let Some((bytes, count, ts)) = partial_payload {
            let dir = Path::new("snap");
            let mut log_line = None;
            let mut json_line = None;
            if let Err(err) = fs::create_dir_all(dir) {
                events.push(format!("PARTIAL SNAPSHOT ERROR: create dir failed: {err}"));
            } else {
                let path = dir.join(format!("partial_{}.psnap", ts));
                match fs::write(&path, &bytes) {
                    Ok(()) => {
                        let size_kb = (bytes.len() as f32) / 1024.0;
                        log_line = Some(format!(
                            "PARTIAL SNAPSHOT: cells={} size={:.1}KB",
                            count, size_kb
                        ));
                        json_line = Some(
                            json!({
                                "ev": "snapshot",
                                "meta": {"kind": "partial", "cells": count}
                            })
                            .to_string(),
                        );
                    }
                    Err(err) => {
                        events.push(format!(
                            "PARTIAL SNAPSHOT ERROR: write {}: {}",
                            path.display(),
                            err
                        ));
                    }
                }
            }
            if let Some(line) = log_line {
                events.push(line);
            }
            if let Some(json_line) = json_line {
                events.push(json_line);
            }
        }
        for event in events {
            on_event(&event);
        }
    }
}

fn apply_hints(field: &mut ClusterField, tick_ms: &mut u64, hints: &[Hint]) {
    for hint in hints {
        match hint {
            Hint::SlowTick => {
                *tick_ms = (*tick_ms + 50).min(400);
            }
            Hint::FastTick => {
                if *tick_ms > 100 {
                    *tick_ms = (*tick_ms).saturating_sub(10).max(100);
                }
            }
            Hint::TrimField => {
                field.trim_low_energy();
            }
            Hint::WakeSeeds => {
                field.inject_seed_variation("liminal/wake");
            }
        }
    }
}
