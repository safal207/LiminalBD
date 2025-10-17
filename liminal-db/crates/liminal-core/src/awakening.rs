use serde::{Deserialize, Serialize};

pub const RESONANT_SNAPSHOT_LIMIT: usize = 16;
pub const SYNCLOG_SNAPSHOT_LIMIT: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AwakeningConfig {
    pub enabled: bool,
    pub resonance_threshold: f32,
    pub max_nodes: u32,
}

impl Default for AwakeningConfig {
    fn default() -> Self {
        AwakeningConfig {
            enabled: false,
            resonance_threshold: 0.5,
            max_nodes: 64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelFrame {
    pub hash: String,
    pub cell_count: u32,
    pub created_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResonantModel {
    frames: Vec<ModelFrame>,
    last_applied: Option<String>,
}

impl ResonantModel {
    pub const MAX_FRAMES: usize = 64;

    pub fn frames(&self) -> &[ModelFrame] {
        &self.frames
    }

    pub fn last_hash(&self) -> Option<&str> {
        self.frames.last().map(|frame| frame.hash.as_str())
    }

    pub fn last_applied(&self) -> Option<&str> {
        self.last_applied.as_deref()
    }

    pub fn record_build(&mut self, frame: ModelFrame) {
        self.frames.push(frame);
        self.enforce_limit();
    }

    pub fn set_last_applied(&mut self, hash: Option<String>) {
        self.last_applied = hash;
    }

    pub fn truncated(&self, limit: usize) -> Self {
        if self.frames.len() <= limit {
            return self.clone();
        }
        let mut clone = self.clone();
        let keep_start = clone.frames.len().saturating_sub(limit);
        clone.frames = clone.frames.split_off(keep_start);
        clone
    }

    fn enforce_limit(&mut self) {
        if self.frames.len() > Self::MAX_FRAMES {
            let excess = self.frames.len() - Self::MAX_FRAMES;
            self.frames.drain(0..excess);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncAction {
    Build,
    Apply,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncEntry {
    pub hash: String,
    pub action: SyncAction,
    pub now_ms: u64,
    pub cell_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SyncLog {
    entries: Vec<SyncEntry>,
    #[serde(default)]
    last_tick_ms: u64,
}

impl SyncLog {
    pub const MAX_ENTRIES: usize = 256;

    pub fn entries(&self) -> &[SyncEntry] {
        &self.entries
    }

    pub fn last_tick_ms(&self) -> u64 {
        self.last_tick_ms
    }

    pub fn record_build(&mut self, hash: String, now_ms: u64, cell_count: u32) {
        self.entries.push(SyncEntry {
            hash,
            action: SyncAction::Build,
            now_ms,
            cell_count,
        });
        self.enforce_limit();
    }

    pub fn record_apply(&mut self, hash: String, now_ms: u64, cell_count: u32) {
        self.entries.push(SyncEntry {
            hash,
            action: SyncAction::Apply,
            now_ms,
            cell_count,
        });
        self.enforce_limit();
    }

    pub fn record_tick(&mut self, now_ms: u64) {
        self.last_tick_ms = now_ms;
    }

    pub fn truncated(&self, limit: usize) -> Self {
        if self.entries.len() <= limit {
            return self.clone();
        }
        let mut clone = self.clone();
        let keep_start = clone.entries.len().saturating_sub(limit);
        clone.entries = clone.entries.split_off(keep_start);
        clone
    }

    fn enforce_limit(&mut self) {
        if self.entries.len() > Self::MAX_ENTRIES {
            let excess = self.entries.len() - Self::MAX_ENTRIES;
            self.entries.drain(0..excess);
        }
    }
}
