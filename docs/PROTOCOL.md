# Протокол взаимодействия LiminalDB v0.2

Документ объединяет сведения из раннего описания (v0.1) и обновлений v0.2,
которые появились вместе с Harmony Loop, TRS и языком запросов LQL. Он
предназначен и для разработчиков, и для интеграторов, которым нужно понимать,
как выглядит обмен между мостом (`liminal-bridge-abi`), ядром и CLI.

## Структуры импульсов

В основе взаимодействия находятся импульсы — события, которые подаются в ткань
клеток. В версии v0.1 структура описывалась так:

```text
struct Impulse {
    kind: ImpulseKind,      // Query | Write | Affect
    pattern: String,        // произвольная строка, разбивается на токены
    strength: f32,          // 0.0..1.0, интенсивность
    ttl_ms: u64,            // время жизни
    tags: Vec<String>,      // произвольные метки источника
}
```

В v0.2 она транслируется в компактное CBOR-представление (используется в
`--pipe-cbor` режиме и мосте). Поля сопоставляются так:

```text
// Impulse -> ProtocolImpulse (CBOR / JSON)
{
  "k": 0 | 1 | 2,   // тип импульса: 0 = affect, 1 = query, 2 = write
  "p": <text>,      // паттерн или путь
  "s": <f32>,       // сила воздействия (0.0–1.0)
  "t": <u32>,       // TTL в миллисекундах
  "tg": [<text>]    // теги источника (может отсутствовать или быть пустым)
}
```

CLI по умолчанию ограничивает силу диапазоном `[0.0, 1.0]`; для удобства
поддерживаются короткие команды `a`, `q`, `w` (см. ниже).

## CBOR-пакет обмена

Все сообщения между мостом и потребителями кодируются в CBOR. Команды
`liminal_push`/`push` принимают CBOR-объект импульса или управления (см.
команды TRS/LQL), а `liminal_pull`/`pull` возвращают CBOR-объект "пакета":

```text
{
  "events": [Event, ...],   // опционально, массив может быть пустым
  "metrics": Metrics?       // опционально, последнее известное состояние ядра
}
```

### События `Event`

```text
{
  "ev": "divide" | "sleep" | "dead" | "metrics" | "hint" | "trs_trace" |
         "harmony" | "lql" | "view",
  "id": <u64 | text>,  // идентификатор сущности или произвольный тег
  "dt": <u32>,         // дельта времени тикера (мс)
  "meta": { ... }      // дополнительные поля, зависят от события
}
```

* `divide`: `meta = { "parent": u64, "child": u64, "aff_before": f32,
  "aff_after": f32 }`.
* `sleep`:  `meta = { "state": "sleep" }`.
* `dead`:   `meta = { "state": "dead" }`.
* `metrics`: `meta = { "cells": u32, "sleeping": f32, "avgMet": f32,
  "avgLat": u32 }`.
* `hint`:   `meta = { "hint": "slow_tick" | "fast_tick" | "trim_field" |
  "wake_seeds", "tick_ms": u32, ... }`.
* `trs_trace`: `meta = { "alpha": f32, "err": f32, "observed": f32,
  "tick_adj": i32 }`.
* `harmony`: `meta = { "alpha": f32, "aff_scale": f32, "met_scale": f32,
  "sleep_delta": f32 }`.
* `lql`: события, связанные с запросами LQL.
* `view`: публикации живых представлений.

### Метрики `Metrics`

Ранее метрики описывались как структура:

```text
struct Metrics {
    cells: usize,           // количество живых клеток
    sleeping_pct: f32,      // доля спящих клеток (0.0..1.0)
    avg_metabolism: f32,    // средняя метаболическая активность
    avg_latency_ms: f32,    // среднее время ответа
}
```

В v0.2 она расширена — ядро также считает `active_pct` и `live_load`.
CBOR-представление:

```text
{
  "cells": <u32>,
  "sleeping": <f32>,
  "avgMet": <f32>,
  "avgLat": <u32>
}
```

Метрики формируются наблюдателем (`morph_mind`). На их основе появляются
подсказки (`Hint`), управляющие жизненным циклом. CLI каждую секунду печатает
сводку вида:

```
METRICS cells=12 sleeping=0.42 active=0.38 avgMet=0.71 avgLat=140.0 live=0.58 | HINTS: [FastTick]
```

## Примеры CBOR (hex)

```text
Impulse: a5616b006170686370752f6c6f61646173fb3fe999999999999a61741903846274678163636c69
Metrics: a46563656c6c730568736c656570696e67fb3fd0000000000000666176674d6574fb3fe3851eb851eb85666176674c617418b4
```

Hex-дампы приведены без пробелов и префикса `0x` для удобства копирования в
`--pipe-cbor` режим CLI.

