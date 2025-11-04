use std::collections::HashMap;

use lazy_static::lazy_static;
use regex::Regex;
use serde_json::Value;

use crate::{
    echo::{Intent, IntentKind},
    security::NsId,
};

#[derive(Debug, thiserror::Error)]
pub enum NluError {
    #[error("unrecognized intent")]
    Unrecognized,
}

lazy_static! {
    static ref BOOST_RE: Regex = Regex::new(r"^(?i)(boost|усиль)\s+(?P<token>.+)$").unwrap();
    static ref DEPRIORITIZE_RE: Regex =
        Regex::new(r"^(?i)(deprioritize|понизь)\s+(?P<token>.+)$").unwrap();
    static ref SNAPSHOT_RE: Regex = Regex::new(r"^(?i)(snapshot|снимок)\s+(?P<token>.+)$").unwrap();
    static ref SET_TARGET_RE: Regex =
        Regex::new(r"^(?i)(set\s+target|установи\s+цель)\s+(?P<value>0\.[0-9]+|1\.0)$").unwrap();
    static ref EXPLAIN_RE: Regex = Regex::new(r"^(?i)(explain|объясни)\s+(?P<token>.+)$").unwrap();
    static ref DREAM_NOW_RE: Regex = Regex::new(r"^(?i)(dream\s+now|сон\s+сейчас)$").unwrap();
    static ref SYNC_NOW_RE: Regex =
        Regex::new(r"^(?i)(sync\s+now|синхронизируй\s+сейчас)$").unwrap();
    static ref AWAKEN_NOW_RE: Regex = Regex::new(r"^(?i)(awaken\s+now|пробуди\s+сейчас)$").unwrap();
    static ref HOW_ARE_YOU_RE: Regex =
        Regex::new(r"^(?i)(how\s+are\s+you\??|как\s+ты\??)$").unwrap();
}

pub fn parse_intent_text(ns: NsId, id: u64, ts: u64, text: &str) -> Result<Intent, NluError> {
    let normalized = text.trim();
    if normalized.is_empty() {
        return Err(NluError::Unrecognized);
    }

    if let Some(cap) = BOOST_RE.captures(normalized) {
        let token = cap.name("token").unwrap().as_str().trim().to_string();
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::Boost,
            target: token,
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    if let Some(cap) = DEPRIORITIZE_RE.captures(normalized) {
        let token = cap.name("token").unwrap().as_str().trim().to_string();
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::Deprioritize,
            target: token,
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    if let Some(cap) = SNAPSHOT_RE.captures(normalized) {
        let token = cap.name("token").unwrap().as_str().trim().to_string();
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::Snapshot,
            target: token,
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    if DREAM_NOW_RE.is_match(normalized) {
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::DreamNow,
            target: String::new(),
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    if SYNC_NOW_RE.is_match(normalized) {
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::SyncNow,
            target: String::new(),
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    if AWAKEN_NOW_RE.is_match(normalized) {
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::AwakenNow,
            target: String::new(),
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    if let Some(cap) = SET_TARGET_RE.captures(normalized) {
        let value = cap.name("value").unwrap().as_str().parse::<f32>().unwrap();
        let mut args = HashMap::new();
        args.insert("load".to_string(), Value::from(value));
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::SetTarget,
            target: value.to_string(),
            args,
            affect: 0.0,
        });
    }

    if let Some(cap) = EXPLAIN_RE.captures(normalized) {
        let token = cap.name("token").unwrap().as_str().trim().to_string();
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::Explain,
            target: token,
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    if HOW_ARE_YOU_RE.is_match(normalized) {
        return Ok(Intent {
            id,
            ts,
            ns,
            kind: IntentKind::Ask,
            target: String::new(),
            args: HashMap::new(),
            affect: 0.0,
        });
    }

    Err(NluError::Unrecognized)
}
