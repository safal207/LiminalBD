# Протокол взаимодействия LiminalDB v0.2

## CBOR-структуры

### Пакет обмена

Все сообщения между мостом и потребителями кодируются в CBOR. `liminal_push`/`push` принимают CBOR-объект импульса. `liminal_pull`/`pull` возвращают CBOR-объект "пакета":

```
{
  "events": [Event, ...],   // опционально, массив может быть пустым
  "metrics": Metrics?       // опционально, последнее известное состояние ядра
}
```

### Impulse

```
{
  "k": 0 | 1 | 2,   // тип импульса: 0 = affect, 1 = query, 2 = write
  "p": <text>,      // паттерн или путь
  "s": <f32>,       // сила воздействия (0.0–1.0)
  "t": <u32>,       // TTL в миллисекундах
  "tg": [<text>]    // теги источника (может отсутствовать или быть пустым)
}
```

### Event

```
{
  "ev": "divide" | "sleep" | "dead" | "metrics" | "hint" | "trs_trace" | "harmony",
  "id": <u64 | text>,  // идентификатор сущности или произвольный тег
  "dt": <u32>,         // дельта времени тикера (мс)
  "meta": { ... }      // дополнительные поля, зависят от события
}
```

* `divide`: `meta = { "parent": u64, "child": u64, "aff_before": f32, "aff_after": f32 }`.
* `sleep`:  `meta = { "state": "sleep" }`.
* `dead`:   `meta = { "state": "dead" }`.
* `metrics`: `meta = { "cells": u32, "sleeping": f32, "avgMet": f32, "avgLat": u32 }`.
* `hint`:   `meta = { "hint": "slow_tick" | "fast_tick" | "trim_field" | "wake_seeds", "tick_ms": u32, ... }`. Дополнительные поля могут появляться в будущем.
* `trs_trace`: `meta = { "alpha": f32, "err": f32, "observed": f32, "tick_adj": i32 }`.
* `harmony`: `meta = { "alpha": f32, "aff_scale": f32, "met_scale": f32, "sleep_delta": f32 }`.

### Metrics

```
{
  "cells": <u32>,     // количество живых клеток
  "sleeping": <f32>,  // доля клеток в состоянии сна
  "avgMet": <f32>,    // средний метаболизм
  "avgLat": <u32>     // усреднённая латентность (мс)
}
```

## Примеры CBOR (hex)

```
Impulse: a5616b006170686370752f6c6f61646173fb3fe999999999999a61741903846274678163636c69
Metrics: a46563656c6c730568736c656570696e67fb3fd0000000000000666176674d6574fb3fe3851eb851eb85666176674c617418b4
```

Hex-дампы приведены в формате без пробелов и префикса `0x` для удобства копирования в `--pipe-cbor` режим CLI.

## CLI-команды (человекочитаемый режим)

Режим v0.1 сохранён. Команды принимаются через stdin. Сила (`strength`) по умолчанию `0.6` и ограничивается диапазоном `[0.0, 1.0]`.

```
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

### Правила рефлексов

CLI принимает JSON-описание правил через команду `:reflex add`. Поля структуры:

```
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

```
:reflex add {"token":"cpu/load","kind":"Affect","min_strength":0.7,"window_ms":1000,"min_count":5,"then":{"BoostLinks":{"factor":1.2,"top":8}}}
:reflex add {"token":"mem/free","kind":"Query","min_strength":0.5,"window_ms":1500,"min_count":3,"then":{"WakeSleeping":{"count":2}}}
```

### Harmony Loop / TRS

TRS управляет плавностью цикла. Команды CLI:

```
:trs show
:trs set {"alpha":0.25,"beta":0.6,"k_p":0.8,"k_i":0.15,"k_d":0.05,"target_load":0.6}
:trs target 0.62
```

В `--pipe-cbor` режиме соответствующие команды передаются как объекты CBOR:

```
{"cmd":"trs_set","cfg":{"alpha":0.25,"beta":0.6,"k_p":0.8,"k_i":0.15,"k_d":0.05,"target_load":0.6}}
{"cmd":"trs_target","value":0.62}
```

## Режим `--pipe-cbor`

CLI может работать в потоковом режиме CBOR. Каждая строка stdin трактуется как hex-представление CBOR-импульса. Ответы печатаются в stdout также в виде hex-строк CBOR-пакетов. Полезно для интеграционных тестов и скриптов.

Пример запуска:

```
cargo run -p liminal-cli -- --pipe-cbor
```

После старта можно вручную ввести hex-строку импульса и получить hex-ответ с событиями/метриками.
