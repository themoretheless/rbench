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
- Seeded Monte Carlo: IID coverage + AR(1)/drift diagnostics + synthetic A/A (`cargo rbench accept`). Это ограниченная synthetic приёмка; hardware A/A и полный FPR under thermal drift не закрыты.

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
- Allocator считает Rust process scope; OS RSS/CPU provider добавлен (`rbench::process`, вне timed batches). Нет thread-local allocator scope. Lifetime peak явно отличается от phase peak.
- HTML автономный, с таблицей, фильтром, сортировкой и раскрытием contracts; исторический dashboard отсутствует.
- Сборка проверена на текущем macOS toolchain. Заявленный MSRV 1.85 и Windows/Linux ещё не проверены. На Unix убирается process group; на других ОС только непосредственный child.
- Runner lock исключает параллельные rbench, но не другую нагрузку, температурный дрейф или изменения частот. После аварийного kill stale lock может требовать ручного удаления после проверки отсутствия runner.
- Окружение фиксируется частично. Для значимых настроек нужны explicit plan.env и contract; секреты в plan.env попадут в локальные артефакты. Автоматического редактирования system settings нет.
- JSON-схема валидируется; финальные `run.json`/`status-final.json` публикуются через temp+fsync+rename (`rbench::publish`). `cargo rbench recover` классифицирует interrupted dirs. Окончательный статус — `status-final.json`; отсутствие финального файла означает незавершённый запуск.
- Legacy importer намеренно принимает известную матрицу Forma, а не произвольные будущие схемы. Сохранённые aggregates не превращаются в raw frame samples.
- Эффект instrumentation, overhead относительно handwritten/Criterion/Divan и статистическая устойчивость при систематическом дрейфе ещё требуют отдельных экспериментов.

Расширенные критерии остаются в [ROADMAP.md](ROADMAP.md). Архитектурные документы описывают также будущие возможности; для состояния реализации используйте README и этот файл.

## Final twenty — 2026-09-07

All 50 workspace tests/doctests and the Forma GPU-negative-result test passed. Strict all-target/all-feature Clippy passed in the core and standalone Forma workspaces. Build/check evidence uses the installed nightly toolchain, not an independent MSRV run.

New real Metal validation: 1,152 normal frames plus 1,152 GPU timestamp samples across six scenarios and two processes; final smoke added 576 of each with the final counter contracts. Real winit surface: exactly 96 measured frames, resize/occlusion observed, automatic exit verified. A queued extra redraw during shutdown was found and fixed. Cold text: two new workers, first completed frame with model/renderer/resource initialization and post-golden validation. Matrix: four Metal viewport/DPI combinations passed separate checked goldens; four DX12 combinations explicitly Unsupported. Reference PNGs were visually inspected, including expected clipping at 640 physical pixels / DPI 2.

Known 1 ms → 3 ms sleep regression was found in isolated committed snapshots with 12 independent pairs. A separate **synthetic protocol** 100 → 200 → 100 Git fixture produced `nonmonotonic` with no culprit. The search audits a bounded first-parent range rather than assuming monotonicity. PR Markdown was generated locally; no comment was sent.

Privacy tests cover streamed split literals, escaped secrets, allowlisted inheritance, metadata/log/bundle non-disclosure and bounded shutdown when an inherited log pipe stays open. Resume preserves the parent artifact; retention protects baselines and linked sessions and only quarantines eligible owned directories. Profiler replay was verified with `/usr/bin/time -l`; the initial sandbox-denied sysctl attempt remained Failed, then an authorized host run passed.

Instrumentation cost was measured in a balanced system/tracked/phase A/B/C experiment (36 processes, 8 samples each). Both corrected comparisons were Inconclusive at the 5% practical margin; no zero-overhead or universal coefficient claim is made. Pilot simulation uses independent confirmation data, 2,000 fixed-seed experiments; the budget extrapolation remains a heuristic, not a power guarantee.

Local artifact paths and detailed feature contracts are in [FINAL20.md](FINAL20.md). Checksums are recorded in ignored `.rbench/final20-evidence.json`. Raw artifacts are intentionally not committed. Window image goldens, compositor scanout/drop counts, non-Metal GPU success paths, actual query-device-failure recovery, remote CI execution and new browser visual validation are **not** established by these checks.