## CLI-команды (человекочитаемый режим)

Режим v0.1 сохранён. Команды принимаются через stdin. Сила (`strength`) по
умолчанию `0.6` и ограничивается диапазоном `[0.0, 1.0]`.

```text
q <pattern> [strength]  # создать Query-импульс
w <pattern> [strength]  # создать Write-импульс
a <pattern> [strength]  # создать Affect-импульс
:reflex add <json>      # добавить правило рефлекса
:reflex list            # вывести список правил
:reflex rm <id>         # удалить правило по идентификатору
:trs show               # показать состояние TRS/Harmony Loop
:trs set <json>         # установить коэффициенты TRS
:trs target <value>     # обновить целевую "живость" (0.3..0.8)
```

### LQL и Views

В версии v0.6 добавлен лёгкий язык запросов LQL. Команды вводятся через `lql
...`:

```text
SELECT <pattern> [WHERE strength>=<f32>] [WINDOW <ms>]
SUBSCRIBE <pattern> [WINDOW <ms>] [EVERY <ms>]
UNSUBSCRIBE <view_id>
```

* `SELECT` возвращает сводку по совпадениям шаблона за окно: `count`,
  `avg_strength`, `avg_latency`, `top_nodes` (до трёх узлов).
* `SUBSCRIBE` регистрирует живое представление (View). Каждые `EVERY`
  миллисекунд оно публикует событие `view` с такой же сводкой.
* `UNSUBSCRIBE` отменяет подписку по идентификатору.

Примеры CLI:

```text
lql SELECT cpu/load WINDOW 1000
lql SUBSCRIBE temp/device WINDOW 3000 EVERY 1000
```

В `--pipe-cbor` режиме запросы передаются как команды:

```text
{"cmd":"lql","q":"SELECT cpu/load WHERE strength>=0.7 WINDOW 1000"}
```

События ядра:

```text
{"ev":"lql","meta":{"select":{...}}}
{"ev":"lql","meta":{"subscribe":{...}}}
{"ev":"lql","meta":{"unsubscribe":{...}}}
{"ev":"view","meta":{"id":<u64>,"pattern":<text>,"window":<u32>,"every":<u32>,"stats":{...}}}
```

`stats` содержит:

```text
{
  "count": <u32>,
  "avg_strength": <f32>,
  "avg_latency": <f32>,
  "top_nodes": [ {"id": <u64>, "hits": <u32>}, ... ]
}
```

### Правила рефлексов

CLI принимает JSON-описание правил через команду `:reflex add`. Поля
структуры:

```text
{
  "token": <text>,             // ключевой токен (всегда приводится к нижнему регистру)
  "kind": "Affect"|"Query"|"Write",
  "min_strength": <f32>,       // порог силы импульса
  "window_ms": <u32>,          // ширина скользящего окна, миллисекунды
  "min_count": <u16>,          // минимальное число импульсов
  "then": {
    "EmitHint": { "hint": "SlowTick"|... } |
    "SpawnSeed": { "seed": <text>, "affinity_shift": <f32> } |
    "WakeSleeping": { "count": <u16> } |
    "BoostLinks": { "factor": <f32>, "top": <u16> }
  },
  "enabled": <bool?>           // опционально, по умолчанию true
}
```

Примеры:

```text
:reflex add {"token":"cpu/load","kind":"Affect","min_strength":0.7,"window_ms":1000,"min_count":5,"then":{"BoostLinks":{"factor":1.2,"top":8}}}
:reflex add {"token":"mem/free","kind":"Query","min_strength":0.5,"window_ms":1500,"min_count":3,"then":{"WakeSleeping":{"count":2}}}
```

### Harmony Loop / TRS

TRS управляет плавностью цикла. Команды CLI:

```text
:trs show
:trs set {"alpha":0.25,"beta":0.6,"k_p":0.8,"k_i":0.15,"k_d":0.05,"target_load":0.6}
:trs target 0.62
```

В `--pipe-cbor` режиме соответствующие команды передаются как объекты CBOR:

```text
{"cmd":"trs_set","cfg":{"alpha":0.25,"beta":0.6,"k_p":0.8,"k_i":0.15,"k_d":0.05,"target_load":0.6}}
{"cmd":"trs_target","value":0.62}
```

## Режим `--pipe-cbor`

CLI может работать в потоковом режиме CBOR. Каждая строка stdin трактуется как
hex-представление CBOR-импульса. Ответы печатаются в stdout также в виде
hex-строк CBOR-пакетов. Полезно для интеграционных тестов и скриптов.

Пример запуска:

```text
cargo run -p liminal-cli -- --pipe-cbor
```

После старта можно вручную ввести hex-строку импульса и получить hex-ответ с
событиями/метриками.
