use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use blake3::Hasher;
use rand::Rng;
use serde::Serialize;
use serde_json::json;

use crate::awakening::{AwakeningConfig, ModelFrame, ResonantModel, SyncLog};
use crate::dream_engine::{run_dream, DreamConfig, DreamReport, PairStat};
use crate::journal::{
    AffinityDelta, AwakenApplyDelta, AwakenTickDelta, CellSnapshot, CollectiveDreamReportDelta,
    DivideDelta, DreamEdgeDelta, DreamLinkDelta, DreamReportDelta, EnergyDelta, EventDelta,
    Journal, LinkDelta, ModelBuildDelta, StateDelta, SyncAlignDelta, SyncShareDelta, TickDelta,
    TrsHarmonyDelta, TrsTargetDelta, TrsTraceDelta,
};
use crate::lql::{
    LqlAst, LqlExecResult, LqlResponse, LqlResult, LqlSelectResult, LqlSubscribeResult,
    LqlUnsubscribeResult,
};
use crate::node_cell::NodeCell;
use crate::reflex::{ReflexAction, ReflexEngine, ReflexFire, ReflexId, ReflexRule};
use crate::seed::{create_seed, SeedParams};
use crate::symmetry::HarmonySnapshot;
use crate::synchrony::{SyncConfig, SyncReport};
use crate::trs::{TrsConfig, TrsOutput, TrsState};
use crate::types::{Impulse, ImpulseKind, NodeId, NodeState};
use crate::views::{NodeHitStat, ViewFilter, ViewRegistry, ViewStats};

pub type FieldEvents = Vec<String>;

pub struct ClusterField {
    pub cells: HashMap<NodeId, NodeCell>,
    pub index: HashMap<String, Vec<NodeId>>,
    pub now_ms: u64,
    pub next_id: NodeId,
    pub rules: HashMap<ReflexId, ReflexRule>,
    pub next_reflex_id: ReflexId,
    pub counters: HashMap<(String, ImpulseKind), VecDeque<(u64, f32)>>,
    token_hits: HashMap<String, VecDeque<Hit>>,
    journal: Option<Arc<dyn Journal + Send + Sync>>,
    ticks_since_impulse: u64,
    link_usage: HashMap<(NodeId, NodeId), LinkStat>,
    link_scores: HashMap<(NodeId, NodeId), f32>,
    route_usage: HashMap<(NodeId, NodeId), RouteStat>,
    route_bias: HashMap<(String, NodeId), f32>,
    recent_routes: HashMap<String, VecDeque<RecentRoute>>,
    last_dream_ms: u64,
    dream_cfg: DreamConfig,
    dream_reports: VecDeque<(u64, DreamReport)>,
    sync_cfg: SyncConfig,
    sync_reports: VecDeque<(u64, SyncReport)>,
    pub trs: TrsState,
    awakening_cfg: AwakeningConfig,
    resonant_model: ResonantModel,
    sync_log: SyncLog,
    last_awaken_tick: u64,
    harmony: HarmonyTuning,
    pub views: ViewRegistry,
    next_hit_seq: u64,
    view_tick_accum: u64,
    reflex_engine: ReflexEngine,
    important_cells: HashSet<NodeId>,
    noradrenaline: Option<NoradrenalineEffect>,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub seq: u64,
    pub ts: u64,
    pub node: NodeId,
    pub strength: f32,
    pub latency_ms: u32,
}

#[derive(Debug, Clone)]
struct RecentRoute {
    ts: u64,
    node: NodeId,
    strength: f32,
}

const MAX_TOKEN_HITS: usize = 512;
const HIT_RETENTION_MS: u64 = 120_000;
const DEFAULT_WINDOW_MS: u32 = 1_000;
const DEFAULT_VIEW_EVERY_MS: u32 = 1_000;
const MAX_RECENT_ROUTES: usize = 96;
const ROUTE_RETENTION_MS: u64 = 90_000;

#[derive(Debug, Clone, Copy)]
pub struct HarmonyTuning {
    pub alpha: f32,
    pub affinity_scale: f32,
    pub metabolism_scale: f32,
    pub sleep_delta: f32,
}

#[derive(Debug, Clone, Copy)]
struct NoradrenalineEffect {
    until_ms: u64,
    strength: f32,
}

impl Default for HarmonyTuning {
    fn default() -> Self {
        HarmonyTuning {
            alpha: 0.2,
            affinity_scale: 1.0,
            metabolism_scale: 1.0,
            sleep_delta: 0.0,
        }
    }
}

impl ClusterField {
    pub fn new() -> Self {
        ClusterField {
            cells: HashMap::new(),
            index: HashMap::new(),
            now_ms: 0,
            next_id: 1,
            rules: HashMap::new(),
            next_reflex_id: 1,
            counters: HashMap::new(),
            token_hits: HashMap::new(),
            journal: None,
            ticks_since_impulse: 0,
            link_usage: HashMap::new(),
            link_scores: HashMap::new(),
            route_usage: HashMap::new(),
            route_bias: HashMap::new(),
            recent_routes: HashMap::new(),
            last_dream_ms: 0,
            dream_cfg: DreamConfig::default(),
            dream_reports: VecDeque::new(),
            sync_cfg: SyncConfig::default(),
            sync_reports: VecDeque::new(),
            trs: TrsState::default(),
            awakening_cfg: AwakeningConfig::default(),
            resonant_model: ResonantModel::default(),
            sync_log: SyncLog::default(),
            last_awaken_tick: 0,
            harmony: HarmonyTuning::default(),
            views: ViewRegistry::new(),
            next_hit_seq: 1,
            view_tick_accum: 0,
            reflex_engine: ReflexEngine::new(2_000),
            important_cells: HashSet::new(),
            noradrenaline: None,
        }
    }

    pub fn with_journal(mut self, journal: Arc<dyn Journal + Send + Sync>) -> Self {
        self.journal = Some(journal);
        self
    }

    pub fn set_journal(&mut self, journal: Arc<dyn Journal + Send + Sync>) {
        self.journal = Some(journal);
    }

    pub fn clear_journal(&mut self) {
        self.journal = None;
    }

    pub fn dream_config(&self) -> DreamConfig {
        self.dream_cfg.clone()
    }

    pub fn set_dream_config(&mut self, cfg: DreamConfig) {
        self.dream_cfg = cfg.clone();
        self.emit(EventDelta::DreamConfig(cfg));
    }

    pub fn run_dream_cycle(&mut self) -> Option<DreamReport> {
        if self.cells.len() < 2 {
            return None;
        }
        let cfg = self.dream_cfg.clone();
        let report = run_dream(self, &cfg, self.now_ms);
        self.last_dream_ms = self.now_ms;
        Some(report)
    }

    pub fn last_dream_at(&self) -> u64 {
        self.last_dream_ms
    }

    pub fn sync_config(&self) -> SyncConfig {
        self.sync_cfg.clone()
    }

    pub fn set_sync_config(&mut self, cfg: SyncConfig) {
        let mut cfg = cfg;
        cfg.weight_xfer = cfg.weight_xfer.clamp(0.05, 0.5);
        self.sync_cfg = cfg.clone();
        self.emit(EventDelta::SyncConfig(cfg));
    }

    pub fn awakening_config(&self) -> AwakeningConfig {
        self.awakening_cfg.clone()
    }

    pub fn set_awakening_config(&mut self, cfg: AwakeningConfig) {
        self.awakening_cfg = cfg;
    }

    pub fn resonant_model(&self) -> &ResonantModel {
        &self.resonant_model
    }

    pub fn sync_log(&self) -> &SyncLog {
        &self.sync_log
    }

    pub fn last_awaken_tick(&self) -> u64 {
        self.last_awaken_tick
    }

    pub fn restore_awakening_state(
        &mut self,
        cfg: AwakeningConfig,
        model: ResonantModel,
        sync_log: SyncLog,
        last_tick: u64,
    ) {
        self.awakening_cfg = cfg;
        self.resonant_model = model;
        self.sync_log = sync_log;
        self.last_awaken_tick = last_tick;
    }

