//! Live benchmark runner for a real LiminalDB instance.
//!
//! This example connects to the WebSocket server exposed by `liminal-cli`,
//! drives a small live workload, and prints measured round-trip latency plus
//! a coarse ingest throughput probe.
//!
//! Start the server first:
//!   cargo build --release -p liminal-cli
//!   ./target/release/liminal-cli --store ./data --ws-port 8787
//!
//! Then run:
//!   cargo run -p liminaldb-client --example live-benchmark --release
//!
//! Optional flags:
//!   --url ws://127.0.0.1:8787
//!   --warmup 50
//!   --query-rounds 25
//!   --batch-rounds 5
//!   --batch-size 500
//!   --timeline-top 20

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value as JsonValue};
use tokio::net::TcpStream;
use tokio::time::Instant;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tungstenite::protocol::Message;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
struct BenchConfig {
    url: String,
    warmup: usize,
    query_rounds: usize,
    batch_rounds: usize,
    batch_size: usize,
    timeline_top: usize,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:8787".to_string(),
            warmup: 50,
            query_rounds: 25,
            batch_rounds: 5,
            batch_size: 500,
            timeline_top: 20,
        }
    }
}

#[derive(Debug, Clone)]
struct Stats {
    samples_ms: Vec<f64>,
}

impl Stats {
    fn from_samples(mut samples_ms: Vec<f64>) -> Self {
        samples_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Self { samples_ms }
    }

    fn p50(&self) -> f64 {
        percentile(&self.samples_ms, 50)
    }

    fn p95(&self) -> f64 {
        percentile(&self.samples_ms, 95)
    }

    fn p99(&self) -> f64 {
        percentile(&self.samples_ms, 99)
    }

    fn avg(&self) -> f64 {
        self.samples_ms.iter().sum::<f64>() / self.samples_ms.len() as f64
    }
}

fn percentile(sorted_samples: &[f64], pct: usize) -> f64 {
    let idx = ((sorted_samples.len().saturating_sub(1)) * pct) / 100;
    sorted_samples[idx]
}

fn parse_args() -> Result<BenchConfig> {
    let mut cfg = BenchConfig::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => {
                cfg.url = args.next().context("--url requires a value")?;
            }
            "--warmup" => {
                cfg.warmup = args
                    .next()
                    .context("--warmup requires a value")?
                    .parse()
                    .context("--warmup expects an integer")?;
            }
            "--query-rounds" => {
                cfg.query_rounds = args
                    .next()
                    .context("--query-rounds requires a value")?
                    .parse()
                    .context("--query-rounds expects an integer")?;
            }
            "--batch-rounds" => {
                cfg.batch_rounds = args
                    .next()
                    .context("--batch-rounds requires a value")?
                    .parse()
                    .context("--batch-rounds expects an integer")?;
            }
            "--batch-size" => {
                cfg.batch_size = args
                    .next()
                    .context("--batch-size requires a value")?
                    .parse()
                    .context("--batch-size expects an integer")?;
            }
            "--timeline-top" => {
                cfg.timeline_top = args
                    .next()
                    .context("--timeline-top requires a value")?
                    .parse()
                    .context("--timeline-top expects an integer")?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    Ok(cfg)
}

fn print_help() {
    println!("LiminalDB live benchmark runner");
    println!("  --url <ws-url>");
    println!("  --warmup <count>");
    println!("  --query-rounds <count>");
    println!("  --batch-rounds <count>");
    println!("  --batch-size <count>");
    println!("  --timeline-top <count>");
}

async fn send_json(ws: &mut WsStream, value: JsonValue) -> Result<()> {
    ws.send(Message::Text(value.to_string()))
        .await
        .context("failed to send websocket message")
}

async fn wait_for_event(ws: &mut WsStream, event_name: &str) -> Result<JsonValue> {
    let timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => {
                bail!("timed out waiting for event `{event_name}`");
            }
            maybe_msg = ws.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(text))) => {
                        let value: JsonValue = serde_json::from_str(&text)
                            .with_context(|| format!("failed to decode server message: {text}"))?;
                        if value.get("ev").and_then(|v| v.as_str()) == Some(event_name) {
                            return Ok(value);
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        ws.send(Message::Pong(payload))
                            .await
                            .context("failed to answer ping")?;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Close(frame))) => {
                        return Err(anyhow!("websocket closed while waiting for {event_name}: {frame:?}"));
                    }
                    Some(Err(err)) => return Err(err).context("websocket receive failed"),
                    None => bail!("websocket closed while waiting for `{event_name}`"),
                }
            }
        }
    }
}

