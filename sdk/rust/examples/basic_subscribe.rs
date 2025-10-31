use liminaldb_client::{ClientOptions, LiminalClient};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut opts = ClientOptions::default();
    opts.url = "wss://demo.liminaldb.dev/ws".parse()?;
    let client = LiminalClient::connect(opts).await?;

    client.auth().await;
    let sub_id = client.subscribe("harmony/*", None).await;
    println!("Subscribed with id {}", sub_id);

    let mut events = client.events();
    while let Some(event) = events.next().await {
        println!("event: {:?}", event);
    }

    Ok(())
}