    pub fn build_resonant_model(&mut self) -> String {
        let mut hasher = Hasher::new();
        let mut ids: Vec<NodeId> = self.cells.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(cell) = self.cells.get(&id) {
                let data = format!(
                    "{}|{}|{:?}|{:.3}|{:.3}|{:.3}|{}",
                    cell.id,
                    cell.seed.core_pattern,
                    cell.state,
                    cell.affinity,
                    cell.metabolism,
                    cell.energy,
                    cell.links.len()
                );
                hasher.update(data.as_bytes());
            }
        }
        hasher.update(&self.now_ms.to_le_bytes());
        hasher.update(&(self.cells.len() as u64).to_le_bytes());
        let hash = hasher.finalize().to_hex().to_string();
        let cell_count = self.cells.len().min(u32::MAX as usize) as u32;
        let frame = ModelFrame {
            hash: hash.clone(),
            cell_count,
            created_ms: self.now_ms,
        };
        self.resonant_model.record_build(frame.clone());
        self.sync_log
            .record_build(frame.hash.clone(), frame.created_ms, frame.cell_count);
        let delta = EventDelta::ModelBuild(ModelBuildDelta {
            hash: frame.hash.clone(),
            cell_count: frame.cell_count,
            now_ms: frame.created_ms,
        });
        self.emit(delta);
        hash
    }

    pub fn apply_awakening_model(&mut self) {
        let hash = self.resonant_model.last_hash().map(|h| h.to_string());
        let cell_count = self.cells.len().min(u32::MAX as usize) as u32;
        if let Some(hash_value) = &hash {
            self.sync_log
                .record_apply(hash_value.clone(), self.now_ms, cell_count);
        }
        self.resonant_model.set_last_applied(hash.clone());
        let delta = EventDelta::AwakenApply(AwakenApplyDelta {
            hash,
            config: self.awakening_cfg.clone(),
            now_ms: self.now_ms,
            cell_count,
        });
        self.emit(delta);
    }

    pub fn awaken_tick(&mut self) {
        self.last_awaken_tick = self.now_ms;
        self.sync_log.record_tick(self.now_ms);
        let hash = self.resonant_model.last_applied().map(|h| h.to_string());
        let delta = EventDelta::AwakenTick(AwakenTickDelta {
            hash,
            now_ms: self.now_ms,
        });
        self.emit(delta);
    }

    pub fn last_sync_reports(&self, count: usize) -> Vec<(u64, SyncReport)> {
        let mut collected: Vec<(u64, SyncReport)> = self
            .sync_reports
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect();
        collected.reverse();
        collected
    }

    pub fn last_dream_reports(&self, count: usize) -> Vec<(u64, DreamReport)> {
        let mut collected: Vec<(u64, DreamReport)> = self
            .dream_reports
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect();
        collected.reverse();
        collected
    }

    pub fn cell(&self, id: NodeId) -> Option<&NodeCell> {
        self.cells.get(&id)
    }

    fn record_token_hit(&mut self, token: &str, node: NodeId, strength: f32, latency_ms: u32) {
        let normalized = token.to_lowercase();
        let seq = self.next_hit_seq;
        self.next_hit_seq = self.next_hit_seq.saturating_add(1);
        let entry = self
            .token_hits
            .entry(normalized)
            .or_insert_with(VecDeque::new);
        entry.push_back(Hit {
            seq,
            ts: self.now_ms,
            node,
            strength,
            latency_ms,
        });
        while let Some(front) = entry.front() {
            if self.now_ms.saturating_sub(front.ts) > HIT_RETENTION_MS {
                entry.pop_front();
            } else {
                break;
            }
        }
        while entry.len() > MAX_TOKEN_HITS {
            entry.pop_front();
        }
    }

    pub fn recent_for(&self, token: &str, window_ms: u32) -> Vec<Hit> {
        let normalized = token.to_lowercase();
        let Some(queue) = self.token_hits.get(&normalized) else {
            return Vec::new();
        };
        let cutoff = self.now_ms.saturating_sub(window_ms as u64);
        let mut hits: Vec<Hit> = queue
            .iter()
            .rev()
            .take_while(|hit| hit.ts >= cutoff)
            .cloned()
            .collect();
        hits.reverse();
        hits
    }

    pub fn important_cells(&self) -> &HashSet<NodeId> {
        &self.important_cells
    }

    pub fn partial_snapshot(&self, ids: &HashSet<NodeId>) -> Vec<u8> {
        #[derive(Serialize)]
        struct PartialSnapshotEnvelope {
            now_ms: u64,
            cells: Vec<CellSnapshot>,
        }

        let mut cells = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(cell) = self.cells.get(id) {
                cells.push(CellSnapshot::from(cell));
            }
        }
        let envelope = PartialSnapshotEnvelope {
            now_ms: self.now_ms,
            cells,
        };
        serde_cbor::to_vec(&envelope).unwrap_or_default()
    }

    fn noradrenaline_active(&self) -> Option<NoradrenalineEffect> {
        self.noradrenaline.and_then(|effect| {
            if self.now_ms < effect.until_ms {
                Some(effect)
            } else {
                None
            }
        })
    }

    fn affinity_scale_for(&self, cell: &NodeCell) -> f32 {
        let mut scale = self.harmony.affinity_scale;
        if cell.adreno_tag {
            if let Some(effect) = self.noradrenaline_active() {
                scale *= 1.0 + 0.25 * effect.strength;
            }
        }
        scale
    }

    fn refresh_important_cells(&mut self) {
        let mut next: HashSet<NodeId> = HashSet::new();
        for (id, cell) in &self.cells {
            if cell.salience >= 0.7 {
                let since_recall = self.now_ms.saturating_sub(cell.last_recall_ms);
                if since_recall <= 10_000 {
                    next.insert(*id);
                }
            }
        }
        self.important_cells = next;
    }

    fn apply_filters(&self, hits: &mut Vec<Hit>, filter: &ViewFilter) {
        if let Some(min_strength) = filter.min_strength {
            hits.retain(|hit| hit.strength >= min_strength);
        }
        if let Some(min_salience) = filter.min_salience {
            hits.retain(|hit| {
                self.cells
                    .get(&hit.node)
                    .map(|cell| cell.salience >= min_salience)
                    .unwrap_or(false)
            });
        }
        if let Some(adreno) = filter.adreno {
            hits.retain(|hit| {
                self.cells
                    .get(&hit.node)
                    .map(|cell| cell.adreno_tag == adreno)
                    .unwrap_or(false)
            });
        }
    }

    fn gather_hits_for_pattern(&self, pattern: &str, window_ms: u32) -> Vec<Hit> {
        let tokens = tokenize(pattern);
        if tokens.is_empty() {
            return Vec::new();
        }
        let mut combined: Vec<Hit> = Vec::new();
        let mut seen: HashSet<u64> = HashSet::new();
        for token in tokens {
            for hit in self.recent_for(&token, window_ms) {
                if seen.insert(hit.seq) {
                    combined.push(hit);
                }
            }
        }
        combined.sort_by_key(|hit| hit.ts);
        combined
    }

    fn stats_from_hits(&self, hits: &[Hit]) -> ViewStats {
        if hits.is_empty() {
            return ViewStats::default();
        }
        let count = hits.len() as u32;
        let total_strength: f32 = hits.iter().map(|hit| hit.strength).sum();
        let total_latency: f32 = hits.iter().map(|hit| hit.latency_ms as f32).sum();
        let emotional_hits = hits
            .iter()
            .filter(|hit| {
                self.cells
                    .get(&hit.node)
                    .map(|cell| cell.adreno_tag)
                    .unwrap_or(false)
            })
            .count() as f32;
        let mut node_counts: HashMap<NodeId, u32> = HashMap::new();
        for hit in hits {
            *node_counts.entry(hit.node).or_insert(0) += 1;
        }
        let mut top_nodes: Vec<NodeHitStat> = node_counts
            .into_iter()
            .map(|(id, hits)| NodeHitStat { id, hits })
            .collect();
        top_nodes.sort_by(|a, b| b.hits.cmp(&a.hits).then_with(|| a.id.cmp(&b.id)));
        top_nodes.truncate(3);
        ViewStats {
            count,
            avg_strength: (total_strength / count as f32).clamp(0.0, 1.0),
            avg_latency: total_latency / count as f32,
            top_nodes,
            emotional_load: if count == 0 {
                0.0
            } else {
                (emotional_hits / count as f32).clamp(0.0, 1.0)
            },
        }
    }

    pub fn stats_for_pattern(
        &self,
        pattern: &str,
        window_ms: u32,
        filter: &ViewFilter,
    ) -> ViewStats {
        let window = window_ms.max(1);
        let mut hits = self.gather_hits_for_pattern(pattern, window);
        self.apply_filters(&mut hits, filter);
        self.stats_from_hits(&hits)
    }

    pub fn symmetry_samples(&self, window_ms: u32) -> Vec<(String, Hit)> {
        let cutoff = self.now_ms.saturating_sub(window_ms as u64);
        let mut samples: Vec<(String, Hit)> = Vec::new();
        for (token, queue) in &self.token_hits {
            for hit in queue.iter().rev() {
                if hit.ts < cutoff {
                    break;
                }
                samples.push((token.clone(), hit.clone()));
            }
        }
        samples
    }

    pub fn exec_lql(&mut self, ast: LqlAst) -> LqlResult {
        match ast {
            LqlAst::Select {
                pattern,
                window_ms,
                filter,
            } => {
                let window = window_ms.unwrap_or(DEFAULT_WINDOW_MS).max(1);
                let stats = self.stats_for_pattern(&pattern, window, &filter);
                let payload = LqlSelectResult {
                    pattern: pattern.clone(),
                    window_ms: window,
                    min_strength: filter.min_strength,
                    min_salience: filter.min_salience,
                    adreno: filter.adreno,
                    stats: stats.clone(),
                };
                let event = json!({
                    "ev": "lql",
                    "meta": {
                        "select": payload,
                    }
                })
                .to_string();
                Ok(LqlExecResult {
                    response: Some(LqlResponse::Select(payload)),
                    events: vec![event],
                })
            }
            LqlAst::Subscribe {
                pattern,
                window_ms,
                every_ms,
                filter,
            } => {
                let window = window_ms.unwrap_or(DEFAULT_WINDOW_MS).max(1);
                let every = every_ms.unwrap_or(DEFAULT_VIEW_EVERY_MS).max(200);
                let id = self.views.add_view(
                    pattern.clone(),
                    window,
                    every,
                    self.now_ms,
                    filter.clone(),
                );
                let payload = LqlSubscribeResult {
                    id,
                    pattern: pattern.clone(),
                    window_ms: window,
                    every_ms: every,
                    min_strength: filter.min_strength,
                    min_salience: filter.min_salience,
                    adreno: filter.adreno,
                };
                let event = json!({
                    "ev": "lql",
                    "meta": {
                        "subscribe": payload,
                    }
                })
                .to_string();
                Ok(LqlExecResult {
                    response: Some(LqlResponse::Subscribe(payload)),
                    events: vec![event],
                })
            }
            LqlAst::Unsubscribe { id } => {
                let removed = self.views.remove_view(id);
                let payload = LqlUnsubscribeResult { id, removed };
                let event = json!({
                    "ev": "lql",
                    "meta": {
                        "unsubscribe": payload,
                    }
                })
                .to_string();
                Ok(LqlExecResult {
                    response: Some(LqlResponse::Unsubscribe(payload)),
                    events: vec![event],
                })
            }
        }
    }

    pub fn rebuild_caches(&mut self) {
        self.rebuild_index();
    }

    pub fn harmony_state(&self) -> HarmonyTuning {
        self.harmony
    }

    pub fn trs_config(&self) -> TrsConfig {
        self.trs.to_config()
    }

    pub fn set_trs_config(&mut self, cfg: TrsConfig) -> String {
        self.trs.apply_config(&cfg);
        self.harmony.alpha = self.trs.alpha;
        self.emit(EventDelta::TrsSet(cfg.clone()));
        json!({
            "ev": "trs_config",
            "meta": {
                "alpha": cfg.alpha,
                "beta": cfg.beta,
                "k_p": cfg.k_p,
                "k_i": cfg.k_i,
                "k_d": cfg.k_d,
                "target": cfg.target_load
            }
        })
        .to_string()
    }

    pub fn set_trs_target(&mut self, target: f32) -> String {
        self.trs.set_target(target);
        let applied = self.trs.target_load;
        self.emit(EventDelta::TrsTarget(TrsTargetDelta {
            target_load: applied,
        }));
        json!({
            "ev": "trs_target",
            "meta": { "target": applied }
        })
        .to_string()
    }

    pub fn apply_trs_output(
        &mut self,
        now_ms: u64,
        observed_load: f32,
        output: &TrsOutput,
    ) -> Vec<String> {
        self.harmony.alpha = output.alpha_new.clamp(0.05, 0.95);
        self.harmony.affinity_scale = output.affinity_scale.clamp(0.7, 1.35);
        self.harmony.metabolism_scale = output.metabolism_scale.clamp(0.5, 1.5);
        self.harmony.sleep_delta = output.sleep_threshold_delta.clamp(-0.2, 0.2);

        let trace = json!({
            "ev": "trs_trace",
            "meta": {
                "alpha": self.harmony.alpha,
                "err": self.trs.last_err,
                "observed": observed_load,
                "tick_adj": output.tick_adjust_ms
            }
        })
        .to_string();
        self.emit(EventDelta::TrsTrace(TrsTraceDelta {
            now_ms,
            observed_load,
            error: self.trs.last_err,
            tick_adjust_ms: output.tick_adjust_ms,
            alpha: self.harmony.alpha,
        }));

        let harmony = json!({
            "ev": "harmony",
            "meta": {
                "alpha": self.harmony.alpha,
                "aff_scale": self.harmony.affinity_scale,
                "met_scale": self.harmony.metabolism_scale,
                "sleep_delta": self.harmony.sleep_delta
            }
        })
        .to_string();
        self.emit(EventDelta::TrsHarmony(TrsHarmonyDelta {
            alpha: self.harmony.alpha,
            affinity_scale: self.harmony.affinity_scale,
            metabolism_scale: self.harmony.metabolism_scale,
            sleep_delta: self.harmony.sleep_delta,
        }));

        vec![trace, harmony]
    }

    fn emit(&self, delta: EventDelta) {
        if let Some(journal) = &self.journal {
            journal.append_delta(&delta);
        }
    }

    fn attach_cell(&mut self, cell: NodeCell) {
        let id = cell.id;
        self.cells.insert(id, cell);
        self.update_index_for(id);
        self.ensure_link_tracking(id);
    }

    fn update_index_for(&mut self, id: NodeId) {
        let Some(cell) = self.cells.get(&id) else {
            return;
        };
        for ids in self.index.values_mut() {
            ids.retain(|existing| *existing != id);
        }
        for token in tokenize(&cell.seed.core_pattern) {
            self.index.entry(token).or_default().push(id);
        }
    }

    fn ensure_link_tracking(&mut self, id: NodeId) {
        if let Some(links) = self
            .cells
            .get(&id)
            .map(|cell| cell.links.iter().copied().collect::<Vec<_>>())
        {
            for to in links {
                self.register_link(id, to);
            }
        }
    }

    fn register_link(&mut self, from: NodeId, to: NodeId) {
        self.link_usage
            .entry((from, to))
            .or_insert_with(|| LinkStat {
                hits: 0,
                last_used_ms: self.now_ms,
            });
        self.link_scores.entry((from, to)).or_insert(1.0);
    }

    fn unregister_link(&mut self, from: NodeId, to: NodeId) {
        self.link_usage.remove(&(from, to));
        self.link_scores.remove(&(from, to));
    }

    fn record_link_hit(&mut self, from: NodeId, to: NodeId) {
        let stat = self
            .link_usage
            .entry((from, to))
            .or_insert_with(LinkStat::default);
        stat.hits = stat.hits.saturating_add(1);
        stat.last_used_ms = self.now_ms;
    }

    fn record_route_pair(&mut self, from: NodeId, to: NodeId) {
        let stat = self
            .route_usage
            .entry((from, to))
            .or_insert_with(RouteStat::default);
        stat.hits = stat.hits.saturating_add(1);
        stat.last_used_ms = self.now_ms;
        if self.route_usage.len() > 2048 {
            if let Some(candidate) = self
                .route_usage
                .iter()
                .min_by(|a, b| {
                    let (stat_a, stat_b) = (a.1, b.1);
                    match stat_a.hits.cmp(&stat_b.hits) {
                        Ordering::Equal => stat_a.last_used_ms.cmp(&stat_b.last_used_ms),
                        other => other,
                    }
                })
                .map(|(key, _)| *key)
            {
                self.route_usage.remove(&candidate);
            }
        }
    }

    fn purge_tracking_for(&mut self, id: NodeId) {
        self.link_usage
            .retain(|(from, to), _| *from != id && *to != id);
        self.route_usage
            .retain(|(from, to), _| *from != id && *to != id);
        self.link_scores
            .retain(|(from, to), _| *from != id && *to != id);
    }

    fn record_route_sequence(&mut self, sequence: &[NodeId]) {
        for pair in sequence.windows(2) {
            let from = pair[0];
            let to = pair[1];
            if let Some(cell) = self.cells.get(&from) {
                if cell.links.contains(&to) {
                    self.record_link_hit(from, to);
                }
            }
            self.record_route_pair(from, to);
        }
    }

    pub(crate) fn adjust_link_score(&mut self, from: NodeId, to: NodeId, delta: f32) -> bool {
        let Some(cell) = self.cells.get(&from) else {
            return false;
        };
        if !cell.links.contains(&to) {
            return false;
        }
        let entry = self.link_scores.entry((from, to)).or_insert(1.0);
        let updated = (*entry + delta).clamp(0.2, 3.0);
        let changed = (updated - *entry).abs() > f32::EPSILON;
        *entry = updated;
        changed
    }

    pub(crate) fn link_score(&self, from: NodeId, to: NodeId) -> f32 {
        self.link_scores.get(&(from, to)).copied().unwrap_or(1.0)
    }

    pub(crate) fn prune_link(&mut self, from: NodeId, to: NodeId) -> bool {
        let Some(cell) = self.cells.get_mut(&from) else {
            return false;
        };
        if cell.links.remove(&to) {
            self.unregister_link(from, to);
            self.emit(EventDelta::Unlink(LinkDelta { from, to }));
            true
        } else {
            false
        }
    }

    pub(crate) fn rewire_lonely(&mut self, id: NodeId) -> Option<u32> {
        let needs_rewire = {
            let cell = self.cells.get(&id)?;
            cell.links.len() < 1
        };
        if !needs_rewire {
            return Some(0);
        }
        let (affinity, existing) = {
            let cell = self.cells.get(&id)?;
            (cell.affinity, cell.links.clone())
        };
        let mut best: Option<(NodeId, f32)> = None;
        for (candidate_id, candidate) in &self.cells {
            if *candidate_id == id || existing.contains(candidate_id) {
                continue;
            }
            let distance = (candidate.affinity - affinity).abs();
            let replace = best
                .as_ref()
                .map(|(_, current_dist)| distance < *current_dist)
                .unwrap_or(true);
            if replace {
                best = Some((*candidate_id, distance));
            }
        }
        let Some((target, _)) = best else {
            return Some(0);
        };
        {
            let cell = self.cells.get_mut(&id)?;
            if !cell.links.insert(target) {
                return Some(0);
            }
        }
        self.register_link(id, target);
        self.emit(EventDelta::Link(LinkDelta {
            from: id,
            to: target,
        }));
        self.emit_dream_rewire(id, target);
        Some(1)
    }

    pub(crate) fn emit_dream_strengthen(
        &self,
        from: NodeId,
        to: NodeId,
        freq: f32,
        avg_strength: f32,
    ) {
        self.emit(EventDelta::DreamStrengthen(DreamEdgeDelta {
            from,
            to,
            score: self.link_score(from, to),
            freq,
            avg_strength,
        }));
    }

    pub(crate) fn emit_dream_weaken(&self, from: NodeId, to: NodeId, freq: f32, avg_strength: f32) {
        self.emit(EventDelta::DreamWeaken(DreamEdgeDelta {
            from,
            to,
            score: self.link_score(from, to),
            freq,
            avg_strength,
        }));
    }

    pub(crate) fn emit_dream_prune(&self, from: NodeId, to: NodeId) {
        self.emit(EventDelta::DreamPrune(DreamLinkDelta { from, to }));
    }

    pub(crate) fn emit_dream_rewire(&self, from: NodeId, to: NodeId) {
        self.emit(EventDelta::DreamRewire(DreamLinkDelta { from, to }));
    }

    pub(crate) fn record_dream_report(&mut self, now_ms: u64, report: DreamReport) {
        self.dream_reports.push_back((now_ms, report.clone()));
        while self.dream_reports.len() > 16 {
            self.dream_reports.pop_front();
        }
        self.emit(EventDelta::DreamReport(DreamReportDelta { now_ms, report }));
    }

    pub(crate) fn record_sync_report(&mut self, now_ms: u64, report: SyncReport) {
        self.sync_reports.push_back((now_ms, report.clone()));
        while self.sync_reports.len() > 16 {
            self.sync_reports.pop_front();
        }
        self.emit(EventDelta::CollectiveDreamReport(
            CollectiveDreamReportDelta { now_ms, report },
        ));
    }

    pub fn community_centroid(&self, nodes: &[NodeId]) -> f32 {
        if nodes.is_empty() {
            return 0.0;
        }
        let mut total = 0.0f32;
        let mut count = 0u32;
        for node in nodes {
            if let Some(cell) = self.cells.get(node) {
                total += cell.affinity;
                count = count.saturating_add(1);
            }
        }
        if count == 0 {
            0.0
        } else {
            total / (count as f32)
        }
    }

    pub(crate) fn sync_top_tokens(&self, nodes: &[NodeId], top_k: usize) -> Vec<String> {
        if nodes.is_empty() || top_k == 0 {
            return Vec::new();
        }
        let focus: HashSet<NodeId> = nodes.iter().copied().collect();
        let mut scores: HashMap<String, f32> = HashMap::new();
        for (token, history) in &self.recent_routes {
            let mut total = 0.0f32;
            let mut hits = 0u32;
            for entry in history.iter().rev().take(64) {
                if focus.contains(&entry.node) {
                    total += entry.strength;
                    hits = hits.saturating_add(1);
                }
            }
            if hits > 0 {
                let score = total + hits as f32 * 0.25;
                scores.insert(token.clone(), score);
            }
        }
        let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let mut selected: Vec<String> = ranked
            .into_iter()
            .take(top_k)
            .map(|(token, _)| token)
            .collect();
        if selected.is_empty() {
            let mut fallback: HashMap<String, u32> = HashMap::new();
            for node in nodes {
                if let Some(cell) = self.cells.get(node) {
                    for token in tokenize(&cell.seed.core_pattern) {
                        *fallback.entry(token).or_insert(0) += 1;
                    }
                }
            }
            let mut ranked_tokens: Vec<(String, u32)> = fallback.into_iter().collect();
            ranked_tokens.sort_by(|a, b| b.1.cmp(&a.1));
            selected = ranked_tokens
                .into_iter()
                .take(top_k)
                .map(|(token, _)| token)
                .collect();
        }
        selected
    }

    pub(crate) fn share_tokens(
        &mut self,
        nodes: &[NodeId],
        tokens: &[String],
        weight_xfer: f32,
    ) -> (u32, u32, u32) {
        if nodes.is_empty() || tokens.is_empty() {
            return (0, 0, 0);
        }
        let norm_weight = weight_xfer.clamp(0.05, 0.5);
        let centroid = self.community_centroid(nodes);
        let normalized_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
        let mut shared = 0u32;
        let mut aligned = 0u32;
        let mut protected = 0u32;
        let mut share_events: Vec<(NodeId, String, f32)> = Vec::new();
        for &node in nodes {
            let node_protected;
            {
                let Some(cell) = self.cells.get_mut(&node) else {
                    continue;
                };
                node_protected = cell.salience >= 0.6 || cell.adreno_tag;
                let diff = centroid - cell.affinity;
                if diff.abs() > 0.002 {
                    cell.affinity = (cell.affinity + diff * 0.08).clamp(0.0, 1.0);
                    aligned = aligned.saturating_add(1);
                }
                if !node_protected {
                    let adjust = if diff < 0.0 {
                        1.0 - norm_weight * 0.05
                    } else {
                        1.0 + norm_weight * 0.05
                    };
                    cell.metabolism = (cell.metabolism * adjust).clamp(0.05, 5.0);
                }
            }
            if node_protected {
                protected = protected.saturating_add(1);
            }
            for token in &normalized_tokens {
                let entry = self.route_bias.entry((token.clone(), node)).or_insert(1.0);
                let updated = (*entry + norm_weight).clamp(0.2, 5.0);
                if (updated - *entry).abs() > f32::EPSILON {
                    *entry = updated;
                    shared = shared.saturating_add(1);
                    share_events.push((node, token.clone(), updated));
                }
            }
        }
        for (idx, &from) in nodes.iter().enumerate() {
            for &to in nodes.iter().skip(idx + 1) {
                if self.adjust_link_score(from, to, norm_weight) {
                    aligned = aligned.saturating_add(1);
                    let score = self.link_score(from, to);
                    self.emit(EventDelta::SyncAlign(SyncAlignDelta { from, to, score }));
                }
                if self.adjust_link_score(to, from, norm_weight) {
                    aligned = aligned.saturating_add(1);
                    let score = self.link_score(to, from);
                    self.emit(EventDelta::SyncAlign(SyncAlignDelta {
                        from: to,
                        to: from,
                        score,
                    }));
                }
            }
        }
        for (node, token, bias) in share_events {
            self.emit(EventDelta::SyncShare(SyncShareDelta { node, token, bias }));
        }
        (shared, aligned, protected)
    }

    pub(crate) fn collect_recent_pairs(&self, window_ms: u32) -> Vec<PairStat> {
        let cutoff = self.now_ms.saturating_sub(window_ms as u64);
        let mut accum: HashMap<(NodeId, NodeId), (u32, f32)> = HashMap::new();
        for history in self.recent_routes.values() {
            let relevant: Vec<&RecentRoute> =
                history.iter().filter(|entry| entry.ts >= cutoff).collect();
            if relevant.len() < 2 {
                continue;
            }
            for (idx, left) in relevant.iter().enumerate() {
                for right in relevant.iter().skip(idx + 1) {
                    if right.ts.saturating_sub(left.ts) > window_ms as u64 {
                        break;
                    }
                    if left.node == right.node {
                        continue;
                    }
                    let (a, b) = if left.node < right.node {
                        (left.node, right.node)
                    } else {
                        (right.node, left.node)
                    };
                    let entry = accum.entry((a, b)).or_insert((0, 0.0));
                    entry.0 = entry.0.saturating_add(1);
                    entry.1 += (left.strength + right.strength) * 0.5;
                }
            }
        }
        accum
            .into_iter()
            .map(|((u, v), (count, total_strength))| PairStat {
                u,
                v,
                freq: count as f32,
                avg_strength: if count > 0 {
                    total_strength / count as f32
                } else {
                    0.0
                },
            })
            .collect()
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for id in self.cells.keys().copied().collect::<Vec<_>>() {
            self.update_index_for(id);
            self.ensure_link_tracking(id);
        }
    }

    pub fn add_root(&mut self, seed: &str) -> NodeId {
        let params = create_seed(seed);
        let id = self.next_id;
        self.next_id += 1;
        let cell = NodeCell::from_seed(id, params);
        self.attach_cell(cell);
        if let Some(cell) = self.cells.get(&id) {
            self.emit(EventDelta::Spawn(CellSnapshot::from(cell)));
        }
        id
    }

    pub fn add_reflex(&mut self, mut rule: ReflexRule) -> ReflexId {
        let id = if rule.id == 0 {
            let id = self.next_reflex_id;
            self.next_reflex_id += 1;
            id
        } else {
            self.next_reflex_id = self.next_reflex_id.max(rule.id + 1);
            rule.id
        };
        rule.id = id;
        rule.when.token = rule.when.token.trim().to_lowercase();
        rule.when.min_strength = rule.when.min_strength.clamp(0.0, 1.0);
        self.rules.insert(id, rule.clone());
        self.emit(EventDelta::ReflexAdd(rule));
        id
    }

    pub fn remove_reflex(&mut self, id: ReflexId) -> bool {
        let removed = self.rules.remove(&id).is_some();
        if removed {
            self.emit(EventDelta::ReflexRemove { id });
        }
        removed
    }

    pub fn list_reflex(&self) -> Vec<ReflexRule> {
        let mut rules = self.rules.values().cloned().collect::<Vec<_>>();
        rules.sort_by_key(|rule| rule.id);
        rules
    }

    fn process_reflex_for(
        &mut self,
        token: &str,
        imp: &Impulse,
        logs: &mut Vec<String>,
        fired: &mut HashSet<ReflexId>,
    ) {
        let normalized = token.to_lowercase();
        let relevant_rules: Vec<ReflexRule> = self
            .rules
            .values()
            .filter(|rule| {
                rule.enabled
                    && rule.when.kind == imp.kind
                    && reflex_token_matches(&rule.when.token, &normalized)
            })
            .cloned()
            .collect();
        let key = (normalized.clone(), imp.kind.clone());
        let retention = relevant_rules
            .iter()
            .map(|rule| rule.when.window_ms as u64)
            .max()
            .unwrap_or(2_000)
            .max(1);
        {
            let queue = self
                .counters
                .entry(key.clone())
                .or_insert_with(VecDeque::new);
            queue.push_back((self.now_ms, imp.strength));
            while let Some((ts, _)) = queue.front() {
                if self.now_ms.saturating_sub(*ts) > retention {
                    queue.pop_front();
                } else {
                    break;
                }
            }
            while queue.len() > 256 {
                queue.pop_front();
            }
        }
        if relevant_rules.is_empty() {
            return;
        }
        let Some(queue_snapshot) = self.counters.get(&key).cloned() else {
            return;
        };
        let mut pending: Vec<(ReflexRule, u16)> = Vec::new();
        for rule in relevant_rules {
            if fired.contains(&rule.id) {
                continue;
            }
            let matched = queue_snapshot
                .iter()
                .filter(|(ts, strength)| {
                    self.now_ms.saturating_sub(*ts) <= rule.when.window_ms as u64
                        && *strength >= rule.when.min_strength
                })
                .count() as u16;
            if matched >= rule.when.min_count {
                pending.push((rule, matched));
            }
        }
        for (rule, matched) in pending {
            self.fire_reflex(&rule, matched, &normalized, logs);
            fired.insert(rule.id);
        }
    }

    fn fire_reflex(
        &mut self,
        rule: &ReflexRule,
        matched: u16,
        token: &str,
        logs: &mut Vec<String>,
    ) {
        let fire = ReflexFire {
            id: rule.id,
            at_ms: self.now_ms,
            matched,
        };
        logs.push(format!("REFLEX_FIRE id={} matched={}", rule.id, matched));
        self.emit(EventDelta::ReflexFire(fire));
        match &rule.then {
            ReflexAction::EmitHint { hint } => {
                logs.push(format!("REFLEX_HINT id={} hint={:?}", rule.id, hint));
            }
            ReflexAction::SpawnSeed {
                seed,
                affinity_shift,
            } => {
                self.perform_spawn_seed(rule.id, seed, *affinity_shift, logs);
            }
            ReflexAction::WakeSleeping { count } => {
                self.perform_wake_sleeping(rule.id, *count, token, logs);
            }
            ReflexAction::BoostLinks { factor, top } => {
                self.perform_boost_links(rule.id, token, *factor, *top, logs);
            }
        }
    }

    fn perform_spawn_seed(
        &mut self,
        rule_id: ReflexId,
        seed: &str,
        affinity_shift: f32,
        logs: &mut Vec<String>,
    ) {
        let mut params = create_seed(seed);
        params.affinity = (params.affinity + affinity_shift).clamp(0.0, 1.0);
        let new_id = self.next_id;
        self.next_id += 1;
        let mut cell = NodeCell::from_seed(new_id, params);
        cell.affinity = cell.affinity.clamp(0.0, 1.0);
        cell.seed.affinity = cell.affinity;
        let snapshot = CellSnapshot::from(&cell);
        self.attach_cell(cell);
        self.emit(EventDelta::Spawn(snapshot));
        logs.push(format!(
            "REFLEX_SPAWN id={} node=n{} seed={} shift={:.2}",
            rule_id, new_id, seed, affinity_shift
        ));
    }

    fn perform_wake_sleeping(
        &mut self,
        rule_id: ReflexId,
        count: u16,
        token: &str,
        logs: &mut Vec<String>,
    ) {
        if count == 0 {
            return;
        }
        let mut sleeping: Vec<(f32, NodeId)> = self
            .cells
            .iter()
            .filter_map(|(id, cell)| {
                if cell.state == NodeState::Sleep {
                    Some((wake_priority(cell, token), *id))
                } else {
                    None
                }
            })
            .collect();
        sleeping.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
        let mut woke = 0u16;
        for (_priority, node_id) in sleeping.into_iter().take(count as usize) {
            if let Some(cell) = self.cells.get_mut(&node_id) {
                if cell.state == NodeState::Sleep {
                    cell.state = NodeState::Idle;
                    cell.energy = cell.energy.max(0.3);
                    cell.last_response_ms = self.now_ms;
                    let delta = EventDelta::Energy(EnergyDelta {
                        id: node_id,
                        energy: cell.energy,
                        metabolism: cell.metabolism,
                        state: cell.state,
                        last_response_ms: cell.last_response_ms,
                    });
                    self.emit(delta);
                    woke = woke.saturating_add(1);
                }
            }
        }
        logs.push(format!(
            "REFLEX_WAKE id={} requested={} woke={}",
            rule_id, count, woke
        ));
    }

    fn perform_boost_links(
        &mut self,
        rule_id: ReflexId,
        token: &str,
        factor: f32,
        top: u16,
        logs: &mut Vec<String>,
    ) {
        if top == 0 {
            logs.push(format!(
                "REFLEX_BOOST id={} top={} factor={:.2} boosted=0",
                rule_id, top, factor
            ));
            return;
        }
        let normalized = token.to_lowercase();
        let history = self.recent_routes.entry(normalized.clone()).or_default();
        let mut counts: HashMap<NodeId, u32> = HashMap::new();
        for entry in history.iter().rev().take(64) {
            *counts.entry(entry.node).or_insert(0) += 1;
        }
        let mut ranked: Vec<(u32, NodeId)> = counts
            .into_iter()
            .map(|(node, count)| (count, node))
            .collect();
        ranked.sort_by(|a, b| b.cmp(a));
        let mut boosted = 0u16;
        for (_count, node_id) in ranked.into_iter().take(top as usize) {
            let entry = self
                .route_bias
                .entry((normalized.clone(), node_id))
                .or_insert(1.0);
            let factor = factor.clamp(0.1, 3.0);
            *entry = (*entry * factor).clamp(0.2, 5.0);
            boosted = boosted.saturating_add(1);
        }
        logs.push(format!(
            "REFLEX_BOOST id={} top={} factor={:.2} boosted={}",
            rule_id, top, factor, boosted
        ));
    }

    pub fn route_impulse(&mut self, imp: Impulse) -> Vec<String> {
        let tokens = tokenize(&imp.pattern);
        let mut seen_tokens = HashSet::new();
        let mut logs = Vec::new();
        let mut energy_deltas: Vec<EventDelta> = Vec::new();

        if imp.pattern == "affect/noradrenaline" {
            let until = self.now_ms.saturating_add(imp.ttl_ms);
            self.noradrenaline = Some(NoradrenalineEffect {
                until_ms: until,
                strength: imp.strength.clamp(0.0, 1.0),
            });
            logs.push(format!(
                "HORMONE noradrenaline s={:.2} window={}ms",
                imp.strength, imp.ttl_ms
            ));
            logs.push("ASTRO TAG on".into());
        }
        let mut fired_rules = HashSet::new();
        for token in tokens.iter().cloned() {
            if seen_tokens.insert(token.clone()) {
                self.process_reflex_for(&token, &imp, &mut logs, &mut fired_rules);
            }
        }
        let mut candidates: HashSet<NodeId> = HashSet::new();
        for token in &tokens {
            if let Some(ids) = self.index.get(token) {
                candidates.extend(ids.iter().copied());
            }
        }
        if candidates.is_empty() {
            candidates.extend(self.cells.keys().copied());
        }
        let mut scored: Vec<(f32, NodeId)> = candidates
            .into_iter()
            .filter_map(|id| {
                self.cells.get(&id).map(|cell| {
                    let affinity_scale = self.affinity_scale_for(cell);
                    let mut score = score_cell(cell, &imp, affinity_scale);
                    let mut best_bias = 1.0f32;
                    for token in &tokens {
                        if let Some(bias) = self.route_bias.get(&(token.clone(), id)) {
                            if *bias > best_bias {
                                best_bias = *bias;
                            }
                        }
                    }
                    score *= best_bias;
                    (score, id)
                })
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut responded: Vec<(NodeId, u32)> = Vec::new();
        for (_score, id) in scored.into_iter().take(3) {
            if let Some(cell) = self.cells.get_mut(&id) {
                let before_salience = cell.salience;
                let previous = cell.last_response_ms;
                if let Some(log) = cell.ingest(&imp, self.now_ms) {
                    cell.last_response_ms = self.now_ms;
                    let delta = EventDelta::Energy(EnergyDelta {
                        id,
                        energy: cell.energy,
                        metabolism: cell.metabolism,
                        state: cell.state,
                        last_response_ms: cell.last_response_ms,
                    });
                    logs.push(log);
                    energy_deltas.push(delta);
                    let latency = self.now_ms.saturating_sub(previous).min(u32::MAX as u64) as u32;
                    responded.push((id, latency));
                    if (cell.salience - before_salience).abs() > f32::EPSILON {
                        logs.push(format!(
                            "SALIENCE n{} {:.2}->{:.2}",
                            id, before_salience, cell.salience
                        ));
                    }
                }
            }
        }
        let responded_nodes: Vec<NodeId> = responded.iter().map(|(id, _)| *id).collect();
        self.record_route_sequence(&responded_nodes);
        for token in seen_tokens.iter() {
            let history = self.recent_routes.entry(token.clone()).or_default();
            for id in &responded_nodes {
                history.push_back(RecentRoute {
                    ts: self.now_ms,
                    node: *id,
                    strength: imp.strength,
                });
            }
            while let Some(front) = history.front() {
                if self.now_ms.saturating_sub(front.ts) > ROUTE_RETENTION_MS {
                    history.pop_front();
                } else {
                    break;
                }
            }
            while history.len() > MAX_RECENT_ROUTES {
                history.pop_front();
            }
        }
        if !responded.is_empty() {
            for token in seen_tokens.iter() {
                for (node, latency) in &responded {
                    self.record_token_hit(token, *node, imp.strength, *latency);
                }
            }
        }
        self.ticks_since_impulse = 0;
        for delta in energy_deltas {
            self.emit(delta);
        }
        logs
    }

    pub fn tick_all(&mut self, dt_ms: u64) -> FieldEvents {
        self.now_ms = self.now_ms.saturating_add(dt_ms);
        self.emit(EventDelta::Tick(TickDelta {
            now_ms: self.now_ms,
        }));
        let ids: Vec<NodeId> = self.cells.keys().copied().collect();
        let hormone = self.noradrenaline_active();
        let base_sleep = self.harmony.sleep_delta;
        let mut pending_events: Vec<EventDelta> = Vec::new();
        for id in &ids {
            if let Some(cell) = self.cells.get_mut(id) {
                let sleep_shift = if cell.adreno_tag {
                    hormone
                        .map(|effect| (base_sleep - 0.2 * effect.strength).clamp(-0.4, 0.2))
                        .unwrap_or(base_sleep)
                } else {
                    base_sleep
                };
                cell.tick(dt_ms, self.harmony.metabolism_scale, sleep_shift);
                cell.drift_affinity(self.harmony.alpha);
                pending_events.push(EventDelta::Energy(EnergyDelta {
                    id: *id,
                    energy: cell.energy,
                    metabolism: cell.metabolism,
                    state: cell.state,
                    last_response_ms: cell.last_response_ms,
                }));
            }
        }
        let mut events: FieldEvents = Vec::new();
        if let Some(effect) = self.noradrenaline {
            if self.now_ms >= effect.until_ms {
                self.noradrenaline = None;
                events.push("ASTRO TAG off".into());
            }
        }
        let mut new_cells: Vec<(NodeId, NodeCell)> = Vec::new();
        let mut purge_ids: Vec<NodeId> = Vec::new();
        for id in &ids {
            if let Some(cell) = self.cells.get_mut(id) {
                let parent_affinity = cell.affinity;
                if let Some(mut child) = cell.maybe_divide() {
                    let child_affinity = child.affinity;
                    let new_id = self.next_id;
                    self.next_id += 1;
                    child.id = new_id;
                    child.seed.core_pattern = format!("{}:n{}", child.seed.core_pattern, new_id);
                    child.last_response_ms = self.now_ms;
                    events.push(format!(
                        "DIVIDE parent=n{} -> child=n{} (aff {:.2}->{:.2})",
                        id, new_id, parent_affinity, child_affinity
                    ));
                    pending_events.push(EventDelta::Affinity(AffinityDelta {
                        id: *id,
                        affinity: cell.affinity,
                    }));
                    pending_events.push(EventDelta::Energy(EnergyDelta {
                        id: *id,
                        energy: cell.energy,
                        metabolism: cell.metabolism,
                        state: cell.state,
                        last_response_ms: cell.last_response_ms,
                    }));
                    new_cells.push((*id, child));
                }
            }
        }
        for (parent, child) in new_cells {
            if let Some(parent_cell) = self.cells.get_mut(&parent) {
                if parent_cell.links.insert(child.id) {
                    self.register_link(parent, child.id);
                    pending_events.push(EventDelta::Link(LinkDelta {
                        from: parent,
                        to: child.id,
                    }));
                }
            }
            let snapshot = CellSnapshot::from(&child);
            self.attach_cell(child);
            pending_events.push(EventDelta::Divide(DivideDelta {
                parent,
                child: snapshot,
            }));
        }

        let mut needs_reindex = false;
        for id in ids {
            if let Some(cell) = self.cells.get_mut(&id) {
                let prev_state = cell.state;
                let sleep_shift = if cell.adreno_tag {
                    hormone
                        .map(|effect| (base_sleep - 0.2 * effect.strength).clamp(-0.4, 0.2))
                        .unwrap_or(base_sleep)
                } else {
                    base_sleep
                };
                let died = cell.maybe_sleep_or_die(self.now_ms, sleep_shift);
                if prev_state != NodeState::Sleep && cell.state == NodeState::Sleep {
                    events.push(format!("SLEEP n{}", id));
                    pending_events.push(EventDelta::Sleep(StateDelta { id }));
                }
                if died && prev_state != NodeState::Dead {
                    events.push(format!("DEAD n{}", id));
                    pending_events.push(EventDelta::Dead(StateDelta { id }));
                    needs_reindex = true;
                    purge_ids.push(id);
                }
                pending_events.push(EventDelta::Energy(EnergyDelta {
                    id,
                    energy: cell.energy,
                    metabolism: cell.metabolism,
                    state: cell.state,
                    last_response_ms: cell.last_response_ms,
                }));
            }
        }
        let before_trim = self.cells.len();
        self.cells.retain(|_, cell| cell.state != NodeState::Dead);
        if self.cells.len() != before_trim {
            needs_reindex = true;
        }
        if needs_reindex {
            self.rebuild_index();
        }
        for id in purge_ids {
            self.purge_tracking_for(id);
        }
        self.refresh_important_cells();
        self.ticks_since_impulse = self.ticks_since_impulse.saturating_add(1);
        for event in pending_events {
            self.emit(event);
        }
        self.view_tick_accum = self.view_tick_accum.saturating_add(dt_ms);
        while self.view_tick_accum >= 500 {
            self.view_tick_accum -= 500;
            let due = self.views.take_due(self.now_ms);
            for view in due {
                let stats = self.stats_for_pattern(&view.pattern, view.window_ms, &view.filter);
                events.push(ViewRegistry::build_event(&view, &stats));
            }
        }
        let harmony_snapshot = {
            let window = self.reflex_engine.window();
            let samples = self.symmetry_samples(window);
            self.reflex_engine.tick(self.now_ms, dt_ms, &samples)
        };
        if let Some(snapshot) = harmony_snapshot {
            let mirror_value = snapshot
                .mirror
                .as_ref()
                .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
                .unwrap_or(serde_json::Value::Null);
            let event = json!({
                "ev": "harmony",
                "meta": {
                    "strength": snapshot.metrics.avg_strength,
                    "latency": snapshot.metrics.avg_latency,
                    "entropy": snapshot.entropy_ratio,
                    "delta_strength": snapshot.delta_strength,
                    "delta_latency": snapshot.delta_latency,
                    "status": snapshot.status.as_str(),
                    "pattern": snapshot.dominant_pattern,
                    "mirror": mirror_value,
                }
            });
            events.push(event.to_string());
        }
        self.awaken_tick();
        events
    }

    pub fn trim_low_energy(&mut self) {
        let before = self.cells.len();
        let mut rng = rand::thread_rng();
        for cell in self.cells.values_mut() {
            if cell.adreno_tag || cell.salience >= 0.7 {
                continue;
            }
            if cell.salience < 0.2
                && self.now_ms.saturating_sub(cell.last_recall_ms) > 60_000
                && rng.gen_bool(0.35)
            {
                if cell.state == NodeState::Active {
                    cell.state = NodeState::Sleep;
                }
                if cell.energy < 0.05 && rng.gen_bool(0.25) {
                    cell.state = NodeState::Dead;
                }
            }
        }
        let victims: Vec<NodeId> = self
            .cells
            .iter()
            .filter_map(|(id, cell)| {
                if cell.adreno_tag || cell.salience >= 0.7 {
                    return None;
                }
                if cell.energy <= 0.1 || cell.state == NodeState::Dead {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        self.cells.retain(|_, cell| {
            if cell.adreno_tag || cell.salience >= 0.7 {
                return cell.state != NodeState::Dead;
            }
            cell.energy > 0.1 && cell.state != NodeState::Dead
        });
        if self.cells.len() != before {
            self.rebuild_index();
        }
        for id in victims {
            self.emit(EventDelta::Dead(StateDelta { id }));
            self.purge_tracking_for(id);
        }
        self.refresh_important_cells();
    }

    pub fn inject_seed_variation(&mut self, base_seed: &str) {
        if self.cells.len() > 64 {
            return;
        }
        let mut rng = rand::thread_rng();
        let params = create_seed(base_seed);
        let mut cell = NodeCell::from_seed(self.next_id, params.clone());
        cell.affinity = (params.affinity + rng.gen_range(-0.1..0.1)).clamp(0.0, 1.0);
        cell.seed.affinity = cell.affinity;
        cell.energy = 0.5 + rng.gen_range(0.0..0.3);
        cell.last_response_ms = self.now_ms;
        cell.seed.core_pattern = format!("{}:bud{}", params.core_pattern, self.next_id);
        let id = cell.id;
        self.next_id += 1;
        let snapshot = CellSnapshot::from(&cell);
        self.attach_cell(cell);
        self.emit(EventDelta::Spawn(snapshot));
        let mut link_events: Vec<EventDelta> = Vec::new();
        let mut new_links: Vec<(NodeId, NodeId)> = Vec::new();
        for other in self.cells.values_mut() {
            if other.id != id && rng.gen_bool(0.1) {
                if other.links.insert(id) {
                    new_links.push((other.id, id));
                    link_events.push(EventDelta::Link(LinkDelta {
                        from: other.id,
                        to: id,
                    }));
                }
            }
        }
        for event in link_events {
            self.emit(event);
        }
        for (from, to) in new_links {
            self.register_link(from, to);
        }
    }

    pub fn metrics_snapshot(&self) -> Vec<(NodeId, f32, NodeState, u64, f32)> {
        self.cells
            .values()
            .map(|c| {
                (
                    c.id,
                    c.metabolism,
                    c.state,
                    self.now_ms.saturating_sub(c.last_response_ms),
                    c.energy,
                )
            })
            .collect()
    }

    pub fn symmetry_snapshot(&self) -> HarmonySnapshot {
        self.reflex_engine.snapshot()
    }

    pub fn set_mirror_interval(&mut self, interval_ms: u64) {
        self.reflex_engine.set_interval(interval_ms);
    }

    pub fn mirror_interval(&self) -> u64 {
        self.reflex_engine.interval()
    }

    pub fn apply_delta(&mut self, delta: &EventDelta) {
        match delta {
            EventDelta::Tick(tick) => {
                self.now_ms = tick.now_ms;
                self.ticks_since_impulse = 0;
            }
            EventDelta::Spawn(snapshot) => {
                let cell = snapshot_to_node(snapshot);
                self.attach_cell(cell);
                self.next_id = self.next_id.max(snapshot.id + 1);
            }
            EventDelta::Divide(divide) => {
                let cell = snapshot_to_node(&divide.child);
                if let Some(parent) = self.cells.get_mut(&divide.parent) {
                    if parent.links.insert(divide.child.id) {
                        self.register_link(divide.parent, divide.child.id);
                    }
                }
                self.attach_cell(cell);
                self.next_id = self.next_id.max(divide.child.id + 1);
            }
            EventDelta::Sleep(state) => {
                if let Some(cell) = self.cells.get_mut(&state.id) {
                    cell.state = NodeState::Sleep;
                }
            }
            EventDelta::Dead(state) => {
                self.cells.remove(&state.id);
                self.rebuild_index();
                self.purge_tracking_for(state.id);
            }
            EventDelta::Link(link) => {
                if let Some(cell) = self.cells.get_mut(&link.from) {
                    if cell.links.insert(link.to) {
                        self.register_link(link.from, link.to);
                    }
                }
            }
            EventDelta::Unlink(link) => {
                if let Some(cell) = self.cells.get_mut(&link.from) {
                    if cell.links.remove(&link.to) {
                        self.unregister_link(link.from, link.to);
                    }
                }
            }
            EventDelta::Affinity(delta) => {
                if let Some(cell) = self.cells.get_mut(&delta.id) {
                    cell.affinity = delta.affinity;
                    cell.seed.affinity = delta.affinity;
                }
            }
            EventDelta::Energy(delta) => {
                if let Some(cell) = self.cells.get_mut(&delta.id) {
                    cell.energy = delta.energy;
                    cell.metabolism = delta.metabolism;
                    cell.state = delta.state;
                    cell.last_response_ms = delta.last_response_ms;
                }
            }
            EventDelta::ReflexAdd(rule) => {
                let mut rule = rule.clone();
                rule.when.token = rule.when.token.to_lowercase();
                self.next_reflex_id = self.next_reflex_id.max(rule.id + 1);
                self.rules.insert(rule.id, rule);
            }
            EventDelta::ReflexRemove { id } => {
                self.rules.remove(id);
            }
            EventDelta::ReflexFire(_fire) => {}
            EventDelta::TrsSet(cfg) => {
                self.trs.apply_config(cfg);
                self.harmony.alpha = self.trs.alpha;
            }
            EventDelta::TrsTarget(delta) => {
                self.trs.set_target(delta.target_load);
            }
            EventDelta::TrsTrace(trace) => {
                self.trs.last_err = trace.error;
                self.trs.last_ts = trace.now_ms;
                self.harmony.alpha = trace.alpha;
            }
            EventDelta::TrsHarmony(delta) => {
                self.harmony.alpha = delta.alpha;
                self.harmony.affinity_scale = delta.affinity_scale;
                self.harmony.metabolism_scale = delta.metabolism_scale;
                self.harmony.sleep_delta = delta.sleep_delta;
            }
            EventDelta::DreamConfig(cfg) => {
                self.dream_cfg = cfg.clone();
            }
            EventDelta::SyncConfig(cfg) => {
                self.sync_cfg = cfg.clone();
            }
            EventDelta::DreamStrengthen(delta) => {
                self.link_scores.insert((delta.from, delta.to), delta.score);
            }
            EventDelta::DreamWeaken(delta) => {
                self.link_scores.insert((delta.from, delta.to), delta.score);
            }
            EventDelta::DreamPrune(delta) => {
                if let Some(cell) = self.cells.get_mut(&delta.from) {
                    cell.links.remove(&delta.to);
                }
                self.unregister_link(delta.from, delta.to);
            }
            EventDelta::DreamRewire(delta) => {
                if let Some(cell) = self.cells.get_mut(&delta.from) {
                    if cell.links.insert(delta.to) {
                        self.register_link(delta.from, delta.to);
                    }
                }
            }
            EventDelta::DreamReport(delta) => {
                self.dream_reports
                    .push_back((delta.now_ms, delta.report.clone()));
                while self.dream_reports.len() > 16 {
                    self.dream_reports.pop_front();
                }
                self.last_dream_ms = delta.now_ms;
            }
            EventDelta::SyncShare(delta) => {
                self.route_bias
                    .insert((delta.token.to_lowercase(), delta.node), delta.bias);
            }
            EventDelta::SyncAlign(delta) => {
                self.link_scores.insert((delta.from, delta.to), delta.score);
            }
            EventDelta::CollectiveDreamReport(delta) => {
                self.sync_reports
                    .push_back((delta.now_ms, delta.report.clone()));
                while self.sync_reports.len() > 16 {
                    self.sync_reports.pop_front();
                }
            }
            EventDelta::ModelBuild(delta) => {
                let frame = ModelFrame {
                    hash: delta.hash.clone(),
                    cell_count: delta.cell_count,
                    created_ms: delta.now_ms,
                };
                self.resonant_model.record_build(frame.clone());
                self.sync_log
                    .record_build(frame.hash.clone(), frame.created_ms, frame.cell_count);
            }
            EventDelta::AwakenApply(delta) => {
                self.awakening_cfg = delta.config.clone();
                self.resonant_model.set_last_applied(delta.hash.clone());
                if let Some(hash) = &delta.hash {
                    self.sync_log
                        .record_apply(hash.clone(), delta.now_ms, delta.cell_count);
                }
            }
            EventDelta::AwakenTick(delta) => {
                self.last_awaken_tick = delta.now_ms;
                self.sync_log.record_tick(delta.now_ms);
                if let Some(hash) = &delta.hash {
                    if self.resonant_model.last_applied().is_none() {
                        self.resonant_model.set_last_applied(Some(hash.clone()));
                    }
                }
            }
        }
    }
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|c| matches!(c, '/' | ':' | '.'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

fn wake_priority(cell: &NodeCell, token: &str) -> f32 {
    let token_lower = token.to_lowercase();
    let core_tokens = tokenize(&cell.seed.core_pattern);
    if core_tokens.iter().any(|existing| existing == &token_lower) {
        0.0
    } else {
        (cell.affinity - 0.5).abs() + 0.5
    }
}

fn reflex_token_matches(rule_token: &str, token: &str) -> bool {
    if rule_token == token {
        return true;
    }
    tokenize(rule_token)
        .iter()
        .any(|candidate| candidate == token)
}

#[derive(Debug, Clone, Copy, Default)]
struct LinkStat {
    hits: u64,
    last_used_ms: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct RouteStat {
    hits: u64,
    last_used_ms: u64,
}

#[cfg(test)]
mod tests {
    #![allow(dead_code, unnameable_test_items)]

    use super::*;
    use crate::reflex::ReflexWhen;
    use crate::types::Hint;
    use serde_json::Value;

    use crate::synchrony::{detect_sync_groups, SyncConfig};

    #[test]
    fn reflex_fire_on_window_threshold() {
        let mut field = ClusterField::new();
        field.add_root("liminal/test");
        let rule = ReflexRule {
            id: 0,
            when: ReflexWhen {
                token: "cpu/load".into(),
                kind: ImpulseKind::Affect,
                min_strength: 0.7,
                window_ms: 1_000,
                min_count: 5,
            },
            then: ReflexAction::EmitHint {
                hint: Hint::SlowTick,
            },
            enabled: true,
        };
        field.add_reflex(rule);
        for i in 0..5 {
            field.now_ms = (i * 200) as u64;
            let logs = field.route_impulse(Impulse {
                kind: ImpulseKind::Affect,
                pattern: "cpu/load".into(),
                strength: 0.8,
                ttl_ms: 1_000,
                tags: Vec::new(),
            });
            if i < 4 {
                assert!(
                    !logs.iter().any(|log| log.contains("REFLEX_FIRE")),
                    "reflex should not fire before threshold"
                );
            } else {
                assert!(
                    logs.iter().any(|log| log.contains("REFLEX_FIRE")),
                    "reflex should fire on fifth impulse"
                );
            }
        }

        #[test]
        fn detect_sync_groups_forms_clusters() {
            let mut field = ClusterField::new();
            let a1 = field.add_root("liminal/a1");
            let a2 = field.add_root("liminal/a2");
            let b1 = field.add_root("liminal/b1");
            let b2 = field.add_root("liminal/b2");
            field.now_ms = 1_000;
            field.record_route_sequence(&[a1, a2, a1]);
            field.record_route_sequence(&[b1, b2, b1]);
            field.record_route_sequence(&[a1, a2]);
            field.record_route_sequence(&[b1, b2]);

            let mut cfg = SyncConfig::default();
            cfg.cooccur_threshold = 0.1;
            cfg.max_groups = 4;
            cfg.share_top_k = 2;
            let groups = detect_sync_groups(&field, &cfg, field.now_ms);
            assert!(
                groups.len() >= 2,
                "expected at least two groups, got {:?}",
                groups
            );
            assert!(groups
                .iter()
                .any(|g| g.nodes.contains(&a1) && g.nodes.contains(&a2)));
            assert!(groups
                .iter()
                .any(|g| g.nodes.contains(&b1) && g.nodes.contains(&b2)));
        }

        #[test]
        fn share_tokens_updates_bias_and_protects_salient() {
            let mut field = ClusterField::new();
            let a = field.add_root("liminal/share_a");
            let b = field.add_root("liminal/share_b");
            field.now_ms = 5_000;
            if let Some(cell) = field.cells.get_mut(&a) {
                cell.links.insert(b);
            }
            field.register_link(a, b);
            if let Some(cell) = field.cells.get_mut(&b) {
                cell.links.insert(a);
                cell.salience = 0.7;
            }
            field.register_link(b, a);

            let (shared, aligned, protected) =
                field.share_tokens(&[a, b], &["sync/test".into()], 0.2);
            assert!(shared >= 2);
            assert!(aligned > 0);
            assert!(protected >= 1);
            let bias_a = field
                .route_bias
                .get(&("sync/test".to_string(), a))
                .cloned()
                .unwrap_or(0.0);
            assert!(bias_a >= 1.0 && bias_a <= 5.0);
            let score = field.link_score(a, b);
            assert!(score >= 0.2 && score <= 3.0);
        }

        #[test]
        fn dream_recent_pairs_counts() {
            let mut field = ClusterField::new();
            field.now_ms = 10_000;
            let token = "unit".to_string();
            let history = field.recent_routes.entry(token).or_default();
            history.push_back(RecentRoute {
                ts: 9_900,
                node: 1,
                strength: 0.6,
            });
            history.push_back(RecentRoute {
                ts: 9_905,
                node: 2,
                strength: 0.8,
            });
            history.push_back(RecentRoute {
                ts: 9_910,
                node: 1,
                strength: 0.7,
            });
            let pairs = field.collect_recent_pairs(200);
            assert_eq!(pairs.len(), 1, "exactly one pair should be recorded");
            let stat = &pairs[0];
            assert_eq!(stat.u, 1);
            assert_eq!(stat.v, 2);
            assert_eq!(stat.freq, 2.0);
            assert!((stat.avg_strength - 0.7).abs() < 1e-6);
        }

        #[test]
        fn dream_adjusts_and_prunes_links() {
            let mut field = ClusterField::new();
            let a = field.add_root("liminal/a");
            let b = field.add_root("liminal/b");
            {
                let cell = field.cells.get_mut(&a).unwrap();
                cell.links.insert(b);
            }
            field.register_link(a, b);
            assert!((field.link_score(a, b) - 1.0).abs() < f32::EPSILON);
            assert!(field.adjust_link_score(a, b, 0.5));
            assert!(field.link_score(a, b) > 1.0);
            assert!(field.adjust_link_score(a, b, 5.0));
            assert!((field.link_score(a, b) - 3.0).abs() < f32::EPSILON);
            assert!(field.prune_link(a, b));
            assert_eq!(field.link_scores.get(&(a, b)), None);
        }

        #[test]
        fn dream_respects_protected_nodes() {
            let mut field = ClusterField::new();
            let a = field.add_root("liminal/a");
            let b = field.add_root("liminal/b");
            {
                let cell = field.cells.get_mut(&a).unwrap();
                cell.links.insert(b);
                cell.salience = 0.9;
            }
            field.register_link(a, b);
            let history = field.recent_routes.entry("dream".into()).or_default();
            field.now_ms = 5_000;
            history.push_back(RecentRoute {
                ts: 4_900,
                node: a,
                strength: 0.6,
            });
            history.push_back(RecentRoute {
                ts: 4_905,
                node: b,
                strength: 0.6,
            });
            let cfg = DreamConfig {
                protect_salience: 0.6,
                max_ops_per_cycle: 10,
                ..DreamConfig::default()
            };
            let now = field.now_ms;
            let report = run_dream(&mut field, &cfg, now);
            assert!(report.protected > 0, "protected counter should increase");
            assert!((field.link_score(a, b) - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn remove_reflex_disables_rule() {
        let mut field = ClusterField::new();
        let id = field.add_reflex(ReflexRule {
            id: 0,
            when: ReflexWhen {
                token: "cpu/load".into(),
                kind: ImpulseKind::Affect,
                min_strength: 0.5,
                window_ms: 500,
                min_count: 2,
            },
            then: ReflexAction::EmitHint {
                hint: Hint::FastTick,
            },
            enabled: true,
        });
        assert_eq!(field.list_reflex().len(), 1);
        assert!(field.remove_reflex(id));
        assert!(field.list_reflex().is_empty());
        for i in 0..3 {
            field.now_ms = (i * 200) as u64;
            let logs = field.route_impulse(Impulse {
                kind: ImpulseKind::Affect,
                pattern: "cpu/load".into(),
                strength: 0.9,
                ttl_ms: 1_000,
                tags: Vec::new(),
            });
            assert!(
                !logs.iter().any(|log| log.contains("REFLEX_FIRE")),
                "removed reflex should not fire"
            );
        }
    }

    #[test]
    fn dream_session_reconfigures_field() {
        let mut field = ClusterField::new();
        let ids: Vec<NodeId> = (0..4)
            .map(|i| field.add_root(&format!("liminal/test{}", i)))
            .collect();
        let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
        for &(from, to) in &[(a, b), (c, d)] {
            if let Some(cell) = field.cells.get_mut(&from) {
                cell.links.insert(to);
            }
            field.register_link(from, to);
        }
        assert!((field.link_score(a, b) - 1.0).abs() < f32::EPSILON);
        assert!(field.adjust_link_score(c, d, -0.7));
        assert!(field.link_score(c, d) <= 0.3);

        field.now_ms = 20_000;
        {
            let history = field.recent_routes.entry("dream/high".into()).or_default();
            let base = field.now_ms - 400;
            for offset in [0_u64, 30, 60, 90] {
                history.push_back(RecentRoute {
                    ts: base + offset,
                    node: a,
                    strength: 0.9,
                });
                history.push_back(RecentRoute {
                    ts: base + offset + 5,
                    node: b,
                    strength: 0.85,
                });
            }
        }
        {
            let history = field.recent_routes.entry("dream/low".into()).or_default();
            let base = field.now_ms - 200;
            history.push_back(RecentRoute {
                ts: base,
                node: c,
                strength: 0.5,
            });
            history.push_back(RecentRoute {
                ts: base + 8,
                node: d,
                strength: 0.45,
            });
        }

        let mut cfg = field.dream_config();
        cfg.strengthen_top_pct = 0.5;
        cfg.weaken_bottom_pct = 0.5;
        cfg.max_ops_per_cycle = 10;
        field.set_dream_config(cfg.clone());

        let report = field
            .run_dream_cycle()
            .expect("dream cycle should run with enough data");
        assert!(report.strengthened >= 1, "expected strengthened links");
        assert!(report.weakened >= 1, "expected weakened links");
        assert!(report.pruned >= 1, "expected pruned links");
        assert!(
            field.link_score(a, b) > 1.0,
            "strengthened link should have increased score"
        );
        assert!(
            field.link_scores.get(&(c, d)).is_none(),
            "weakened link should be pruned"
        );
        let history = field.last_dream_reports(1);
        assert_eq!(history.len(), 1, "dream report should be recorded");
        assert_eq!(history[0].0, field.now_ms);
        assert_eq!(history[0].1.strengthened, report.strengthened);
        assert_eq!(history[0].1.weakened, report.weakened);
        assert_eq!(history[0].1.pruned, report.pruned);
    }

    #[test]
    fn harmony_event_emits_with_mirror() {
        let mut field = ClusterField::new();
        field.add_root("cpu/load");
        field.set_mirror_interval(400);
        for i in 0..4 {
            field.now_ms = (i * 100) as u64;
            let _ = field.route_impulse(Impulse {
                kind: ImpulseKind::Query,
                pattern: "cpu/load".into(),
                strength: 0.9,
                ttl_ms: 1_000,
                tags: Vec::new(),
            });
        }
        let events = field.tick_all(400);
        let harmony_line = events
            .iter()
            .find(|line| line.contains("\"harmony\""))
            .expect("expected harmony event");
        let payload: Value = serde_json::from_str(harmony_line).expect("valid json");
        assert_eq!(payload["ev"], "harmony");
        assert!(payload["meta"]["mirror"].is_object());
        assert!(payload["meta"]["entropy"].is_number());
    }

    #[test]
    fn harmony_event_repeats_on_interval() {
        let mut field = ClusterField::new();
        field.add_root("cpu/load");
        field.set_mirror_interval(300);
        field.tick_all(300);
        let first = field.tick_all(300);
        assert!(first.iter().any(|line| line.contains("\"harmony\"")));
        let second = field.tick_all(300);
        assert!(second.iter().any(|line| line.contains("\"harmony\"")));
    }

    #[test]
    fn recent_for_respects_window() {
        let mut field = ClusterField::new();
        field.now_ms = 100;
        field.record_token_hit("cpu", 1, 0.8, 120);
        field.now_ms = 600;
        field.record_token_hit("cpu", 2, 0.6, 80);
        field.now_ms = 1_200;
        let hits = field.recent_for("cpu", 700);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node, 2);
        assert_eq!(hits[0].latency_ms, 80);
    }

    #[test]
    fn lql_select_subscribe_flow() {
        let mut field = ClusterField::new();
        field.now_ms = 100;
        field.record_token_hit("cpu", 1, 0.9, 110);
        field.now_ms = 400;
        field.record_token_hit("load", 2, 0.7, 90);
        let mut filter = ViewFilter::default();
        filter.min_strength = Some(0.8);
        let select = field
            .exec_lql(LqlAst::Select {
                pattern: "cpu/load".into(),
                window_ms: Some(1_000),
                filter,
            })
            .unwrap();
        match select.response.unwrap() {
            LqlResponse::Select(result) => {
                assert_eq!(result.stats.count, 1);
                assert!(result.stats.avg_strength >= 0.8);
            }
            other => panic!("unexpected response: {other:?}"),
        }

        let subscribe = field
            .exec_lql(LqlAst::Subscribe {
                pattern: "cpu".into(),
                window_ms: Some(1_000),
                every_ms: Some(500),
                filter: ViewFilter::default(),
            })
            .unwrap();
        let view_id = match subscribe.response.unwrap() {
            LqlResponse::Subscribe(result) => result.id,
            other => panic!("unexpected response: {other:?}"),
        };
        assert!(!subscribe.events.is_empty());

        field.now_ms = 700;
        field.record_token_hit("cpu", 3, 0.85, 70);
        let events = field.tick_all(500);
        assert!(events.iter().any(|ev| ev.contains("\"ev\":\"view\"")));

        let unsubscribe = field.exec_lql(LqlAst::Unsubscribe { id: view_id }).unwrap();
        match unsubscribe.response.unwrap() {
            LqlResponse::Unsubscribe(result) => {
                assert!(result.removed);
            }
            other => panic!("unexpected response: {other:?}"),
        }
        let followup = field.tick_all(500);
        assert!(
            followup
                .iter()
                .filter(|ev| ev.contains("\"ev\":\"view\""))
                .count()
                <= 1
        );
    }

    #[test]
    fn sleep_threshold_delta_keeps_cells_awake() {
        let mut baseline = ClusterField::new();
        let id = baseline.add_root("liminal/idle");
        {
            let cell = baseline.cells.get_mut(&id).unwrap();
            cell.state = NodeState::Idle;
            cell.energy = 0.12;
            cell.last_response_ms = 0;
        }
        baseline.now_ms = 5_000;
        baseline.tick_all(200);
        assert!(matches!(
            baseline.cells.get(&id).unwrap().state,
            NodeState::Sleep
        ));

        let mut tuned = ClusterField::new();
        let id_tuned = tuned.add_root("liminal/idle");
        {
            let cell = tuned.cells.get_mut(&id_tuned).unwrap();
            cell.state = NodeState::Idle;
            cell.energy = 0.12;
            cell.last_response_ms = 0;
        }
        tuned.now_ms = 5_000;
        let output = TrsOutput {
            alpha_new: 0.3,
            tick_adjust_ms: 0,
            affinity_scale: 1.0,
            metabolism_scale: 1.0,
            sleep_threshold_delta: 0.12,
        };
        tuned.apply_trs_output(0, 0.4, &output);
        tuned.tick_all(200);
        assert!(
            !matches!(tuned.cells.get(&id_tuned).unwrap().state, NodeState::Sleep),
            "positive sleep delta should keep cell awake"
        );
    }

    #[test]
    fn affinity_scale_increases_match_scores() {
        let mut cell = NodeCell::from_seed(
            1,
            SeedParams {
                affinity: 0.85,
                base_metabolism: 0.2,
                core_pattern: "liminal/test".into(),
            },
        );
        cell.affinity = 0.86;
        cell.state = NodeState::Active;
        let impulse = Impulse {
            kind: ImpulseKind::Query,
            pattern: "liminal/test".into(),
            strength: 0.88,
            ttl_ms: 1000,
            tags: Vec::new(),
        };
        let base = score_cell(&cell, &impulse, 1.0);
        let boosted = score_cell(&cell, &impulse, 1.3);
        assert!(boosted > base);
    }

    fn drive_harmony(field: &mut ClusterField) {
        field.add_root("cpu/load");
        for idx in 0..4 {
            field.now_ms = (idx * 200) as u64;
            let _ = field.route_impulse(Impulse {
                kind: ImpulseKind::Query,
                pattern: "cpu/load".into(),
                strength: 0.9,
                ttl_ms: 1_000,
                tags: Vec::new(),
            });
        }
    }

    #[test]
    fn harmony_event_emitted_on_interval() {
        let mut field = ClusterField::new();
        drive_harmony(&mut field);
        field.set_mirror_interval(400);
        let mut harmony_seen = 0;
        for _ in 0..5 {
            let events = field.tick_all(400);
            if events.iter().any(|ev| ev.contains("\"ev\":\"harmony\"")) {
                harmony_seen += 1;
            }
        }
        assert!(harmony_seen >= 1, "expected harmony events to be emitted");
    }

    #[test]
    fn harmony_event_contains_mirror_signal() {
        let mut field = ClusterField::new();
        drive_harmony(&mut field);
        field.set_mirror_interval(400);
        let mut mirror_present = false;
        for _ in 0..6 {
            let events = field.tick_all(400);
            for ev in events {
                if ev.contains("\"ev\":\"harmony\"") {
                    assert!(ev.contains("\"mirror\""));
                    mirror_present = true;
                }
            }
            if mirror_present {
                break;
            }
        }
        assert!(
            mirror_present,
            "mirror impulse not observed in harmony event"
        );
    }

    #[test]
    fn noradrenaline_adjusts_and_resets() {
        let mut field = ClusterField::new();
        let id = field.add_root("liminal/norepi");
        if let Some(cell) = field.cells.get_mut(&id) {
            cell.mark_adreno();
        }
        field.now_ms = 1_000;
        let logs = field.route_impulse(Impulse {
            kind: ImpulseKind::Affect,
            pattern: "affect/noradrenaline".into(),
            strength: 0.8,
            ttl_ms: 2_000,
            tags: Vec::new(),
        });
        assert!(
            logs.iter().any(|log| log.contains("HORMONE noradrenaline")),
            "hormone activation not logged"
        );
        let cell = field.cells.get(&id).unwrap();
        let boosted_affinity = field.affinity_scale_for(cell);
        assert!(
            boosted_affinity > field.harmony.affinity_scale,
            "affinity scale should increase during hormone window"
        );
        let hormone = field
            .noradrenaline_active()
            .expect("hormone should be active");
        let sleepy = (field.harmony.sleep_delta - 0.2 * hormone.strength).clamp(-0.4, 0.2);
        assert!(
            sleepy < field.harmony.sleep_delta,
            "sleep threshold should decrease during hormone window"
        );
        let _ = field.tick_all(2_100);
        let _cell = field.cells.get(&id).unwrap();
        assert!(field.noradrenaline_active().is_none());
        let restored = field.harmony.sleep_delta;
        assert!(
            (restored - field.harmony.sleep_delta).abs() < f32::EPSILON,
            "sleep shift should return to baseline after hormone"
        );
    }
}

fn score_cell(cell: &NodeCell, imp: &Impulse, affinity_scale: f32) -> f32 {
    let match_score = 1.0 - (cell.affinity - imp.strength).abs();
    let mut affinity_score = match_score;
    if match_score > 0.6 {
        affinity_score *= affinity_scale.clamp(0.5, 1.5);
    }
    let state_bonus = match cell.state {
        NodeState::Active => 0.2,
        NodeState::Idle => 0.0,
        NodeState::Sleep => -0.4,
        NodeState::Dead => -1.0,
    };
    affinity_score + state_bonus
}

fn snapshot_to_node(snapshot: &CellSnapshot) -> NodeCell {
    let mut cell = NodeCell::from_seed(
        snapshot.id,
        SeedParams {
            affinity: snapshot.seed_affinity,
            base_metabolism: snapshot.seed_metabolism,
            core_pattern: snapshot.core_pattern.clone(),
        },
    );
    cell.links = snapshot.links.iter().copied().collect();
    cell.metabolism = snapshot.metabolism;
    cell.affinity = snapshot.affinity;
    cell.last_response_ms = snapshot.last_response_ms;
    cell.energy = snapshot.energy;
    cell.state = snapshot.state;
    cell.salience = snapshot.salience;
    cell.adreno_tag = snapshot.adreno_tag;
    cell.last_recall_ms = snapshot.last_recall_ms;
    cell
}
