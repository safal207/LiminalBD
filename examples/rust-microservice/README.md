# LiminalDB Rust Microservice

A minimal async microservice subscribing to `dream/*` events and responding with acknowledgement intents.

## Running

```bash
cargo run --manifest-path examples/rust-microservice/Cargo.toml
```

Set `LIMINALDB_URL`, `LIMINALDB_KEY_ID`, and `LIMINALDB_SECRET` for authentication.
