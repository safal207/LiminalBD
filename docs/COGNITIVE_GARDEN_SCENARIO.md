# Сценарий «Cognitive Garden»

Короткий, живой сценарий демонстрации командного интерфейса LIMINALDB. Подходит для CLI-дэма или видеозаписи и показывает, как система сажает, растит и собирает семена, а затем осмысляет опыт через Mirror.

---

## 🪴 1. Подготовка

```bash
cargo run -p liminal-cli -- --store ./data --ws-port 8787
```

Ожидаемый вывод:

```
LIMINALDB ready. Garden awake.
```

---

## 🌱 2. Посадка двух семян

```bash
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:seed plant ViewWatch mem/free {"window_ms":3000,"every_ms":1000} 90000
```

Ждём логи:

```
SEED id=1 stage=Planted kind=BoostToken target=cpu/load
SEED id=2 stage=Planted kind=ViewWatch target=mem/free
```

---

## ☀️ 3. Рост

Через 30 секунд:

```bash
:seed garden
```

Вывод:

```
ID  KIND        STAGE      VITALITY  MUTATION  YIELD
1   BoostToken  Blooming   0.83      0.02      0.41
2   ViewWatch   Sprouting  0.71      0.00      0.27
```

---

## 🔄 4. Действие и эффект

Проверяем живость поля:

```bash
:status
```

Ответ:

```
HARMONY load=0.47  emotional_load=0.68
ACTIVE_SEEDS=2  avg_latency↓15%
```

---

## 🌾 5. Уборка урожая

Через ~90 секунд:

```bash
:seed harvest
```

Ожидаемое сообщение:

```
HARVESTED 2/2 seeds. Total yield=+0.68
Mirror updated: +2 epochs (Seed kind)
```

---

## 🪞 6. Отражение

```bash
:mirror top 5
```

Результат:

```
#  KIND   IMPACT   NOTE
1  Seed   +0.34    BoostToken cpu/load improved latency
2  Seed   +0.28    ViewWatch mem/free stabilized harmony
```

---

## 🧠 7. Завершение

```bash
:snapshot
```

Лог завершения:

```
SNAPSHOT complete. Garden state saved (2 harvested, 0 active)
```

---

## 🎯 Результат

Система посадила и вырастила семена без ручного вмешательства. Garden выдал положительный yield. Mirror зафиксировал новые эпохи «Seed», усилив модель резонансов. После перезапуска можно восстановить тот же «урожай» из сохранённого снапшота.

---

## ✅ 8. Проверка

После прохождения всего сценария запустите тесты рабочего пространства, чтобы удостовериться, что система собирается и выполняет основные проверки без ошибок:

```bash
cargo test
```

Команда должна завершиться сообщением `test result: ok`, подтверждающим, что все тесты прошли успешно.

