use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use rand::Rng;
use serde_json::json;

use crate::journal::{
    AffinityDelta, CellSnapshot, DivideDelta, EnergyDelta, EventDelta, Journal, LinkDelta,
    StateDelta, TickDelta, TrsHarmonyDelta, TrsTargetDelta, TrsTraceDelta,
};
use crate::lql::{
    LqlAst, LqlExecResult, LqlResponse, LqlResult, LqlSelectResult, LqlSubscribeResult,
    LqlUnsubscribeResult,
};
use crate::node_cell::NodeCell;
use crate::reflex::{ReflexAction, ReflexEngine, ReflexFire, ReflexId, ReflexRule};
use crate::seed::{create_seed, SeedParams};
use crate::symmetry::HarmonySnapshot;
use crate::trs::{TrsConfig, TrsOutput, TrsState};
use crate::types::{Impulse, ImpulseKind, NodeId, NodeState};
use crate::views::{NodeHitStat, ViewRegistry, ViewStats};

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
    route_usage: HashMap<(NodeId, NodeId), RouteStat>,
    route_bias: HashMap<(String, NodeId), f32>,
    recent_routes: HashMap<String, VecDeque<NodeId>>,
    sleeping_accum_ms: u64,
    last_dream_ms: u64,
    pub trs: TrsState,
    harmony: HarmonyTuning,
    pub views: ViewRegistry,
    next_hit_seq: u64,
    view_tick_accum: u64,
    reflex_engine: ReflexEngine,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub seq: u64,
    pub ts: u64,
    pub node: NodeId,
    pub strength: f32,
    pub latency_ms: u32,
}

const MAX_TOKEN_HITS: usize = 512;
const HIT_RETENTION_MS: u64 = 120_000;
const DEFAULT_WINDOW_MS: u32 = 1_000;
const DEFAULT_VIEW_EVERY_MS: u32 = 1_000;

