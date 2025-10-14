use anyhow::Result;
use liminal_core::EventDelta;

pub fn encode_delta(delta: &EventDelta) -> Result<Vec<u8>> {
    Ok(serde_cbor::to_vec(delta)?)
}

pub fn decode_delta(bytes: &[u8]) -> Result<EventDelta> {
    Ok(serde_cbor::from_slice(bytes)?)
}
