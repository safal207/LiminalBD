use liminaldb_client::{ClientOptions, LiminalClient, protocol_types::EventEnvelope};
use serde_json::json;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut opts = ClientOptions::default();
    opts.url = std::env::var("LIMINALDB_URL")?.parse()?;
    opts.key_id = std::env::var("LIMINALDB_KEY_ID").ok();
    opts.secret = std::env::var("LIMINALDB_SECRET").ok();
    let client = LiminalClient::connect(opts).await?;
    client.auth().await;

    let mut events = client.events();
    client.subscribe("dream/*", None).await;

    while let Some(event) = events.next().await {
        tracing::info!(?event, "dream event");
        if let EventEnvelope::DreamEvent(dream) = event {
            client
                .intent(
                    "ack",
                    json!({ "event_id": dream.parts.0.id }),
                )
                .await;
        }
    }

    Ok(())
}
