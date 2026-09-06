# rbench

Своя Rust benchmark-библиотека и runner: короткие операции, отдельные процессы и сценарии приложения в одном формате наблюдений. Реализован рабочий прототип 0.1.0; полная приёмка и переносимость ещё проверяются.

Новые удобства: **init, обнаружение workspace targets, именованные baseline, прогресс/ETA, матрицы параметров, проверка результата, HTML-таблицы, бюджеты и Forma golden-сценарии**. [Полный рабочий процесс](docs/USABILITY.md).

Добавлены ещё [20 возможностей рабочего цикла](docs/NEXT20.md): профили, Git-сравнение, `last`, dry-run, фильтры/теги, fixtures/фазы, throughput, seed, история, графики, экспорт/bundle, заметки и CI-политика.

Следующая очередь: [оставшиеся 20 функций и порядок реализации](docs/FINAL20.md) — пока план.

## Быстрый запуск

```sh
cargo build --release --workspace --offline
cargo build --release --examples --offline
cargo rbench doctor
# Список без исполнения workload:
target/release/examples/sort --list
cargo rbench run --program target/release/examples/sort --protocol \
  --repetitions 12 -o .rbench/sort -- --json
cargo rbench report .rbench/sort -o .rbench/sort.html
```

`--offline` подходит при наличии зависимостей в Cargo cache; при первой сборке его можно убрать. Alias `cargo rbench` настроен в этом workspace. Для других проектов: `cargo install --path crates/cargo-rbench --offline`.

## API библиотеки

Подключение: `rbench = { path = "/path/to/rbench/crates/rbench" }`. Для Cargo benchmark target задайте `harness = false`.

```rust
use rbench::{DropPolicy, Suite};
fn main() -> rbench::Result<()> {
    let mut suite = Suite::new("collections");
    suite.bench_with_input(
        "sort/1000",
        || (0..1000u64).rev().collect::<Vec<_>>(),
        |input| input.sort_unstable(),
        DropPolicy::InsideTiming,
    ).parameter("elements", 1000);
    suite.main()
}
```

Каждая операция получает свежий input. Его создание и уничтожение находятся вне измерения; `DropPolicy` определяет уничтожение **результата** операции. Внутренние batch ограничены 64 входами, калибровка ограничена числом операций. Простые замыкания регистрируются через `suite.bench`. `--filter` ищет подстроку, `--samples`, `--sample-ms`, `--warmup-ms` управляют сбором. Исполнение требует release-сборку.

## Сравнение и автоматические проверки

```sh
cargo rbench run --program /path/to/candidate --baseline /path/to/baseline \
  --protocol --repetitions 12 -o .rbench/ab -- --json
cargo rbench compare .rbench/ab --threshold 5 --check
# Или два исторических запуска:
cargo rbench compare .rbench/old .rbench/new --json
# Абсолютный бюджет на каждое наблюдение, без статистического вывода:
cargo rbench check .rbench/forma --metric geometry.uploads --max 0
```

Runner чередует AB/BA, последовательно запускает процессы, сохраняет stdout/stderr, план, SHA-256 бинарников/fixtures и сырые наблюдения. Каталог результата должен быть новым: существующие данные не перезаписываются. Без `--protocol` измеряется длительность процесса целиком, включая запуск и ожидание завершения, с разрешением polling около 1 ms.

Сравнение использует независимые процессы, медианы и непараметрические интервалы с поправкой на множество метрик. Недостаточные данные дают `Inconclusive`. `compare --check`: 0 — пройдено, 1 — регрессия, 2 — неопределённость/недоступность/ошибка. `--filter` и `--metric` позволяют заранее выбрать проверяемое семейство. Изменённые контракты и окружение отклоняются; метрики с baseline=0 требуют абсолютного бюджета.

Для разных argv/env/cwd, fixtures и контрактов используйте `run --plan plan.json`; схема примера — [docs/example-plan.json](docs/example-plan.json). Таймауты, ошибки, отмена и недоступные измерения сохраняются явно. `status-final.json` — окончательный статус; `status.json` — начальная запись, её наличие само по себе не означает успех.

## Forma и дополнительные метрики

- `Recorder` принимает наблюдения из собственного event loop приложения; сбор записей можно вынести за измеряемую фазу.
- `TrackingAllocator<System>` подключается явно и считает Rust allocations/reallocations/live/lifetime peak; native/driver allocations в него не входят.
- `import-forma` импортирует завершённые старые normal/paired результаты известной схемы Forma, сохраняя scope и отсутствующие значения.
- [Реальный offscreen-пример](integrations/forma/src/main.rs) использует renderer Forma и wgpu/Metal. Его зависимости изолированы от основного workspace. Сборка: `cargo build --release --offline --manifest-path integrations/forma/Cargo.toml`; затем бинарник запускается runner с `--protocol`.

Интеграция сейчас ссылается на локальный исследовательский снимок `research/private/forma/vector-ui`; для другого checkout исправьте path в её Cargo.toml. Интеграция включает static, hover, animation, scroll, resize и text; image явно Unsupported. Перед запуском нужно записать и просмотреть golden PNG, затем передать `--goldens DIR`. Измеряются submit/completed frames, allocations и geometry uploads с проверкой контрольных кадров. Оконный FPS и GPU timestamp duration этим примером не измеряются.

## Исследование и статус

[Проверки и ограничения](docs/VALIDATION.md) · [Roadmap](docs/ROADMAP.md) · [Архитектура](docs/ARCHITECTURE.md) · [Правила измерений](docs/MEASUREMENT.md)

Исследование охватило 779 записей; 614 репозиториев оставлены после скрининга. Это анализ документации с углублёнными выборочными просмотрами исходников ключевых движков, а не полный аудит или сравнительный запуск сотен библиотек.

[Итоги и методика](research/REPORT.md) · [Ключевые решения](research/FOCUSED.md) · [Потребности проектов, особенно Forma](docs/REQUIREMENTS.md) · [Реестр](research/REPOSITORIES.md) · [Решение по каждой записи](research/DECISIONS.tsv)
