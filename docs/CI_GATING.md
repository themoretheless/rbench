# Гейтинг производительности в CI

Кулинарная книга: как превратить сохранённые запуски `rbench` в проверки, падающие на регрессии. Все проверки работают офлайн по уже записанным наблюдениям и не запускают измерения повторно. Коды выхода единообразны: `0` — пройдено, `1` — нарушение/регрессия, `2` — неопределённость/недоступность/ошибка использования.

## Абсолютные бюджеты на метрику: `check`

Проверяет каждое наблюдение метрики против абсолютных границ (без статистического вывода).

```sh
# верхняя граница
cargo rbench check .rbench/run --metric wall --max 200
# нижняя граница или диапазон; сузить кейсы через --filter
cargo rbench check .rbench/run --metric wall --min 8 --max 16 --filter static
```

## Производная throughput: `throughput`

Единицы работы объявляются в бенчмарке (`work_units`/`Suite::work_units`/`Recorder::work_units`); throughput выводится в units/s (MiB/s для `bytes`).

```sh
cargo rbench throughput .rbench/run --min 1000        # не ниже порога
cargo rbench throughput .rbench/run --min 8 --max 16  # диапазон
```

## Декларативные бюджеты: `gate`

JSON-конфиг с абсолютными (`max`/`min`) и относительными (`max_regression_percent`, требует baseline) бюджетами на точные пары кейс/метрика. Единица должна совпадать; неопределённость по умолчанию не проходит.

```json
{ "budgets": [
  { "case": "sort/stable/512", "metric": "wall", "unit": "ns", "max": 250 },
  { "case": "sort/stable/512", "metric": "wall", "unit": "ns", "min": 1 }
] }
```
```sh
cargo rbench gate .rbench/run --config budgets.json
# с относительным порогом к baseline:
cargo rbench gate .rbench/run --baseline @main --config budgets.json
# политика неопределённости: fail (по умолчанию) | warn | record
cargo rbench gate .rbench/run --config budgets.json --uncertainty warn
```

## Статистическое сравнение: `compare --check`

Непараметрические интервалы с поправкой на множество метрик; `Inconclusive` не выдаётся за эквивалентность.

```sh
cargo rbench compare .rbench/baseline .rbench/candidate --check --threshold 5
# сузить семейство и выбрать политику неопределённости:
cargo rbench compare .rbench/baseline .rbench/candidate --check \
  --filter sort --metric wall --uncertainty warn
```

## Пример шага GitHub Actions

```yaml
- name: Benchmark gate
  run: |
    cargo rbench bench --repetitions 12 -o .rbench/ci -- --profile quick
    for run in .rbench/ci/*/run.json; do
      cargo rbench gate "${run%/run.json}" --config budgets.json
    done
```

Общие правила: каталог результата должен быть новым (данные не перезаписываются); изменённые контракты/окружение отклоняются; бюджеты с `baseline=0` требуют абсолютной границы. Для машинной обработки у `check`/`gate`/`compare`/`throughput`/`stat` есть `--json`.