fn impulse_payload(sequence: usize) -> JsonValue {
    json!({
        "cmd": "impulse",
        "data": {
            "pattern": format!("bench/live/{}", sequence % 32),
            "kind": "query",
            "strength": ((sequence % 100) as f64) / 100.0,
            "ts": sequence,
        }
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = parse_args()?;

    println!("LiminalDB live benchmark");
    println!("  url           : {}", cfg.url);
    println!("  warmup        : {}", cfg.warmup);
    println!("  query_rounds  : {}", cfg.query_rounds);
    println!("  batch_rounds  : {}", cfg.batch_rounds);
    println!("  batch_size    : {}", cfg.batch_size);
    println!("  timeline_top  : {}", cfg.timeline_top);

    let (mut ws, _) = connect_async(&cfg.url)
        .await
        .with_context(|| format!("failed to connect to {}", cfg.url))?;

    for i in 0..cfg.warmup {
        send_json(&mut ws, impulse_payload(i)).await?;
    }

    send_json(
        &mut ws,
        json!({
            "cmd": "mirror.timeline",
            "top": cfg.timeline_top,
        }),
    )
    .await?;
    let _ = wait_for_event(&mut ws, "mirror.timeline").await?;

    println!("\nPhase 1: live LQL round-trip");
    let mut query_samples = Vec::with_capacity(cfg.query_rounds);
    for round in 0..cfg.query_rounds {
        let started = Instant::now();
        send_json(
            &mut ws,
            json!({
                "cmd": "lql",
                "q": "SELECT bench/live WINDOW 1000",
                "round": round,
            }),
        )
        .await?;
        let _ = wait_for_event(&mut ws, "lql").await?;
        query_samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let query_stats = Stats::from_samples(query_samples);
    println!("  p50           : {:>8.2} ms", query_stats.p50());
    println!("  p95           : {:>8.2} ms", query_stats.p95());
    println!("  p99           : {:>8.2} ms", query_stats.p99());
    println!("  avg           : {:>8.2} ms", query_stats.avg());

    println!("\nPhase 2: ingest batch + timeline probe");
    let mut batch_samples = Vec::with_capacity(cfg.batch_rounds);
    let mut total_events = 0usize;
    for round in 0..cfg.batch_rounds {
        let started = Instant::now();
        for i in 0..cfg.batch_size {
            send_json(
                &mut ws,
                impulse_payload(round * cfg.batch_size + i + cfg.warmup),
            )
            .await?;
        }
        send_json(
            &mut ws,
            json!({
                "cmd": "mirror.timeline",
                "top": cfg.timeline_top,
                "round": round,
            }),
        )
        .await?;
        let _ = wait_for_event(&mut ws, "mirror.timeline").await?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        batch_samples.push(elapsed_ms);
        total_events += cfg.batch_size;
    }
    let batch_stats = Stats::from_samples(batch_samples);
    let throughput = total_events as f64 / ((cfg.batch_rounds as f64 * batch_stats.avg()) / 1000.0);
    println!("  batch p50     : {:>8.2} ms", batch_stats.p50());
    println!("  batch p95     : {:>8.2} ms", batch_stats.p95());
    println!("  batch p99     : {:>8.2} ms", batch_stats.p99());
    println!("  batch avg     : {:>8.2} ms", batch_stats.avg());
    println!("  est ingest    : {:>8.0} events/sec", throughput);

    println!("\nNotes:");
    println!("  - LQL numbers are measured live round-trip latency over WebSocket.");
    println!("  - Batch numbers measure impulse ingest followed by a mirror timeline probe.");
    println!("  - For reviewer-grade publishing, record hardware, OS, commit SHA, and exact command lines.");

    Ok(())
}
