use std::collections::HashMap;

use serde::Serialize;

use crate::types::NodeId;

pub type ViewId = u64;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ViewFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_salience: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adreno: Option<bool>,
}

impl Default for ViewFilter {
    fn default() -> Self {
        ViewFilter {
            min_strength: None,
            min_salience: None,
            adreno: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NodeHitStat {
    pub id: NodeId,
    pub hits: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ViewStats {
    pub count: u32,
    pub avg_strength: f32,
    pub avg_latency: f32,
    pub top_nodes: Vec<NodeHitStat>,
    pub emotional_load: f32,
}

impl Default for ViewStats {
    fn default() -> Self {
        ViewStats {
            count: 0,
            avg_strength: 0.0,
            avg_latency: 0.0,
            top_nodes: Vec::new(),
            emotional_load: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct View {
    pub id: ViewId,
    pub pattern: String,
    pub window_ms: u32,
    pub every_ms: u32,
    pub last_emit: u64,
    pub filter: ViewFilter,
}

impl View {
    fn should_emit(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_emit) >= self.every_ms as u64
    }
}

#[derive(Debug, Default)]
pub struct ViewRegistry {
    next_id: ViewId,
    views: HashMap<ViewId, View>,
}

impl ViewRegistry {
    pub fn new() -> Self {
        ViewRegistry {
            next_id: 1,
            views: HashMap::new(),
        }
    }

    pub fn add_view(
        &mut self,
        pattern: String,
        window_ms: u32,
        every_ms: u32,
        now_ms: u64,
        filter: ViewFilter,
    ) -> ViewId {
        let id = self.next_id;
        self.next_id += 1;
        let last_emit = now_ms.saturating_sub(every_ms as u64);
        let view = View {
            id,
            pattern,
            window_ms,
            every_ms,
            last_emit,
            filter,
        };
        self.views.insert(id, view);
        id
    }

    pub fn remove_view(&mut self, id: ViewId) -> bool {
        self.views.remove(&id).is_some()
    }

    pub fn take_due(&mut self, now_ms: u64) -> Vec<View> {
        let mut due = Vec::new();
        for view in self.views.values_mut() {
            if view.should_emit(now_ms) {
                view.last_emit = now_ms;
                due.push(view.clone());
            }
        }
        due
    }

    pub fn build_event(view: &View, stats: &ViewStats) -> String {
        serde_json::json!({
            "ev": "view",
            "meta": {
                "id": view.id,
                "pattern": view.pattern,
                "window": view.window_ms,
                "every": view.every_ms,
                "filter": &view.filter,
                "stats": stats,
            }
        })
        .to_string()
    }

    pub fn list(&self) -> Vec<View> {
        let mut list = self.views.values().cloned().collect::<Vec<_>>();
        list.sort_by_key(|view| view.id);
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_tick_emits_when_due() {
        let mut registry = ViewRegistry::new();
        let now = 1_000u64;
        let id = registry.add_view("cpu".into(), 1_000, 500, now, ViewFilter::default());
        assert_eq!(id, 1);
        let due = registry.take_due(now + 500);
        assert_eq!(due.len(), 1);
        let stats = ViewStats {
            count: 3,
            avg_strength: 0.5,
            avg_latency: 100.0,
            top_nodes: vec![NodeHitStat { id: 1, hits: 3 }],
            emotional_load: 0.5,
        };
        let event = ViewRegistry::build_event(&due[0], &stats);
        assert!(event.contains("\"ev\":\"view\""));
        let next = registry.take_due(now + 700);
        assert!(next.is_empty());
    }
}
