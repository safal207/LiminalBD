use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{ClusterField, NsId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IntentKind {
    Boost,
    Deprioritize,
    Snapshot,
    DreamNow,
    SyncNow,
    AwakenNow,
    SetTarget,
    Query,
    Explain,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: u64,
    pub ts: u64,
    pub ns: NsId,
    pub kind: IntentKind,
    pub target: String,
    pub args: HashMap<String, Value>,
    pub affect: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoEvent {
    pub ev: String,
    pub meta: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentsRing {
    capacity: usize,
    buffer: VecDeque<Intent>,
}

impl Default for IntentsRing {
    fn default() -> Self {
        IntentsRing {
            capacity: 32,
            buffer: VecDeque::new(),
        }
    }
}

impl IntentsRing {
    pub fn with_capacity(capacity: usize) -> Self {
        IntentsRing {
            capacity: capacity.max(1),
            buffer: VecDeque::new(),
        }
    }

    pub fn push(&mut self, intent: Intent) {
        if self.buffer.len() == self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(intent);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Intent> {
        self.buffer.iter()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoState {
    pub last: IntentsRing,
    pub mood: f32,
    pub questions: Vec<String>,
}

impl Default for EchoState {
    fn default() -> Self {
        EchoState {
            last: IntentsRing::default(),
            mood: 0.0,
            questions: Vec::new(),
        }
    }
}

pub fn route_intent(field: &mut ClusterField, intent: Intent) -> EchoEvent {
    use IntentKind::*;

    let note = match intent.kind {
        Boost => format!("boosted {}", intent.target),
        Deprioritize => format!("deprioritized {}", intent.target),
        Snapshot => format!("snapshot requested: {}", intent.target),
        DreamNow => {
            if let Some(report) = field.run_dream_cycle() {
                format!(
                    "dream adjustments s:{} w:{}",
                    report.strengthened, report.weakened
                )
            } else {
                "dream skipped".to_string()
            }
        }
        SyncNow => "sync scheduled".to_string(),
        AwakenNow => {
            let report = field.awaken_now();
            format!(
                "awaken applied {} protected {}",
                report.applied, report.protected
            )
        }
        SetTarget => {
            if let Some(value) = intent
                .args
                .get("load")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
            {
                field.trs.set_target(value);
                format!("target load set to {:.2}", field.trs.target_load)
            } else if let Ok(value) = intent.target.parse::<f32>() {
                field.trs.set_target(value);
                format!("target load set to {:.2}", field.trs.target_load)
            } else {
                "missing load argument".to_string()
            }
        }
        Query => "status requested".to_string(),
        Explain => format!("explain {}", intent.target),
        Ask => "question noted".to_string(),
    };

    EchoEvent {
        ev: "echo".to_string(),
        meta: json!({
            "ok": true,
            "kind": format!("{:?}", intent.kind),
            "note": note,
        }),
    }
}