## Hardening 2026-09-10 (Linux x86_64, rustc 1.85)

Implemented and checked in this workspace (not a substitute for macOS/Metal Forma evidence):

- `rbench::process` samples RSS/CPU outside timed batches on Linux (`/proc`) and macOS (`getrusage`). Unavailable OS returns `Availability::Unsupported`. Doctor reports live RSS.
- `Suite::process_metrics(true)` attaches `os.rss_peak` / `os.cpu_user` / `os.cpu_system` after measurement; example `tokenize` emits them with MiB/s throughput.
- `rbench::publish` writes JSON via temp+fsync+rename; runner publishes `run.json` / `status-final.json` atomically. `cargo rbench recover` classifies incomplete dirs with `partial-*.json`.
- `rbench::acceptance` Monte Carlo: IID coverage near 1−α; AR(1) under-coverage demonstrated; synthetic A/A FPR probe. CLI: `cargo rbench accept --seed N`.
- `Availability::PermissionDenied` distinguished from capability gaps.
- Forma adapter attaches process metrics outside timed frames and notes UMA non-additivity.
- Portability: validated here on Linux + MSRV toolchain 1.85.0. Windows providers remain unsupported (explicit error). Remote CI of generated workflow still not executed.

Workspace tests: library unit tests for process/acceptance/publish + existing contracts/CLI suite green under `--offline`.

## Next slice 2026-09-11

- Throughput is a first-class `Direction::Higher` observation when `work_units` is set; budgets may use `min` / relative regression.
- `cargo rbench compare|list` support `--exact/--glob/--exclude/--tag` (same Selection rules as Suite).
- `cargo rbench check --min` for Higher metrics.
- Windows RSS/CPU via GetProcessMemoryInfo/GetProcessTimes (compile-time; not runtime-validated here).
- `examples/overhead` handwritten vs Suite fixed-batch diagnostic.
- `.github/workflows/rbench-smoke.yml` runs test/clippy/accept/doctor/overhead.
- Accept battery includes heteroscedastic coverage regime.

## Compete slice 2026-09-11

- `rbench::perf`: Linux `perf_event_open` instructions/cycles; probe returns Available / PermissionDenied / Unsupported — never fabricated zeroes. `Suite::perf_counters(true)` samples sibling batches after wall samples.
- `rbench::isolate`: loadavg/governor/freq snapshot, `pin_to_cpu`, `RBENCH_PIN_CPU`, noise warnings. Not BenchExec-grade isolation.
- `cargo rbench accept --hardware`: live Instant A/A on current host with isolation context.
- `cargo rbench compete` / `docs/COMPETE.md`: honest scorecard vs Criterion/Divan/iai/hyperfine.
- `examples/compete.rs`: multi-metric demo (wall + throughput + process + perf).

## Close-the-gaps slice 2026-09-11

- `rbench::callgrind` + `cargo rbench callgrind`: Valgrind Callgrind Ir/Dr/Dw for iai-class deterministic CI; Unavailable when valgrind missing.
- `Suite::bench_batch` + `#[rbench::bench]` / `#[rbench::main]`: Divan-competitive hot-loop ergonomics / bookkeeping.
- `cargo rbench time`: hyperfine-class command wall timing (blocking wait, warmup/runs/shell/prepare/cleanup, markdown/json + optional Run).
- Scorecard Trails removed: hot-loop Competitive/Lead, Callgrind Lead (tied), command timing Lead/Competitive.
- Still open: published Criterion/Divan quiet-host numbers checked into docs after a dedicated quiet run, Forma window/Metal goldens.

## Isolation + AND gates slice 2026-09-11

- `rbench::isolate`: best-effort cgroup v2 enter via `RBENCH_CGROUP` / `RBENCH_CGROUP_CPUS` / `RBENCH_CGROUP_MEMORY_MAX`; snapshot exposes cgroup path + cpu.max + memory.max; never claims BenchExec parity.
- `budget` `groups` with `require: "all"` — conjunctive wall ∩ throughput ∩ RSS ship gates (`docs/examples/budgets-and.json`).
- `examples/bakeoff.rs` + `docs/BAKEOFF.md` — quiet-host Criterion/Divan comparison protocol without default deps.
