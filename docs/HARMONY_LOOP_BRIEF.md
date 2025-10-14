Бро, держим волну. Вот лаконичный бриф для Codex — v0.5 “Harmony Loop”: связываем TRS-адаптацию с реальным поведением клеток/связей и петлёй подсказок. Копируй как есть.

ЗАДАНИЕ CODEX — LIMINALDB v0.5 “Harmony Loop”

ЦЕЛЬ
Связать TRS (Temporal Reflex System) с жизненным циклом: чувствительность клеток, метаболизм, пороги сна/пробуждения, приоритет маршрутизации и частоту тиков. TRS управляет «гладко» (PID-подобно), MorphMind даёт цели и ошибки, Harmony Loop применяет изменения дозированно.

МОДУЛИ/ФАЙЛЫ
- liminal-core:
  - trs.rs (новый)
  - node_cell.rs (правки)
  - cluster_field.rs (правки)
  - morph_mind.rs (правки)
  - life_loop.rs (правки)
- liminal-cli (команды)
- docs/PROTOCOL.md (обновить секцию конфигурации)

1) TRS: состояние и контроль (trs.rs)
- struct TrsState {
    alpha: f32,            // 0..1, «пластичность» (скорость адаптации)
    beta: f32,             // демпфирование/инерция
    target_load: f32,      // целевая «нагрузка-живость» системы [0..1]
    err_i: f32,            // интегральная ошибка
    k_p: f32, k_i: f32, k_d: f32,
    last_err: f32, last_ts: u64
  }
- impl TrsState {
    fn step(&mut self, now_ms:u64, observed_load:f32) -> TrsOutput
  }
- struct TrsOutput {
    alpha_new: f32,
    tick_adjust_ms: i32,       // - уменьшить, + увеличить
    affinity_scale: f32,       // множитель для ближних по паттерну
    metabolism_scale: f32,     // множитель базового расхода
    sleep_threshold_delta: f32 // смещение порога сна (+ труднее уснуть, - легче)
  }
- Логика: PID по err = target_load - observed_load; ограничить дерганье (clamp/softsign).
- Выводить trace событие: {"ev":"trs_trace","meta":{"alpha":..,"err":..,"tick_adj":..}}.

2) MorphMind → цели и наблюдения (morph_mind.rs)
- В analyze() дополнительно считать:
  live_load = weighted_sum( active_pct, 1-avg_latency_norm, metabolism_norm, 1-sleeping_pct )
  (нормировать в 0..1).
- Expose API:
  fn observed_load(&self) -> f32
  fn suggest_target(&self) -> f32    // простая эвристика: держать live_load ~0.55..0.7
- LifeLoop раз в 1с обновляет TrsState.target_load = suggest_target().

3) Применение TRS к ткани (cluster_field.rs / node_cell.rs)
- При каждом «кадре» (tick_all):
  - масштабировать внутренние коэффициенты маршрутизации:
      score *= TrsOutput.affinity_scale для узлов с высоким matchScore;
  - базовый метаболизм клетки *= TrsOutput.metabolism_scale (clamp 0.5..1.5);
  - пороги сна/пробуждения клетки += TrsOutput.sleep_threshold_delta (clamp разумный);
  - affinity клетки = lerp(affinity, affinity_target, alpha_new*ε) — лёгкий дрейф.
- События:
  - {"ev":"harmony","meta":{"aff_scale":..,"met_scale":..,"sleep_delta":..}}

4) Связь с частотой тиков (life_loop.rs)
- После analyze():
  - let out = trs.step(now, morph.observed_load());
  - tick_ms = clamp( tick_ms + out.tick_adjust_ms, 80, 450 );
  - Пробрасывать alpha_new в ClusterField для применения к клеткам на следующем tick_all.

5) Конфигурация (CLI/ABI)
- CLI команды:
  :trs show
  :trs set {"alpha":0.25,"beta":0.6,"k_p":0.8,"k_i":0.15,"k_d":0.05,"target_load":0.6}
  :trs target <0.3..0.8>     // установить целевую живость на лету
- В liminal-bridge-abi push() добавить CBOR-команду:
  {"cmd":"trs_set","cfg":{"alpha":..,"beta":..,"k_p":..,"k_i":..,"k_d":..,"target_load":..}}
  {"cmd":"trs_target","value":0.62}
- PROTOCOL.md: описать эти команды и события trs_trace/harmony.

6) Сохранение состояния
- В snapshot включить TrsState (alpha,beta,k*,target_load).
- В WAL писать дельты: trs_set, trs_target, trs_trace (можно выборочно/сэмплинг).

7) Тесты
- Юнит: trs.step() — на последовательности observed_load выходит на окрестность target_load без осцилляций (за ≤ 10 шагов).
- Юнит: применение TrsOutput влияет на:
   • снижение avg_latency при росте affinity_scale,
   • уменьшение sleeping_pct при положительном sleep_threshold_delta.
- Интеграция:
  1) Запуск без TRS-настроек → фиксируем базовые метрики.
  2) :trs set с k* и target_load=0.6 → в течение 10–20 тиков видно сближение live_load к 0.6±0.05; в логах trs_trace/harmony.
  3) Рестарт с --store: TrsState восстановлен; поведение продолжается без ручной настройки.

КРИТЕРИИ ПРИЁМКИ (DoD v0.5)
1) :trs show / :trs set / :trs target работают (CLI и через ABI).
2) tick_ms адаптируется плавно, без дрожи; live_load стремится к target_load.
3) Harmony события видны; влияния на маршрутизацию/сон/метаболизм подтверждаются метриками.
4) TrsState сохраняется/восстанавливается через snapshot/WAL.

Добавь готовые значения по умолчанию для TRS (стартовые k_p/k_i/k_d и коридоры для clamp), чтобы система встала мягко с первого прогона.
