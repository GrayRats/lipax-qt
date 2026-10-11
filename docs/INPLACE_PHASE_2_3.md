# Inplace: продолжение фаз 2–3, версия 1.3.9

Дата: 2026-10-11. Это отчёт о выполненных изменениях и проверках, а не
подтверждение завершения всех фаз исходной задачи. Полная история первого этапа:
[INPLACE_VALIDATION.md](INPLACE_VALIDATION.md). Незакрытые задачи:
[INPLACE_REMAINING_TASKS.md](INPLACE_REMAINING_TASKS.md).

## 1. Архитектура и исходная проверка

Kizurium — Python, OCR preprocessing/normalization, grouping, измерение изображения
в `typography/metrics.py`, Pango layout, GTK/Cairo overlay и отдельные кэши измерений
и карточек. LipaX использует Rust detector/tracker/classifier/matcher/typography,
Qt QTextLayout для shaping/метрик и QML для уже рассчитанного представления.
Принципы пустого входа, кэширования и проверки метрик адаптированы к API LipaX.
Перенос Pango/GTK и копирование AGPL-кода в MIT-проект отвергнуты. Реализация и
тесты reference-проекта не изменялись. Новых runtime-зависимостей и шрифтов нет.
В продолжении выполнены reference `test_measure_cache`/`test_type_fixtures`:
30 passed, 0 failed, с `PYTHONPATH=src` из директории Kizurium.

Baseline Rust: 373 passed, 0 failed, 15 ignored. Первоначальный запуск в sandbox
получил ошибки запрета loopback в существующих HTTP mocks; разрешённый запуск
на хосте прошёл. До исправления новые регрессии воспроизвели panic пустого блока,
неверное число/выравнивание OCR-строк и подмену detector geometry невалидным OCR.
Отдельный тест воспроизвёл вытеснение перевода повторным фрагментом OCR.
Логи воспроизведения: `tmp/phase23/reproduced-regressions.log`,
`tmp/phase23/duplicate-before.log`.

## 2. Изменённые и добавленные файлы

- `crates/core/src/layout/block_detector.rs`: безопасная line height при пустом/
  невалидном входе; sweep по runs предыдущей строки с сохранением 8-connectivity;
  повторное использование буферов вместо лишних проходов по компонентам.
- `font_classifier.rs`: безопасный пустой блок; foreground coordinates читаются
  один раз перед проверкой 25 углов наклона; histogram buffer переиспользуется.
  Формулы, округление и калиброванные пороги классификации сохранены.
- `typography.rs`, `engine.rs`: нормализация OCR rectangles до группировки,
  IoU/refinement и оценки типографики; finite/positive extents, стабильный порядок,
  удаление точных дублей; при отсутствии валидных строк сохраняется detector.
- `place.rs`: подтверждённый вложенный дубль OCR не становится препятствием для
  своего владельца. Требуются совпадающая нормализованная подстрока минимум из
  восьми символов, >=90% площади внутри владельца и более длинное чтение либо
  одинаковое чтение с меньшим стабильным id. Только внутри одной capture region.
  Пересечение геометрии само по себе не разрешает закрывать независимый HUD.
- `mod.rs`: документация периодического подтверждения смены оформления.
- `crates/app/src/text_measure.h`, `app_icon.h`: единый конструктор QFont с теми же
  весом, italic, размером, kerning, ContextFontMerging и absolute letter spacing.
- `tests/inplace/rust/regression.rs`: 14 отдельных регрессий, включая проверки
  дубля/независимой подписи и повторного текста в другой позиции.
- `tests/inplace/{qt/measure_server.cpp,rust/qt_measure.rs}`: тестовый JSON-lines
  адаптер к общему production Qt helper; IPC в приложение не добавлялся.
- `tests/inplace/rust/bench.rs`: реальный RapidOCR полного кадра и OCR crops,
  application confidence/meaningfulness gates, OCR line refinement, shared Qt
  fitting, реальные client/screen coordinates и причины dropped blocks.
- `tests/inplace/python/{test_inplace_screenshots.py,benchmark.py,live_capture.py}`:
  screenshot/Qt integration, paired benchmark и разрешённый захват двух процессов;
  отсутствующие переводы и fallback учитываются отдельно от clipping checks.
- `tests/inplace/{run.sh,README.md}`: воспроизводимый запуск и зависимости.
- `docs/screenshots/test-game/translations/`: по запросу пользователя постоянные
  PNG, UTF-8 тексты перевода и JSON результатов. Это заданные тестовые переводы,
  без внешнего переводчика; они не являются утверждёнными эталонами.
