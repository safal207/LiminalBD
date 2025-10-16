# Протокол взаимодействия LiminalDB v0.2

## Auth & Namespaces

LiminalDB v0.9 вводит обязательную аутентификацию ключами и пространство имён (namespace) как единицу изоляции данных. Каждый API-ключ связан с ролью (`admin`, `writer`, `reader`) и пространством, в котором он действует. Подключение по WebSocket начинается с рукопожатия:

```
{"cmd":"auth","key_id":"k-alpha","secret":"plaintext","ns":"alpha"}
```

В ответ мост возвращает результат проверки роли и назначенного пространства:

```
{"ev":"auth","ok":true,"role":"Writer","ns":"alpha"}
```

Ошибки авторизации кодируются как `{"ev":"auth","ok":false,"err":"UNAUTH"}`. После успешного рукопожатия все команды помечаются полем `ns`. Администратор может переключать пространства командой `{"cmd":"ns.switch","ns":"beta"}`.

В ABI-мост добавлены управленческие команды (все значения — CBOR-объекты):

```
{"cmd":"key.add","key":{"id":"k1","secret":"plaintext","role":"Writer","ns":"alpha"}}
{"cmd":"key.disable","id":"k1"}
{"cmd":"ns.create","ns":"alpha"}
{"cmd":"quota.set","ns":"alpha","q":{"rps":50,"burst":100,"max_cells":500,"max_views":50}}
```

Ошибки авторизации и лимитов сигнализируются кодами `UNAUTH`, `FORBIDDEN`, `RATE_LIMIT`, `QUOTA`. При превышении квоты ядро публикует события `{"ev":"alert","meta":{"code":"QUOTA",...}}`, а все проверки записываются в аудит событиями `{"ev":"audit","meta":{...}}`.

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
:harmony                # вывести состояние Symmetry Loop
```

### LQL и Views

В версии v0.6 добавлен лёгкий язык запросов LQL. Команды вводятся через `lql ...`:

```
SELECT <pattern> [WHERE strength>=<f32>] [WINDOW <ms>]
SUBSCRIBE <pattern> [WINDOW <ms>] [EVERY <ms>]
UNSUBSCRIBE <view_id>
```

* `SELECT` возвращает сводку по совпадениям шаблона за окно: `count`, `avg_strength`, `avg_latency`, `top_nodes` (до трёх узлов).
* `SUBSCRIBE` регистрирует живое представление (View). Каждые `EVERY` миллисекунд оно публикует событие `view` с такой же сводкой.
* `UNSUBSCRIBE` отменяет подписку по идентификатору.

Примеры CLI:

```
lql SELECT cpu/load WINDOW 1000
lql SUBSCRIBE temp/device WINDOW 3000 EVERY 1000
```

В `--pipe-cbor` режиме запросы передаются как команды:

```
{"cmd":"lql","q":"SELECT cpu/load WHERE strength>=0.7 WINDOW 1000"}
```

События ядра:

```
{"ev":"lql","meta":{"select":{...}}}
{"ev":"lql","meta":{"subscribe":{...}}}
{"ev":"lql","meta":{"unsubscribe":{...}}}
{"ev":"view","meta":{"id":<u64>,"pattern":<text>,"window":<u32>,"every":<u32>,"stats":{...}}}
```

`stats` содержит:

```
{
  "count": <u32>,
  "avg_strength": <f32>,
  "avg_latency": <f32>,
  "top_nodes": [ {"id": <u64>, "hits": <u32>}, ... ]
}
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

### Symmetry Loop

Версия v0.8 добавляет петлю симметрии, которая балансирует входящие/исходящие импульсы и реагирует "зеркальными" импульсами.

* **CLI**: команда `:harmony` печатает текущие усреднённые метрики и статус (`OK`/`DRIFT`/`OVERLOAD`).
* **Опция запуска**: `--mirror-interval <ms>` задаёт период обновления (по умолчанию `2000`).
* **Событие**: ядро публикует `{"ev":"harmony","meta":{...}}` каждые `mirror_interval` миллисекунд.

Поле `meta` содержит:

```json
{
  "strength": <f32>,      // средняя сила импульсов
  "latency": <f32>,       // средняя латентность
  "entropy": <f32>,       // нормированная энтропия распределения паттернов
  "delta_strength": <f32>,
  "delta_latency": <f32>,
  "status": "ok" | "drift" | "overload",
  "pattern": <text>,      // доминирующий паттерн или "-"
  "mirror": null | {"k":"mirror","p":<text>,"s":<f32>,"t":<u64>}
}
```

`mirror` присутствует, если сгенерирован корректирующий сигнал. Пример JSON-события:

