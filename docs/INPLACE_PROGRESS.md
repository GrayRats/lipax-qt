# Сохранённый прогресс inplace — 2026-10-11

Версия **1.3.9**. Продолжение фаз 2–3 сохранено в исходниках рабочего дерева.
Полная исходная задача не завершена. Коммит не создавался: предшествующие
локальные изменения сохранены вместе с текущими. Следующие задачи:
[INPLACE_REMAINING_TASKS.md](INPLACE_REMAINING_TASKS.md).

## Практические примеры и изменения

- **engine.exe, исходный PID 100656:** соседние части диалога не блокируют
  друг друга; короткий дубль `OAD` внутри `LOAD` не создаёт отдельное препятствие.
- **CrossxMars.exe, исходный PID 129055:** полный диалог владеет повторными
  чтениями своих исходных пикселей. Пересекающиеся crops имени и HUD объединяются
  и читаются RapidOCR заново. `Mik`/`ca` превратились в фактическое чтение `Mika`,
  а расширенный HUD сохранил ранее потерянный IP suffix `:1123`.
- OCR evidence хранит отдельные text/confidence/rect spans; invalid geometry,
  слабые чтения и текст вне исходного поля исключаются. Совпадение рамок само по
  себе не позволяет скрыть независимую надпись. Площадь пересечений считается
  объединением, без повторного подсчёта overlapping spans.
- После объединения сбрасывается кэш OCR даже при высоком IoU и прежнем track id.
  Частичные чтения, ожидающие regrouping, не отправляются на перевод как готовые.
- Label/value columns внутри одного исходного поля образуют baseline rows.
  Cross x Mars HUD теперь даёт 7 строк и left alignment вместо ложных 12 строк.
- Пустые/отброшенные результаты OCR получают negative cache на 5 секунд.
  Новый контент, размеры кадра, настройки recognition, force и timeout вызывают
  повторное чтение. Ошибки I/O и отмена не создают negative cache.
- Text-only OCR снова использует detector geometry/background, не устаревшие
  координаты предыдущего backend-а. Геометрия/ревизии стабильны на одинаковом кадре.

Реализация общая: имена процессов, PID, `Mika` и IP suffix используются только
в тестовых примерах. Production не угадывает слова и не подставляет fixture text.

## Файлы

- `crates/core/src/layout/ocr_geometry.rs` — новое описание валидированных OCR
  spans/fields, row geometry, duplicate ownership и необходимость fresh union OCR.
- `layout/{engine.rs,tracker.rs,mod.rs}` — evidence lifecycle, объединение crops,
  cache invalidation, source geometry и подавление подтверждённых дублей.
- `crates/core/src/pipeline.rs` — source-associated text перед переводом,
  rescan regrouped regions, negative cache подтверждённых пустых/слабых чтений.
- `crates/core/Cargo.toml`, `tests/inplace/rust/ocr_ownership.rs` — отдельный
  Rust test target; 20 meaningful regressions, включая реальные OCR replay cases.
- `tests/inplace/rust/bench.rs` — diagnostics по этапам/сканам, actual cropped
  RapidOCR, заранее заданные переводы по полным исходным чтениям, repeat OCR count.
- `tests/inplace/python/test_inplace_screenshots.py` — проверки обязательных
  dialogue/name/HUD fields, сохранения IP suffix, fresh OCR union и boundaries.
- `tests/inplace/fixtures/` — два исходных реальных кадра, client/screen geometry,
  наблюдавшиеся RapidOCR spans и deterministic Russian translations. OCR replay
  inputs явно отмечены как наблюдения, **не независимая ground-truth разметка**.
- `docs/INPLACE_PHASE_2_3.md`, `INPLACE_REMAINING_TASKS.md`, этот файл и test README —
  актуальное состояние и дальнейшие действия. Удалённых файлов приложения нет.

Kizurium reference code/tests не изменялись. Использованы принципы проверки
source coverage, имени отдельно от body, structural grouping и cache invalidation
из его реальных tests; GTK/Pango и AGPL implementation в MIT-проект не копировались.
Новых runtime-зависимостей/шрифтов нет. Qt shaping и controlled bundled-font policy
сохранены; точная идентичность исходного шрифта не заявляется.