- Версия 1.3.9 согласована в Cargo manifest/lock, PKGBUILD и README.

Удалённых файлов приложения нет. Предшествующие локальные изменения сохранены.
Тестовые исходники находятся отдельно в `tests/inplace/`, промежуточные данные,
логи, bytecode, caches, сборки и кадры захвата — в игнорируемой `tmp/`.

## 3. Глифы и типографика

Существующий pixel classifier оценивает признаки штрихов, пропорции, slant,
serif/monospace, cap/x-height и contrast. Это оценка внешнего вида, не доказательство
точного имени шрифта. Оптимизированный slant сохраняет результаты на 16 кадрах
при paired CPU сравнении; отдельные тесты проверяют upright/italic на трёх высотах.
`font_report` отдельно выполнен на 193 существующих размеченных font fixtures:
category group 88%, exact weight 83%, weight within one 99%, italic 100%,
replacement category 94%. CJK group: 0/2; это ограничение pixel classifier,
не результат OCR языка. Эти цифры нельзя переносить на игровые кадры.

Shaping, Unicode advances, kerning и line wrapping рассчитывает Qt QTextLayout,
тот же QFont применяется при Qt-измерении тестов и в приложении. Latin/Cyrillic/
Japanese/Chinese/Korean bundled coverage и mixed-script Qt-рендер проверены.
Выбор пользовательского семейства остаётся только из bundled collection.
Неподдерживаемые письменности могут требовать установленных системных fallback;
полной переносимой гарантии для всех Unicode scripts нет. CJK serif bundled нет.

## 4. Распознавание и визуальное наложение

По прямому указанию OCR-прогоны используют **RapidOCR**:
нативный LipaX backend, CPU, два потока, без GPU. Существующие PP-OCRv5 models
прочитаны из пользовательского model cache; скачивания/изменения этого cache нет.
ORT: `/usr/lib/libonnxruntime.so.1.29.0`. Модельные пути задаются через
`LIPAX_RAPID_TEST_MODELS`, runtime — `LIPAX_RAPID_TEST_ORT`.

Проверяются typed frame→desktop transforms, границы capture/client/screen,
реальные Qt contentWidth/contentHeight/truncation и неизменность пикселей вне
overlay plates. Rust placement cache используется повторно на одинаковом кадре.
Длинные переводы, длинное слово, hard breaks и mixed shaping проходят общий Qt
helper и реальный QML. Предопределённый перевод отделяет проверку размещения от
сетевого сервиса. Полный screenshot OCR не подменяется заранее заданными boxes.

Авторизованные процессы `engine.exe` (PID 100656) и `CrossxMars.exe` (PID 129055)
сняты через зарегистрированный Spectacle/KWin backend. Client geometries:
[421,43,1718,985] и [410,65,1739,978]; один монитор [0,0,2560,1080], observed scale 1.
Transient KWin script выгружен, исходные minimized/focus состояния восстановлены.
Это реальные игровые кадры, но **не** проверка capture transport работающего
LipaX, fullscreen, XWayland или дробного DPI. Офлайн Qt IPC не заменяет in-process
CXX-Qt integration приложения.

Качество оценивается совместно: корректный OCR при потерянном/смещённом overlay
не считается успешным визуальным результатом. `overlay_quality` сохраняет список
распознанных, но не показанных полей и degraded placements. Проверки clipping
могут пройти даже при таком дефекте; это прямо отражается в отчёте и backlog.
Финальная диагностика **FAIL** для обоих live-frame результатов:
engine.exe — 5 accepted OCR blocks, 3 rendered, uncovered blocks [6,9,10];
Cross x Mars — 13 accepted, 1 rendered, uncovered blocks [2,4,9,11,12,14,15].
Для списка учитываются чтения минимум из трёх слов, покрытие plate менее 90% и
исключаются подтверждённые дубль-фрагменты. Это диагностический порог, не
ground-truth accuracy metric. Рендер engine.exe визуально проверен: перевод
диалога вынесен над исходником (`beside-original`), исходные строки не закрыты.
`dialogue-en-13--recognized-blocks` этой диагностикой не отмечен как ошибочный,
но статус «нет выявленной ошибки» не заменяет независимого визуального эталона.

`CrossxMars.exe` (PID 129055) выявил взаимное блокирование диалога пересекающимися candidate boxes.
Подтверждённые точные повторные фрагменты теперь исключаются, но искажённые OCR
фрагменты и смешанные scenery/text components требуют отдельной сегментации.
Не утверждается, что ограниченное подавление дублей решило весь этот случай.

