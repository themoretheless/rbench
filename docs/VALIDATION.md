# Проверка прототипа 0.1.0

Проверено локально 2026-09-07 на macOS/Metal, Rust nightly 1.98. Это рабочий исследовательский прототип, не завершённая приёмка всей архитектуры.

## Вторая итерация: ещё 20 возможностей

Реализованные пункты и команды: [NEXT20.md](NEXT20.md).

- 38 тестов workspace проходят: 26 библиотечных и 12 CLI. Строгий Clippy всех targets проходит. Проверены release-сборки CLI/библиотеки/examples.
- `.rbench/workflow20/0-rbench-workloads`: автоматическая сборка зарегистрированного target, 3 независимых процесса × 2 cases × 8 samples = 48 наблюдений. Общий lazy fixture, seed 42, CPU tags и throughput. Это smoke, не acceptance производительности; короткие samples отмечены предупреждением.
- Проверены worker profile overrides, exact/glob/tag selection и dry-run без workload; runner preview не создал output.
- CSV/JSONL сохранили 48 observations; note не изменила hash исходного Run. Bundle export/unpack восстановил точные bytes run.json; note доступна и с другим пустым store.
- HTML содержит SVG raw plots, throughput и заметку. Trend разделяет графики по context hash. Браузерная визуальная/интерактивная приёмка остаётся непроведённой после ранее полученного запрета browser URL policy; структурные тесты не выданы за визуальную проверку.
- `.rbench/git-control-result/run`: 12 A/B пар двух локальных commits с известной добавленной sleep (1 ms → 3 ms), Regression. Исходные HEAD и незакоммиченный файл сохранились точно. Этот тест проверяет обнаружение контрольного эффекта, не скорость реального алгоритма.
- `.rbench/git-isolation-result/run`: 6 пар с намеренно унаследованным общим `CARGO_TARGET_DIR`; сборки изолированы, общий путь не создан, binaries различны, Regression; исходный checkout сохранён.
- Negative tests: bundle traversal/tampering не создаёт output, unavailable не проходит при `--uncertainty record`, повторный CI export не перезаписывает файл.
- Финальный HTML: 6 SVG plots / 48 точек проверены XML-парсером, координаты конечные и внутри области графика; MiB/s присутствует в отчёте. Это структурная проверка. CI YAML успешно разобран YAML-парсером.
- CI-шаблон сформирован локально. В GitHub он не устанавливался и удалённый job не запускался. Использование hosted runner в шаблоне означает smoke, а не контролируемый performance gate.

## Итерация удобства

Добавлены и проверены init/discover/bench, baseline aliases, прогресс/ETA, parameter matrix, checked operations, HTML tables, declarative budgets и native Forma golden scenarios. Инструкция: [USABILITY.md](USABILITY.md).

- 28 тестов workspace прошли; строгий Clippy core/CLI и отдельного Forma adapter прошёл.
- Полный путь нового Cargo package: init сохранил TOML-комментарий, discover нашёл target, bench собрал и запустил 3 процесса / 3 размера input; baseline save/report/gate успешно использовали `@usability-main`. Артефакт `.rbench/usability-first/0-usability-fixture-rbench`.
- `.rbench/forma-scenarios-v2`: 6 реальных сценариев × 3 процесса × 96 кадров = 1728 измеренных кадров; checkpoints до/после совпали точно. Image возвращает Unsupported.
- Просмотрены PNG static, hover, animation, scroll, resize и text. Это reference images текущей сборки, не независимый rendering oracle.
- `.rbench/forma-goldens-v2` содержит 12 RGBA/PNG checkpoints. Бюджеты нулевых geometry uploads прошли для static/hover/animation.
- `.rbench/forma-golden-negative`: один изменённый pixel в копии reference вызвал mismatch на pre-check frame 0, измерение не началось; сохранён Failed и stderr.
- HTML проверен тестом структуры и экранирования. Визуальная и интерактивная проверка браузером не выполнена: browser security policy запретила открытие локального файла.

## Автоматические проверки исходного прототипа

