use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_cbor::Value as CborValue;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::collections::BTreeMap;
use tungstenite::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "cbor")]
    Cbor,
}

impl Default for StreamFormat {
    fn default() -> Self {
        StreamFormat::Json
    }
}

pub fn encode_cbor_json(value: &JsonValue, format: StreamFormat) -> Result<Message> {
    match format {
        StreamFormat::Json => Ok(Message::Text(serde_json::to_string(value)?)),
        StreamFormat::Cbor => {
            let cbor_value = json_to_cbor(value);
            let bytes = serde_cbor::to_vec(&cbor_value)?;
            Ok(Message::Binary(bytes))
        }
    }
}

pub fn decode_cbor_json(message: Message) -> Result<(JsonValue, StreamFormat)> {
    match message {
        Message::Text(text) => {
            let value = serde_json::from_str(&text)?;
            Ok((value, StreamFormat::Json))
        }
        Message::Binary(bytes) => {
            let cbor_value: CborValue = serde_cbor::from_slice(&bytes)?;
            let json_value = cbor_to_json(&cbor_value);
            Ok((json_value, StreamFormat::Cbor))
        }
        other => Err(anyhow!("unsupported websocket message: {other:?}")),
    }
}

fn json_to_cbor(value: &JsonValue) -> CborValue {
    match value {
        JsonValue::Null => CborValue::Null,
        JsonValue::Bool(b) => CborValue::Bool(*b),
        JsonValue::Number(num) => {
            if let Some(u) = num.as_u64() {
                CborValue::Integer(u as i128)
            } else if let Some(i) = num.as_i64() {
                CborValue::Integer(i as i128)
            } else if let Some(f) = num.as_f64() {
                CborValue::Float(f)
            } else {
                CborValue::Null
            }
        }
        JsonValue::String(s) => CborValue::Text(s.clone()),
        JsonValue::Array(items) => {
            CborValue::Array(items.iter().map(json_to_cbor).collect::<Vec<_>>())
        }
        JsonValue::Object(map) => {
            let mut cbor_map = BTreeMap::new();
            for (key, value) in map {
                let short = shorten_key(key);
                cbor_map.insert(CborValue::Text(short), json_to_cbor(value));
            }
            CborValue::Map(cbor_map)
        }
    }
}

fn cbor_to_json(value: &CborValue) -> JsonValue {
    match value {
        CborValue::Null => JsonValue::Null,
        CborValue::Bool(b) => JsonValue::Bool(*b),
        CborValue::Integer(i) => json!(*i),
        CborValue::Float(f) => json!(*f),
        CborValue::Bytes(bytes) => JsonValue::Array(bytes.iter().map(|b| json!(*b)).collect()),
        CborValue::Text(text) => JsonValue::String(text.clone()),
        CborValue::Array(items) => {
            JsonValue::Array(items.iter().map(cbor_to_json).collect::<Vec<_>>())
        }
        CborValue::Map(map) => {
            let mut object = JsonMap::new();
            for (key, value) in map.iter() {
                let key_str = match key {
                    CborValue::Text(text) => text.clone(),
                    other => format!("{other:?}"),
                };
                let long = expand_key(&key_str);
                object.insert(long, cbor_to_json(value));
            }
            JsonValue::Object(object)
        }
        CborValue::Tag(_, inner) => cbor_to_json(inner),
        _ => JsonValue::Null,
    }
}

fn shorten_key(key: &str) -> String {
    match key {
        "command" | "cmd" => "cmd".into(),
        "query" | "q" => "q".into(),
        "data" | "d" => "d".into(),
        "pattern" | "p" => "p".into(),
        "format" | "f" => "f".into(),
        "events" | "ev" => "ev".into(),
        "metrics" | "mt" => "mt".into(),
        "meta" => "m".into(),
        "source" | "src" => "src".into(),
        "id" => "id".into(),
        "dt" => "dt".into(),
        other => other.to_string(),
    }
}

fn expand_key(key: &str) -> String {
    match key {
        "cmd" => "cmd".into(),
        "q" => "q".into(),
        "d" => "data".into(),
        "p" => "pattern".into(),
        "f" => "format".into(),
        "ev" => "events".into(),
        "mt" => "metrics".into(),
        "m" => "meta".into(),
        "src" => "source".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_json() {
        let original = json!({
            "cmd": "impulse",
            "data": {"pattern": "cpu/load", "strength": 0.8},
        });
        let message = encode_cbor_json(&original, StreamFormat::Cbor).unwrap();
        let (decoded, format) = decode_cbor_json(message).unwrap();
        assert_eq!(format, StreamFormat::Cbor);
        assert_eq!(decoded["cmd"], original["cmd"]);
        assert_eq!(decoded["data"].as_object().unwrap()["pattern"], "cpu/load");
    }
}