## 5. Автоматические проверки

- Rust, полный финальный прогон: **380 passed, 0 failed, 15 ignored**;
  `tmp/phase23/rust-final.log`. Отдельные новые regressions: **14 passed**.
- Опциональные Rust: `font_report` **1 passed**; Rapid native multilingual
  `reads_english_russian_and_japanese` **1 passed**, 2 filtered out. Из 15 default
  ignored в этом продолжении отдельно запускались эти две проверки; остальные
  13 — **NOT RUN**. На известных native fixtures English CER 0,
  Russian CER 0.054–0.081, Japanese CER 0.048. Это конкретные fixtures, не игры.
- QML/JavaScript: новый набор **13 passed**, существующий **106 passed**,
  0 failed; `tmp/phase23/final-qml.log`, baseline existing-QML log.
- C++/Qt helper: executable завершился с exit 0; отдельные cases не имеют
  самостоятельного pass-count. Shared Qt helper и приложенческий build проверены.
- Python/RapidOCR, полный финальный прогон: **25 passed, 0 failed**, 200.70 s;
  `tmp/rapid-live/final-python-corrected.log`. Новый отрицательный тест покрытия:
  **1 passed** отдельно, 25 deselected; всего 26 различных cases выполнены.
  После добавления coverage diagnostics три реальные OCR/Qt cases повторены:
  **3 passed**, 23 deselected, 28.41 s; повторные cases не прибавляются к общему числу.
  Две Pillow deprecation warnings относятся к старому comparator getdata API.
  Первый финальный запуск был остановлен из-за ошибочного пути к Qt server;
  путь исправлен, полный повторный прогон прошёл. Это не скрытый successful run.
- Kizurium selected reference tests: **30 passed**, reference sources untouched.
- Human-approved image regression: **NOT RUN**, эталоны не утверждены.
  Screenshot tests проверяют layout/pixels invariants и отрицательный comparator,
  но не доказывают OCR accuracy или правильность всех candidate regions.
- Live LipaX overlay + translation service E2E и отзывчивость compositor: **NOT RUN**.

## 6. Измерения производительности

Оборудование: AMD Ryzen 5 5600, 6 cores/12 threads; Linux 7.2.9 CachyOS,
Rust 1.99.0, Qt 6.12, Python 3.14.7. Paired release comparison:
3 раунда ×16 одинаковых реальных изображений, по 5 внутренних measurement samples,
порядок baseline/candidate чередуется. CPU governor/boost не закреплены.
Raw samples и SHA256 binaries: `tmp/phase23/paired-benchmark/`.

| Median | Baseline, ms | Detector/slant changes, ms |
|---|---:|---:|
| Detection + mask | 6.3174 | 5.5750 |
| Glyph analysis | 20.4478 | 17.0770 |
| Font matching | 0.071371 | 0.066251 |
| First frame | 48.5250 | 48.2076 |
| Repeat frame | 9.9897 | 8.9705 |
| ApproxMeasure placement | 0.41378 | 0.40353 |
| Cached placement | 0.063861 | 0.067904 |

Detector/classifier и placement metadata точно совпали в этих paired runs.
Это измерение сохранённых бинарников до позднего suppression дублей; таблица
не приписывает последнему изменению ускорение placement. Matching алгоритм здесь
не оптимизирован, небольшое отличие времени не означает улучшение алгоритма.
Median Rust VmHWM: 36.0352→36.1680 MiB; maximum 42.7813→42.4063 MiB.
Уменьшение памяти всего приложения не доказано.

На предыдущем полном RapidOCR прогоне 16 скриншотов cold per-process median OCR
около 3013 ms, включая загрузку моделей, CPU 2 threads. Это не steady-state latency
работающего приложения. OCR не сравнивается с прежним Tesseract как ускорение кода.
Qt helper microbenchmark: boundingRect 0.0167632 ms, shaped QTextLayout 0.0180151 ms
на 10000 calls; это стоимость разных методов, не equivalent before/after.
Test IPC placement/update timings включают transport/offscreen paint; их нельзя
выдавать за in-process Qt или compositor latency.

## 7. Ограничения и следующий этап

Реальное визуальное качество engine.exe и Cross x Mars ещё требует исправления
смещённого fallback и устранения перекрывающихся
ошибочных detector candidates и корректного объединения OCR fragments. Нужны
размеченные текстовые области/базовые линии и независимо утверждённые рендеры.
Лимит 16 candidates, Otsu scenery noise, partial glyphs, language fallback,
калибровка уверенности, live cancellation/flicker, multi-monitor/fractional DPI и
полный app-level профиль остаются открытыми. Внешний translator в тестах не вызван.
Подробные незавершённые пункты сохранены в отдельном Markdown backlog.