- `cargo test --workspace --offline`: 20 тестов успешно, включая реальные child processes success/failure/timeout, повреждённый protocol, immutable output, импорт Forma и неполный набор случаев.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: успешно.
- Release-сборки workspace, examples и отдельного Forma adapter: успешно.
- Контрактные тесты: свежий input, однократный Drop, lazy registration, неверные значения/availability, несовместимые окружения, missing pairs, zero baseline, независимость process-level единиц, escaped reports, failed realloc/shrink/zeroed allocation.
- Seeded Monte Carlo: 2000 IID uniform выборок проверяют coverage интервала медианы. Это ограниченная проверка; автокорреляция, дрейф и все распределения не покрыты.

## Реальные запуски

Артефакты хранятся локально в игнорируемой `.rbench/`; они не входят в пакет библиотеки. Каждый run содержит план, SHA-256 бинарников и логи.

| Запуск | Проверка | Результат |
|---|---|---|
| `sort-aa-final` | 12 A/A пар, по 8 batches в двух sort cases | Оба результата Inconclusive; регрессия не объявлена |
| `control-ab` | 12 пар `/bin/sleep`, 10 ms против 30 ms | Regression, `compare --check` завершился кодом 1 |
| `cancel-check` | SIGINT во время sleep | Быстрый выход, сохранён cancelled |
| `forma-final` | Реальный renderer Forma, Metal, 3 процесса × 96 кадров после 20 warmup | Complete; 288 submit и 288 completed observations |
| `forma-final` budget | geometry.uploads ≤ 0 | Все 3 фазовых наблюдения прошли |
| `forma-gpu` | Запуск в окружении без доступного адаптера | Failed с диагностикой; ноль вместо недоступного измерения не записан |

Forma: медиана наблюдений CPU submit 41 µs, completed 1.302521 ms; 3936 Rust allocations за 96 кадров в каждом процессе, повторных geometry uploads — 0. Это диагностические числа для конкретного forced static offscreen scene с включённым atomic allocator instrumentation. Они не являются оконным FPS, GPU timestamp duration или доказательством ускорения Forma. Полезная работа контрольного A/B — добавленная process sleep, а не реальный алгоритм.

У A/A интервалы изменения: stable [-7.290%, 2.927%], unstable [-5.710%, 2.681%]. При пороге 5% этих данных недостаточно для заявления об эквивалентности. Один A/A прогон не измеряет общую частоту ложных срабатываний.

## Границы текущей версии

- Native window/present lifecycle, GPU timestamp adapter, async/browser drivers и macro DSL ещё не реализованы. Golden comparison реализован для отдельных checkpoints; полная последовательность кадров не проверяется.
- Allocator считает Rust process scope; нет OS RSS/CPU provider, thread-local allocator scope и отдельного phase peak. Lifetime peak явно отличается от phase peak.
- HTML автономный, с таблицей, фильтром, сортировкой и раскрытием contracts; исторический dashboard отсутствует.
- Сборка проверена на текущем macOS toolchain. Заявленный MSRV 1.85 и Windows/Linux ещё не проверены. На Unix убирается process group; на других ОС только непосредственный child.
- Runner lock исключает параллельные rbench, но не другую нагрузку, температурный дрейф или изменения частот. После аварийного kill stale lock может требовать ручного удаления после проверки отсутствия runner.
- Окружение фиксируется частично. Для значимых настроек нужны explicit plan.env и contract; секреты в plan.env попадут в локальные артефакты. Автоматического редактирования system settings нет.
- JSON-схема валидируется, но crash-durable atomic directory publication отсутствует. Окончательный статус — `status-final.json`; отсутствие финального файла означает незавершённый запуск.
- Legacy importer намеренно принимает известную матрицу Forma, а не произвольные будущие схемы. Сохранённые aggregates не превращаются в raw frame samples.
- Эффект instrumentation, overhead относительно handwritten/Criterion/Divan и статистическая устойчивость при систематическом дрейфе ещё требуют отдельных экспериментов.

Расширенные критерии остаются в [ROADMAP.md](ROADMAP.md). Архитектурные документы описывают также будущие возможности; для состояния реализации используйте README и этот файл.