## Выполненные проверки

| Проверка | Результат и предел применимости |
|---|---|
| Полный Rust до последнего cache change | **398 passed, 0 failed, 15 ignored**; `tmp/rapid-live/rust-final.log` |
| OCR ownership/cache target после последних правок | **20 passed, 0 failed**, 2,70 s; `tmp/rapid-live/join-unit.log` |
| Полный Python/RapidOCR | **26 passed, 0 failed**, 214,40 s; `tmp/rapid-live/python-final.log` |
| Повтор трёх затронутых RapidOCR/Qt cases после cache change | **3 passed, 23 deselected**, 27,69 s; `tmp/rapid-live/python-cache-final.log`; не суммировать с 26 |
| Новые QML/JavaScript | **13 passed, 0 failed**; `tmp/rapid-live/qml-final.log` |
| Существующие QML | **106 passed, 0 failed**; `tmp/rapid-live/qml-existing-final.log` |
| Production Qt text-measure helper | executable завершился с кодом 0; `tmp/rapid-live/qt-measure.json` |
| Сборка приложения | успешна до последних cache changes, 13,76 s; `tmp/rapid-live/app-final.log`; повтор ещё нужен |
| Approved visual reference comparison | **NOT RUN**: независимо утверждённых эталонов нет |
| Новая live app/translator E2E проверка | **NOT RUN**: оба указанных PID к моменту checkpoint уже завершились |

Русский перевод в PNG задан детерминированно; внешний translation service не
вызывался. Финальные PNG/TXT/JSON сохранены по запросу пользователя в
`docs/screenshots/test-game/translations/`. Все промежуточные outputs/caches/builds
лежат в игнорируемой `tmp/`; permanent test fixtures находятся отдельно в tests.

## Наложение и измерения

На recorded engine.exe: 5 accepted readings → 4 rendered fields и 1 подтверждённый
дубль; покрытие двух dialogue OCR boxes **99,46% /99,20%**. На Cross x Mars:
11 accepted → 4 rendered fields и 7 подтверждённых дублей; name **98,41%**,
dialogue **97,11%**, HUD **100%**. Required fields не используют `beside-original`.
Qt/QML проверяет фактический fit/overflow и неизменность pixels вне allowed plates.

Это покрытие OCR-derived rectangles, а не независимая OCR accuracy или доказанная
точность baseline/font proportions. В кадре остаются невыделенные HUD labels;
у engine.exe есть ложное чтение элемента сцены «0».

На следующем одинаковом кадре у обоих процессов фактически **0 OCR jobs**.
Cross x Mars стабилизируется после двух scans (18 cropped OCR calls), engine.exe
после одного (14 calls). Это counts diagnostic harness, не application throughput.

CPU Ryzen 5 5600, native ONNX Runtime 1.29.0, RapidOCR CPU 2 threads/no GPU,
Qt 6.12.0 offscreen, software rendering. Shared QTextLayout helper использует
test IPC; его latency не является latency in-process CXX-Qt моста приложения.
Новый сопоставимый benchmark после cache changes **NOT RUN**. Release baseline
до negative cache сохранён в `tmp/rapid-live/ownership-baseline/inplace_bench`
и `.sha256`; нужно собрать candidate и провести замер без параллельных тестов.
Исторические paired detector/slant measurements остаются в phase report и не
приписываются новым ownership/cache changes.

## С чего продолжить

1. Выполнить полный Rust-прогон и собрать приложение/release после cache changes.
2. Провести paired native OCR/cache/profile на одинаковых recorded frames;
   сохранить raw samples, условия и фактические ограничения в `tmp/`.
3. Устранить потерю других HUD labels/ограничение 16 candidates и ложные scenery
   readings; проверить новые реальные кадры и изменение source content/geometry.
4. Провести live app overlay E2E после появления игр, затем fractional DPI,
   multimonitor, Wayland/XWayland и service translation. Независимо проверить
   baseline/пропорции и утвердить visual references.