## 8. Следующее продолжение: OCR-строки, соседние рамки и стабильность

Исправлены три воспроизведённых механизма:

1. OCR words/fragments больше не увеличивают число строк автоматически:
   `reconstructed_ocr_lines` в `typography.rs` собирает близкие fragments при
   сопоставимой высоте (min >=0.6×max), вертикальном пересечении >=0.6×min height
   и горизонтальном промежутке <=2×max height. Далёкие labels остаются раздельными.
   Это ограниченная реконструкция горизонтальных строк, не универсальный
   анализ колонок или повёрнутого текста.
2. `bounded_refinement` в `engine.rs` ограничивает уточнение OCR границы серединой
   исходного свободного промежутка до отдельно детектированного соседа. Рамки,
   которые были раздельны до OCR, не начинают перекрываться из-за OCR padding.
   Пересекающиеся detector regions остаются неоднозначными и этим правилом
   не объединяются и не исключаются.
3. Cached OCR bounds проверяются повторно без добавления ещё одного ink margin.
   Новый регрессионный сценарий воспроизводил сдвиг/изменение размера на 1 px
   следующего идентичного кадра; после изменения geometry сохраняется.

В `place.rs` минимальная длина подтверждённого nested duplicate снижена с восьми
до трёх Unicode symbols: реальные SAVE/LOAD readings дают суффикс `OAD`.
Требования точной нормализованной подстроки, >=90% площади внутри владельца,
одной capture region и стабильного tie-break сохранены. Односимвольные readings
не считаются подтверждёнными дублями. Дополнительные тесты сначала падали:
`tmp/rapid-live/{geometry-before,cache-geometry-before,short-duplicate-before}.log`.

Новые тесты находятся в `tests/inplace/rust/regression.rs` (теперь 16 cases).
Screenshot harness сохраняет `frame_blocks`: фактические уточнённые source
rectangles, число строк и объяснение источника geometry. Live-frame тест engine.exe
требует обе реально прочитанные части конкретного записанного диалога, покрытие
их OCR rectangles >=90% и отсутствие `beside-original`. Это проверяет наличие
перевода в нужной области, а не лишь произвольной плашки в кадре.

Фактическая визуальная проверка нового PNG engine.exe показала перевод поверх
исходных строк. OCR-derived plate coverage двух частей — 0.99457 и 0.99197.
Теперь 5 accepted OCR readings →4 rendered fields +1 подтверждённый дубль,
`uncovered_text_blocks=[]`. Это успех данного placement diagnostic; ложная
детекция элемента сцены «0», качество цвета/spacing и точность OCR требуют
отдельной разметки. Сервисная точность перевода не проверяется: input предопределён.

Cross x Mars по-прежнему **FAIL**: 13 accepted →2 rendered, uncovered ids
[2,4,9,10,11,12,14,15]. Дефект диалога не закрыт. Причина отличается от engine.exe:
пересекаются сами detector regions, некоторые включают фон и несколько строк,
а частичные OCR readings имеют искажённые слова. Решение требует корректного
владения OCR-строками/регионами; простое разрешение рисовать поверх любого соседа
закроет независимый HUD и потому не применялось.

Выполнено в продолжении:

- Полный Rust после правок: 382 passed, 0 failed, 15 ignored; финальный лог
  `tmp/rapid-live/last-geometry-rust.log`. Новые regressions: 16 passed.
- Python/RapidOCR: полный набор 26 passed, 0 failed, 200.29 s;
  `tmp/rapid-live/rows-final-python.log`. После последних поправок к cached bounds
  и коротким дублям повторены три затронутых OCR/Qt cases: 3 passed,
  23 deselected, 37.24 s; `tmp/rapid-live/last-geometry-python.log`.
- QML/JavaScript повторно: 13 новых и 106 существующих passed, 0 failed,
  логи `tmp/rapid-live/rows-{qml,existing-qml}.log`.
- Финальная сборка приложения записана в `tmp/rapid-live/last-geometry-app.log`.
- Рендеры и тексты перезаписаны в `docs/screenshots/test-game/translations/`;
  утверждённые визуальные эталоны по-прежнему **NOT RUN**.

Таблица paired performance выше относится к сохранённым detector/slant binaries
предыдущего этапа. Эти дополнительные geometry fixes не получили отдельного
сопоставимого performance benchmark; им не приписывается ускорение приложения.