#[derive(Debug, Clone, Copy)]
pub struct HarmonyTuning {
    pub alpha: f32,
    pub affinity_scale: f32,
    pub metabolism_scale: f32,
    pub sleep_delta: f32,
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
            route_usage: HashMap::new(),
            sleeping_accum_ms: 0,
            last_dream_ms: 0,
            route_bias: HashMap::new(),
            recent_routes: HashMap::new(),
            trs: TrsState::default(),
            harmony: HarmonyTuning::default(),
            views: ViewRegistry::new(),
            next_hit_seq: 1,
            view_tick_accum: 0,
            reflex_engine: ReflexEngine::new(2_000),
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
        }
    }

    pub fn stats_for_pattern(
        &self,
        pattern: &str,
        window_ms: u32,
        min_strength: Option<f32>,
    ) -> ViewStats {
        let window = window_ms.max(1);
        let mut hits = self.gather_hits_for_pattern(pattern, window);
        if let Some(min_strength) = min_strength {
            hits.retain(|hit| hit.strength >= min_strength);
        }
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
                min_strength,
                window_ms,
            } => {
                let window = window_ms.unwrap_or(DEFAULT_WINDOW_MS).max(1);
                let stats = self.stats_for_pattern(&pattern, window, min_strength);
                let payload = LqlSelectResult {
                    pattern: pattern.clone(),
                    window_ms: window,
                    min_strength,
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
            } => {
                let window = window_ms.unwrap_or(DEFAULT_WINDOW_MS).max(1);
                let every = every_ms.unwrap_or(DEFAULT_VIEW_EVERY_MS).max(200);
                let id = self
                    .views
                    .add_view(pattern.clone(), window, every, self.now_ms);
                let payload = LqlSubscribeResult {
                    id,
                    pattern: pattern.clone(),
                    window_ms: window,
                    every_ms: every,
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
    }

    fn unregister_link(&mut self, from: NodeId, to: NodeId) {
        self.link_usage.remove(&(from, to));
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

    fn evaluate_dream_state(
        &mut self,
        dt_ms: u64,
        sleeping_cells: usize,
        total_cells: usize,
        events: &mut FieldEvents,
    ) {
        if total_cells < 2 {
            self.sleeping_accum_ms = 0;
            return;
        }
        let ratio = sleeping_cells as f32 / total_cells as f32;
        if ratio > 0.6 {
            self.sleeping_accum_ms = self.sleeping_accum_ms.saturating_add(dt_ms);
        } else {
            self.sleeping_accum_ms = 0;
        }
        if self.sleeping_accum_ms >= 10_000
            && self.now_ms.saturating_sub(self.last_dream_ms) >= 5_000
        {
            if let Some((weakened, strengthened, shifted)) = self.execute_dream_session() {
                events.push(format!(
                    "DREAM session: weakened={} strengthened={} shifted={} sleeping={:.2}",
                    weakened, strengthened, shifted, ratio
                ));
            }
            self.sleeping_accum_ms = 0;
            self.last_dream_ms = self.now_ms;
        }
    }

    fn execute_dream_session(&mut self) -> Option<(usize, usize, usize)> {
        if self.cells.len() < 2 {
            return None;
        }
        let mut rng = rand::thread_rng();

        let mut existing_links: Vec<((NodeId, NodeId), LinkStat)> = self
            .link_usage
            .iter()
            .filter_map(|(&(from, to), stat)| {
                let Some(cell) = self.cells.get(&from) else {
                    return None;
                };
                if cell.links.contains(&to) {
                    Some(((from, to), *stat))
                } else {
                    None
                }
            })
            .collect();

        let mut weakened = 0usize;
        let mut strengthened = 0usize;
        let mut shifted = 0usize;

        if !existing_links.is_empty() {
            existing_links.sort_by(|a, b| {
                let (stat_a, stat_b) = (a.1, b.1);
                match stat_a.hits.cmp(&stat_b.hits) {
                    Ordering::Equal => stat_a.last_used_ms.cmp(&stat_b.last_used_ms),
                    other => other,
                }
            });
            let mut target_remove = ((existing_links.len() as f32) * 0.1).ceil() as usize;
            if target_remove == 0 {
                target_remove = 1;
            }
            for ((from, to), _) in existing_links.into_iter().take(target_remove) {
                if let Some(cell) = self.cells.get_mut(&from) {
                    if cell.links.remove(&to) {
                        self.unregister_link(from, to);
                        self.emit(EventDelta::Unlink(LinkDelta { from, to }));
                        weakened += 1;
                    }
                }
            }
        }

        let mut route_candidates: Vec<((NodeId, NodeId), RouteStat)> = self
            .route_usage
            .iter()
            .filter_map(|(&(from, to), stat)| {
                if from == to {
                    return None;
                }
                let Some(cell) = self.cells.get(&from) else {
                    return None;
                };
                if cell.links.contains(&to) {
                    return None;
                }
                Some(((from, to), *stat))
            })
            .collect();
        if !route_candidates.is_empty() {
            route_candidates.sort_by(|a, b| {
                let (stat_a, stat_b) = (a.1, b.1);
                match stat_b.hits.cmp(&stat_a.hits) {
                    Ordering::Equal => stat_b.last_used_ms.cmp(&stat_a.last_used_ms),
                    other => other,
                }
            });
            let mut target_add = ((route_candidates.len() as f32) * 0.1).ceil() as usize;
            if target_add == 0 {
                target_add = 1;
            }
            for ((from, to), _) in route_candidates.into_iter().take(target_add) {
                if let Some(cell) = self.cells.get_mut(&from) {
                    if cell.links.insert(to) {
                        self.register_link(from, to);
                        self.emit(EventDelta::Link(LinkDelta { from, to }));
                        strengthened += 1;
                    }
                }
            }
        }

        let mut leaders: Vec<(f32, NodeId)> = self
            .cells
            .values()
            .map(|cell| {
                let score = cell.links.len() as f32 * 0.6 + cell.energy;
                (score, cell.id)
            })
            .collect();
        leaders.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
        let leader_count = if self.cells.len() >= 6 {
            2
        } else if self.cells.len() >= 3 {
            1
        } else {
            0
        };
        let mut affinity_updates: Vec<(NodeId, f32)> = Vec::new();
        for (_, id) in leaders.into_iter().take(leader_count) {
            if let Some(cell) = self.cells.get_mut(&id) {
                let mut delta: f32 = rng.gen_range(-0.04..0.04);
                if delta.abs() < 0.005 {
                    delta = if delta.is_sign_negative() {
                        -0.01
                    } else {
                        0.01
                    };
                }
                cell.affinity = (cell.affinity + delta).clamp(0.0, 1.0);
                cell.seed.affinity = cell.affinity;
                affinity_updates.push((id, cell.affinity));
                shifted += 1;
            }
        }
        for (id, affinity) in affinity_updates {
            self.emit(EventDelta::Affinity(AffinityDelta { id, affinity }));
        }

        if weakened == 0 && strengthened == 0 && shifted == 0 {
            return None;
        }

        Some((weakened, strengthened, shifted))
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
        for &node in history.iter().rev().take(64) {
            *counts.entry(node).or_insert(0) += 1;
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
                    let mut score = score_cell(cell, &imp, self.harmony.affinity_scale);
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
                let previous = cell.last_response_ms;
                if let Some(log) = cell.ingest(&imp) {
                    cell.last_response_ms = self.now_ms;
                    let delta = EventDelta::Energy(EnergyDelta {
                        id,
                        energy: cell.energy,
                        metabolism: cell.metabolism,
                        state: cell.state,
                        last_response_ms: cell.last_response_ms,
                    });
                    logs.push(log);
                    self.emit(delta);
                    let latency = self.now_ms.saturating_sub(previous).min(u32::MAX as u64) as u32;
                    responded.push((id, latency));
                }
            }
        }
        let responded_nodes: Vec<NodeId> = responded.iter().map(|(id, _)| *id).collect();
        self.record_route_sequence(&responded_nodes);
        for token in seen_tokens.iter() {
            let history = self.recent_routes.entry(token.clone()).or_default();
            for id in &responded_nodes {
                history.push_back(*id);
            }
            while history.len() > 64 {
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
        logs
    }

    pub fn tick_all(&mut self, dt_ms: u64) -> FieldEvents {
        self.now_ms = self.now_ms.saturating_add(dt_ms);
        self.emit(EventDelta::Tick(TickDelta {
            now_ms: self.now_ms,
        }));
        let ids: Vec<NodeId> = self.cells.keys().copied().collect();
        let mut pending_events: Vec<EventDelta> = Vec::new();
        for id in &ids {
            if let Some(cell) = self.cells.get_mut(id) {
                cell.tick(
                    dt_ms,
                    self.harmony.metabolism_scale,
                    self.harmony.sleep_delta,
                );
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
                let died = cell.maybe_sleep_or_die(self.now_ms, self.harmony.sleep_delta);
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
        self.ticks_since_impulse = self.ticks_since_impulse.saturating_add(1);
        let total_cells = self.cells.len();
        let sleeping_cells = self
            .cells
            .values()
            .filter(|cell| cell.state == NodeState::Sleep)
            .count();
        self.evaluate_dream_state(dt_ms, sleeping_cells, total_cells, &mut events);
        for event in pending_events {
            self.emit(event);
        }
        self.view_tick_accum = self.view_tick_accum.saturating_add(dt_ms);
        while self.view_tick_accum >= 500 {
            self.view_tick_accum -= 500;
            let due = self.views.take_due(self.now_ms);
            for view in due {
                let stats = self.stats_for_pattern(&view.pattern, view.window_ms, None);
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
        events
    }

    pub fn trim_low_energy(&mut self) {
        let before = self.cells.len();
        let victims: Vec<NodeId> = self
            .cells
            .iter()
            .filter_map(|(id, cell)| {
                if cell.energy <= 0.1 || cell.state == NodeState::Dead {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        self.cells
            .retain(|_, cell| cell.energy > 0.1 && cell.state != NodeState::Dead);
        if self.cells.len() != before {
            self.rebuild_index();
        }
        for id in victims {
            self.emit(EventDelta::Dead(StateDelta { id }));
            self.purge_tracking_for(id);
        }
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
    use super::*;
    use crate::reflex::ReflexWhen;
    use crate::types::Hint;
    use serde_json::Value;

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
        let ids: Vec<NodeId> = (0..5)
            .map(|i| field.add_root(&format!("liminal/test{}", i)))
            .collect();
        for &(from, to) in &[
            (ids[0], ids[1]),
            (ids[1], ids[2]),
            (ids[2], ids[3]),
            (ids[3], ids[4]),
            (ids[4], ids[0]),
        ] {
            field.apply_delta(&EventDelta::Link(LinkDelta { from, to }));
        }
        if let Some(stat) = field.link_usage.get_mut(&(ids[0], ids[1])) {
            stat.hits = 0;
            stat.last_used_ms = 0;
        }
        for (&key, stat) in field.link_usage.iter_mut() {
            if key != (ids[0], ids[1]) {
                stat.hits = 100;
                stat.last_used_ms = 1_000;
            }
        }
        field.route_usage.insert(
            (ids[1], ids[4]),
            RouteStat {
                hits: 50,
                last_used_ms: 500,
            },
        );

        let affinities_before: HashMap<NodeId, f32> = field
            .cells
            .iter()
            .map(|(id, cell)| (*id, cell.affinity))
            .collect();

        for cell in field.cells.values_mut() {
            cell.state = NodeState::Sleep;
            cell.energy = 0.3;
            cell.metabolism = 0.0;
            cell.seed.base_metabolism = 0.0;
            cell.last_response_ms = 0;
        }

        let mut summary_line = None;
        for _ in 0..12 {
            let events = field.tick_all(1_000);
            if let Some(line) = events.iter().find(|e| e.contains("DREAM session")) {
                summary_line = Some(line.clone());
                break;
            }
        }
        let summary = summary_line.expect("dream session should trigger");
        let weakened = extract_metric(&summary, "weakened=");
        let strengthened = extract_metric(&summary, "strengthened=");
        let shifted_metric = extract_metric(&summary, "shifted=");
        assert!(weakened > 0, "expected weakened links");
        assert!(strengthened > 0, "expected strengthened routes");
        let shifted = field
            .cells
            .iter()
            .filter(|(id, cell)| (cell.affinity - affinities_before[id]).abs() > f32::EPSILON)
            .count();
        assert!(
            shifted >= 1 || shifted_metric > 0,
            "at least one affinity should shift"
        );
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

    fn extract_metric(line: &str, prefix: &str) -> usize {
        line.split_whitespace()
            .find_map(|token| token.strip_prefix(prefix))
            .and_then(|value| {
                value
                    .trim_end_matches(|c: char| c == ',' || c == ')')
                    .parse()
                    .ok()
            })
            .unwrap_or(0)
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
        let select = field
            .exec_lql(LqlAst::Select {
                pattern: "cpu/load".into(),
                min_strength: Some(0.8),
                window_ms: Some(1_000),
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
    cell
}