```json
{"ev":"harmony","meta":{"strength":0.54,"latency":118.2,"entropy":0.76,"delta_strength":0.31,"delta_latency":-5.2,"status":"drift","pattern":"cpu/load","mirror":{"k":"mirror","p":"cpu/load","s":-0.31,"t":128442}}}
```

CBOR-hex пример того же события:

```
a26365766a6861726d6f6e79a46d737472656e677468fb3fe1353f7ced91686c6174656e6379fb405d8ccccc
cccccc668656e74726f7079fb3fe87ae147ae147a6d64656c74615f737472656e677468fb3fd3ae147ae147ae
6d64656c74615f6c6174656e6379fbbed051eb851eb85266737461747573656564726966746d706174746572
6a6370752f6c6f6164a26b6d6972726f72a46b6d6972726f72656b6a6370752f6c6f61646b737fb3fd3ae147
ae147a6474fb000000000001f58a
```

Зеркальный импульс описывается короткой структурой:

```json
{"k":"mirror","p":"cpu/load","s":-0.28,"t":128442}
```

## Режим `--pipe-cbor`

CLI может работать в потоковом режиме CBOR. Каждая строка stdin трактуется как hex-представление CBOR-импульса. Ответы печатаются в stdout также в виде hex-строк CBOR-пакетов. Полезно для интеграционных тестов и скриптов.

Пример запуска:

```
cargo run -p liminal-cli -- --pipe-cbor
```

После старта можно вручную ввести hex-строку импульса и получить hex-ответ с событиями/метриками.

## Nexus Bridge (WebSocket / Stream Core)

Начиная с версии v0.7 мост может работать как WebSocket-шлюз между ядром Liminal и внешними клиентами Nexus.

* **Адрес по умолчанию**: `ws://127.0.0.1:8787` (изменяется флагом `--ws-port`).
* **Формат сообщений**: JSON-текст или CBOR-бинар. Клиент может выбрать формат, отправив поле `"format": "json"|"cbor"` в приветственном сообщении. Для CBOR используются короткие ключи (`"cmd"`, `"q"`, `"d"`, `"ev"`, `"mt"` и т. д.) совместимые с протоколом v0.2.
* **Ping/Pong**: сервер отправляет `Ping` каждые 30 секунд; клиент должен отвечать `Pong`. При отсутствии ответов соединение закрывается.
* **Reconnect**: клиенты могут переподключаться без предварительного уведомления. Сервер отслеживает последнее время активности и автоматически удаляет устаревшие сессии.

### Команды клиента

```json
{"cmd":"impulse","data":{"pattern":"cpu/load","kind":"query","strength":0.8}}
{"cmd":"lql","q":"SELECT cpu/load WINDOW 1000"}
{"cmd":"policy.set","data":{...}}
{"cmd":"subscribe","pattern":"mem/free"}
```

* `impulse` – добавляет импульс ядру (см. структуру `Impulse`).
* `lql` – исполняет LQL-запрос; ответ приходит событием `{"ev":"lql",...}`.
* `policy.set` – задел под будущие политики; пока просто регистрируется в логах.
* `subscribe` – синтаксический сахар к `LQL SUBSCRIBE`, возвращающий поток событий `view`.

### Исходящие события

Любое событие ядра, доступное через `liminal_pull`, транслируется в подключённые WebSocket-сессии:

```json
{"ev":"view","meta":{"pattern":"mem/free","stats":{...},"source":"ws"}}
{"ev":"metrics","metrics":{"cells":128,"sleeping":0.42,...}}
{"ev":"lql","meta":{"select":{...},"source":"ws"}}
```

Поле `meta.source = "ws"` помогает отличить сетевые публикации от локальной консоли.

### Пример CBOR-кодирования

Команда `subscribe` в CBOR-hex:

```
a264636d64a26b7375627363726962656b706d656d2f66726565
```

Событие `view` в CBOR-hex:

```
a26365767476696577a26d736f75726365637773a26d7374617473a362636f756e74
19 03e8
```

(пробелы вставлены для читаемости; реальные байты выдаются подряд).

### CLI-команды для WebSocket-моста

В интерактивном CLI добавлены служебные команды:

```
:ws info              # показать подключённых клиентов и статус nexus-клиента
:ws send <json>       # отправить произвольную команду во внешний Nexus (--nexus-client)
:ws broadcast <json>  # вручную опубликовать событие всем ws-подписчикам
```

При запуске без `--nexus-client` CLI поднимает локальный сервер. С флагом `--nexus-client <url>` CLI подключается к внешнему Nexus и реплицирует полученные события в локальный поток.
