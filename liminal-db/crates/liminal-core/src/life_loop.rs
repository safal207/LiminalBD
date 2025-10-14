use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::cluster_field::ClusterField;
use crate::morph_mind::{analyze, hints as gather_hints};
use crate::types::{Hint, Metrics};

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

    loop {
        sleep(Duration::from_millis(tick_ms)).await;
        let mut guard = field.lock().await;
        let events = guard.tick_all(tick_ms);
        for event in events {
            on_event(&event);
        }

        elapsed_since_metrics += tick_ms;
        if elapsed_since_metrics >= 1000 {
            let metrics = analyze(&guard);
            let advice = gather_hints(&metrics);
            for hint in &advice {
                on_hint(hint);
            }
            apply_hints(&mut *guard, &mut tick_ms, &advice);
            on_metrics(&metrics);
            elapsed_since_metrics = 0;
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
