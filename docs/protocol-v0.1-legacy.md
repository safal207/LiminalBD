# Протокол взаимодействия v0.1

## Impulse

Импульсы — основной способ взаимодействия с тканью.

```text
struct Impulse {
    kind: ImpulseKind,      // Query | Write | Affect
    pattern: String,        // произвольная строка, разбивается на токены
    strength: f32,          // 0.0..1.0, интенсивность
    ttl_ms: u64,            // время жизни
    tags: Vec<String>,      // произвольные метки источника
}
```

`pattern` индексируется по простым токенам (буквы/цифры/`_`/`/`).
`strength` влияет на энергию клетки и вероятность деления/пробуждения.

## Metrics

Метрики рассчитываются наблюдателем (`morph_mind`).

```text
struct Metrics {
    cells: usize,           // количество живых клеток
    sleeping_pct: f32,      // доля спящих клеток (0.0..1.0)
    avg_metabolism: f32,    // средняя метаболическая активность
    avg_latency_ms: f32,    // среднее время ответа
}
```

На основе метрик формируются подсказки (`Hint`), которые управляют циклом жизни.

## Symmetry Loop

Петля симметрии (v0.8) отслеживает баланс импульсов и публикует событие `harmony`:

```json
{"ev":"harmony","meta":{"strength":0.51,"latency":110.0,"entropy":0.72,"status":"drift","mirror":{"k":"mirror","p":"cpu/load","s":-0.24,"t":128442}}}
```

Поле `mirror` присутствует, если сгенерирован корректирующий сигнал. В CLI доступны:

* опция `--mirror-interval <ms>` — период обновления (по умолчанию `2000` мс);
* команда `:harmony` — вывод текущих метрик и статуса (`OK`/`DRIFT`/`OVERLOAD`).

## ResonantModel

`ResonantModel` описывает устойчивое состояние кластера, которому может быть присвоено имя и набор управляющих параметров. Поля сериализуются одинаково для JSON и CBOR; для CBOR используются те же строковые ключи.

| Поле | Тип | Описание |
| --- | --- | --- |
| `id` | `String` | Уникальный идентификатор резонансной модели (например, `core/primary`). |
| `edges` | `Vec<Edge>` | Набор граничных связей. Каждая связь описывается структурой `{ "from": String, "to": String, "weight": f32 }`. |
| `tension` | `TensionState` | Текущее натяжение. См. пример отчёта ниже. |
| `persistence` | `PersistenceMode` | Политика хранения (`memory`, `snapshot`, `archive`). |
| `last_awakened_ms` | `u64` | Unix-время последнего пробуждения в миллисекундах. |
| `latent_traits` | `Map<String, f32>` | Карта латентных параметров (например, `entropy_bias`). |
| `tags` | `Vec<String>` | Пользовательские метки для фильтрации и маршрутизации событий. |

### Список граней резонанса

Команды CLI:

```text
resonance edges <model-id> [--weight-min <f32>] [--weight-max <f32>] [--format json|table]
```

Ответ `edges` в формате JSON:

```json
{
  "model": "core/primary",
  "edges": [
    { "from": "limbic/alpha", "to": "cortex/β", "weight": 0.72 },
    { "from": "cortex/β", "to": "reticulum", "weight": 0.54 }
  ],
  "revised_at_ms": 1712134412098
}
```

CBOR версия следует тем же ключам; CLI поддерживает `--format cbor` и сохраняет бинарный ответ в файл. Для WebSocket доступна команда `{"cmd":"resonance.edges","args":{"model":"core/primary"}}`.

### Отчёты о натяжении

Отчёт о натяжении (tension report) публикуется через `:tension <model-id>` в CLI или событием `{"cmd":"resonance.tension","args":{"model":"..."}}` по WS. Пример JSON:

```json
{
  "model": "core/primary",
  "ev": "tension",
  "scope": "limbic/alpha",
  "pressure": 0.81,
  "gradient": [0.22, 0.41, -0.18],
  "stability": "marginal",
  "updated_ms": 1712134420154
}
```

CBOR полезная нагрузка соответствует JSON и может быть десериализована в те же структуры. Клиенты должны ожидать не более одного отчёта в 500 мс; избыточные запросы могут приводить к троттлингу.

### Политики персистентности

`persistence` управляет долговечностью модели:

* `memory` — состояние хранится в оперативной памяти и сбрасывается при перезапуске.
* `snapshot` — периодический дамп в `liminal-db/state/*.cbor`. Минимальный интервал: 5 секунд.
* `archive` — журналируется каждое изменение в `liminal-db/archive/<model-id>/<ts>.jsonl`.

Клиенты должны сохранять идентификатор снимка (`snapshot_id` из ответа `awaken.set`) для последующего восстановления. Если персистентность `archive`, события `awaken` включают поле `archive_seq` для согласования.

## Пробуждение (Awakening Flow)

Пробуждение управляет переводом модели из латентного состояния в активное. Поток запросов:

1. `awaken.get` — получить текущее состояние и проверить метаданные.
2. `awaken.set` — применить новую конфигурацию и зафиксировать режим персистентности.
3. `awaken.commit` (опционально) — сигнализировать о готовности (автоматически вызывается CLI при `--commit`).

### JSON/CBOR нагрузки

#### `awaken.get`

CLI: `awaken get <model-id> [--format json|cbor]`

WS: `{"cmd":"awaken.get","args":{"id":"core/primary"}}`

Ответ:

```json
{
  "id": "core/primary",
  "status": "sleeping",
  "last_awakened_ms": 1712134312000,
  "persistence": "snapshot",
  "latent_traits": { "entropy_bias": 0.14 },
  "tension": { "pressure": 0.37, "stability": "stable" }
}
```

CBOR: идентичные ключи; типы `String`, `Float`, `Map`, `Array` сохраняются.

#### `awaken.set`

CLI: `awaken set <model-id> --persistence <mode> [--trait key=val ...] [--edge from:to=weight]`

WS: `{"cmd":"awaken.set","args":{"id":"core/primary","persistence":"snapshot","traits":{"entropy_bias":0.2},"edges":[{"from":"limbic/alpha","to":"cortex/β","weight":0.74}]}}`

Ответ подтверждения:

```json
{
  "id": "core/primary",
  "status": "primed",
  "snapshot_id": "1712134412-4c5d",
  "tension": { "pressure": 0.48, "stability": "attentive" },
  "edges": 2
}
```

CBOR полезная нагрузка возвращает такие же ключи; `edges` сериализуется как целое число.

### Поток пробуждения

После `awaken.set` можно запрашивать состояние через `awaken.get` до получения `status = "active"`. CLI команда `awaken watch <model-id>` подписывает клиента на события `awaken` и `tension` через WS.

## События `awaken` и `introspect`

Оба события публикуются на каналах WS (`/ws/awaken`, `/ws/introspect`) и ретранслируются в CLI через `:events`.

### Схема события `awaken`

```json
{
  "ev": "awaken",
  "id": "core/primary",
  "status": "active",
  "tension": { "pressure": 0.52, "stability": "poised" },
  "archive_seq": 18,
  "edges_changed": ["limbic/alpha→cortex/β"],
  "emitted_ms": 1712134415123
}
```

Событие отправляется:

* при успешном переходе из `sleeping` в `primed` или `active`;
* при изменении персистентности (включая `archive_seq` при `archive`).

### Схема события `introspect`

```json
{
  "ev": "introspect",
  "id": "core/primary",
  "focus": "latent_traits",
  "delta": { "entropy_bias": { "old": 0.12, "new": 0.16 } },
  "edges_sample": [
    { "from": "limbic/alpha", "to": "cortex/β", "weight": 0.74 }
  ],
  "tension": { "pressure": 0.49, "gradient": [0.18, 0.07, -0.03] },
  "emitted_ms": 1712134416198
}
```

`introspect` возникает после явного запроса `introspect <model-id>` или по расписанию (`--introspect-interval`). Клиенты должны использовать `focus` для фильтрации обновлений. CBOR события используют одинаковые ключи и типы; поле `delta` сериализуется как карта карт.

### Команды `introspect`

CLI: `introspect <model-id> [--traits|--edges|--tension]`

WS: `{"cmd":"introspect","args":{"id":"core/primary","include":["traits","tension"]}}`

Ответ:

```json
{
  "id": "core/primary",
  "latent_traits": { "entropy_bias": 0.16 },
  "tension": { "pressure": 0.49, "stability": "poised" }
}
```

CBOR возвращает те же поля. Ответ `introspect` всегда сопровождается событием `introspect` с дополнительным контекстом.
