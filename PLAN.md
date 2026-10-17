# PLAN.md — улучшение LipaX-QT на основе анализа Kizurium

Дата анализа: 2026-10-13. Версия LipaX-QT: `9aabff2` (1.3.9-2). Версия Kizurium: `project/kizurium` (pyproject 1.1.0).
Статус документа: **только анализ и планирование**. Код приложения и Kizurium не изменялись.

## Статус реализации (обновлено 2026-10-17)

Всё ниже сделано в рабочем дереве, **не закоммичено**.

**Режим «наложение текста» и смена диалога (проверено на живой CrossxMars.exe, Wine + KWin Wayland, и на записанных кадрах):**
- [x] Старый перевод снимается, как только под полем меняется текст (сигнатура расположения чернил), и вытесняется любым новым найденным блоком на том же месте; в живой игре первое обновление наложения — через ≈0,26 с после нажатия Space вместо 10–19 с (два перехода из трёх; в третьем — 2,1 с).
- [x] Два чтения одной строки, перекрывающиеся и взаимно отбрасываемые («нет места»), теперь склеиваются (`needs_join`: общая строка, точное сравнение слов) или остаётся более полное чтение (`reconcile_ocr`).
- [x] Стоимость фона «Подмена текста» на крупных полях снижена (шаг поиска 4 до 150 000 px, дальше диффузия): на сохранённых кадрах 2,7 с → ≈1,0 с, весь набор скриншотов в явном режиме 214 с → 110 с.
- [x] Ложное отклонение перевода (`guard`: потеря хвоста) исправлено; опция `click_through` и средняя кнопка (скрытие) как в P-02; метрика «призрака» только для явного режима (P-03).
- [x] Новые тесты: `tests/inplace/rust/dialogue_change.rs` (3 теста на живых кадрах), `ocr_ownership.rs` (общая строка), рекордер `crates/core/examples/record_dialogues.rs`, контрольные скриншоты `docs/screenshots/test-game/translations/crossxmars-live-*.png`.
- Проверки: `cargo test --workspace` (код выхода 0 при пропуске двух известных тестов), QML 128 + 18, скриншот-тесты `test_inplace_screenshots.py` с RapidOCR: 19 пройдено, 2 падают (`test_recorded_game_capture_pipeline[engine.exe|CrossxMars.exe]`: повторный OCR на неизменном кадре, **те же 1 и 10 заданий на чистом HEAD**).

**Ранее сделано:** P-04 (шаг 1), P-05, P-07, P-10 (O(1)), P-11, P-14 (часть), CI-зависимости и лог платформы Qt (см. историю ниже).

**Известные ограничения:** живая проверка на одной машине и одной игре; игра печатает реплику постепенно, перевод неполного чтения возможен; имя говорящего режется верхней границей области профиля; после перезапуска игры нужно заново выбрать окно (в настройках остаётся старый uuid); процесс держит заметную долю CPU; два теста записанных игр красные и до этих правок.

**Не сделано:** P-01, P-06, P-08 (эталоны), P-09, P-12, P-13.

## 0. Как читать документ

- **[VERIFIED]** — утверждение проверено чтением исходников (указаны `файл:строка`).
- **[ASSUMPTION]** — вывод по косвенным признакам; перед реализацией нужен шаг «Проверить».
- **[NOT RUN]** — поведение не запускалось в рамках этого анализа (живой KWin, мониторы, дробный scale).
- Бенчмарки в этом документе **не проводились**; все числовые цели — это предлагаемые критерии приёмки, а не измеренные значения. Числа, процитированные из Kizurium (например, «957 → 432 мс»), взяты из комментариев в его коде и мной не воспроизводились.
- Анализ выполнен тремя параллельными разведками по коду; ключевые утверждения (помечены ниже) я перепроверил сам: `app_icon.h`, `InplaceTranslation.qml`, `background.rs::resolve_mode`, `place.rs::settings_fingerprint`, `bridge.rs::save_backdrop`, `pipeline.rs` (`unless_stale`, `translate_shared`), `translate/bergamot.rs`, `cache.rs`, `checks.yml`/`PKGBUILD`.

### 0.1 Политика временных файлов (обязательна для всех шагов плана)

Все временные файлы, кэши, логи, скриншоты, артефакты сборки и тестов — **только в `/home/art/DEV/LipaX-QT/tmp/`** (каталог в `.gitignore`).

Единый набор переменных (расширить существующее в `tests/inplace/run.sh:4-7`, вынести в `tmp/env.sh`, который уже упоминается в рабочих заметках, но сейчас в `tmp/` отсутствует):

```sh
export LIPAX_TMP="$PWD/tmp"
export TMPDIR="$LIPAX_TMP" TMP="$LIPAX_TMP" TEMP="$LIPAX_TMP"
export XDG_CACHE_HOME="$LIPAX_TMP/xdg-cache" XDG_DATA_HOME="$LIPAX_TMP/xdg-data"
export XDG_CONFIG_HOME="$LIPAX_TMP/xdg-config" XDG_STATE_HOME="$LIPAX_TMP/xdg-state"
export XDG_RUNTIME_DIR="$LIPAX_TMP/xdg-runtime"      # chmod 0700; см. примечание 2
export CARGO_TARGET_DIR="$LIPAX_TMP/cargo-target" CARGO_HOME="$LIPAX_TMP/cargo-home"
export RUSTUP_TOOLCHAIN=stable
export PYTHONPYCACHEPREFIX="$LIPAX_TMP/python-bytecode" PYTHONDONTWRITEBYTECODE=1
export QML_DISABLE_DISK_CACHE=1 QT_QPA_PLATFORM=offscreen   # для headless-тестов
```

Примечания и ограничения (честно):

1. `CARGO_HOME` внутри `tmp/` означает полный повторный скачивающий `cargo fetch` (сеть). Если нужен офлайн — оставить `CARGO_HOME` системным **только для чтения реестра**, а `CARGO_TARGET_DIR` всё равно в `tmp/`. Это единственное известное мне исключение, и его надо подтвердить у владельца проекта до первого запуска.
2. **[VERIFIED]** Приложение само пишет вне `tmp/`: `bridge.rs:1445-1462` (`backdrop_path`, `preview_dir`) использует `dirs::runtime_dir()` → `$XDG_RUNTIME_DIR/lipa/*.png`; `portal.rs` пишет скрипты/сокеты в `$XDG_RUNTIME_DIR`; `geometry.rs` — `lipa-geometry-*.js`. Поэтому при запуске приложения и live-тестов `XDG_RUNTIME_DIR` **обязательно** переопределяется на `tmp/xdg-runtime`. Но для живой Wayland-сессии нужен *настоящий* `XDG_RUNTIME_DIR` (там сокет `wayland-0`, D-Bus). Решение: `WAYLAND_DISPLAY=/run/user/1000/wayland-0` (абсолютный путь допустим) и `DBUS_SESSION_BUS_ADDRESS` — оставить, а каталог для файлов приложения направить через новую переменную `LIPAX_RUNTIME_DIR` (см. задачу T-07). До появления этой переменной live-тесты пишут в `/run/user/1000/lipa` — **это нарушение политики, которое нужно либо устранить задачей T-07, либо явно принять**.
3. Нижележащие инструменты, не подчиняющиеся `tmp/` без обходов: `qmltestrunner` (QML-кэш — отключается `QML_DISABLE_DISK_CACHE=1`), `kwin_wayland --virtual` (создаёт сокет в `XDG_RUNTIME_DIR` — нужен отдельный runtime-каталог для вложенного KWin), `spectacle` (пишет в `~/Pictures` по умолчанию — вызывать только с `-o tmp/...`), `cargo` (`CARGO_HOME`, см. п.1), `ort` с `load-dynamic` (читает системную `libonnxruntime`, не пишет). Всё, что не удастся перенаправить, — объяснить пользователю до запуска.
4. Служебные каталоги самого harness Claude Code (`/tmp/claude-1000/...`) создаются инструментом, не проектом, и к проектным операциям не относятся.
5. Очистка: `tmp/` можно чистить целиком (`rm -rf tmp/*`), но не удалять `tmp/` как каталог и не трогать `Old/`, `target/`, `dist/`, `pkg/`, `graphify-out/`.

---

## 1. Текущее состояние LipaX-QT

### 1.1 Архитектура [VERIFIED]

Cargo workspace: `crates/core` (`lipa-core`, библиотека без Qt) и `crates/app` (Rust + cxx-qt + QML, C++ helper'ы в `app_icon.h`, `text_measure.h`, `model_worker.cpp`).

| Слой | Модули | Роль |
|---|---|---|
| Захват | `core/src/capture/{mod,kwin,portal,geometry,title_bar}.rs`, `geometry.js` | `AnyCapture`: KWin ScreenShot2 по D-Bus (окно, `native-resolution`, без декораций) или xdg-desktop-portal ScreenCast + `gst-launch-1.0` (PNG-поток). Геометрия окон — встроенный KWin-скрипт `geometry.js` → D-Bus `io.lipa.Geometry.Update` |
| Конвейер | `core/src/pipeline.rs` (2298 строк) | tokio-цикл с тиком 100 мс; на область — `RegionState` с конечным автоматом `Phase`, бюджет ошибок 60 с, экспоненциальный backoff ≤30 с, `unless_stale` (отбрасывание OCR, если кадр изменился), `translate_shared` (дедупликация запросов, `Shared` future), `generation` для отсева устаревших событий |
| Детектор изменений | `core/src/detect.rs` | битовая карта граней 4×4 px, игнор курсорных пятен, `MINOR_HOLD` |
| OCR | `core/src/ocr/{mod,rapid,meiki,paddle,filter}.rs`, `tesseract.rs` | RapidOCR и Meiki — in-process через `ort` (`load-dynamic`), `with_intra_op_spinning(false)` (`rapid.rs:119`); Tesseract — процесс на вызов; PaddleOCR — долгоживущий Python-воркер (`paddle.rs:94`) |
| In-place движок | `core/src/layout/{engine,block_detector,local_ink,tracker,place,fit,collision,background,typography,font_*,units}.rs` | кадр → маска чернил (Otsu + local contrast) → блоки → трекер (IoU ≥0.3, TTL 1.5 с) → OCR по полям → выбор шрифта (один раз, из бандла) → перевод → фон → `place_regions` (Viewport, `Qt`-метрики через C++, `collision::resolve`, лестница деградации `plain-plate → beside-original → inside-original`) |
| Перевод | `core/src/translate/{mod,bergamot,models}.rs`, `cache.rs` | Google/Yandex/DeepL/Microsoft/custom (reqwest, таймаут 10 с, cooldown по 429) + локальный Bergamot (**внешний процесс**) |
| UI | `app/qml/*.qml`, `app/src/bridge.rs` (1901 строка) | In-place поля — отдельные `Window` + `org.kde.layershell` (LayerShellQt), Overlay-слой; `FloatingTranslationSurface` — обычный `xdg_toplevel` + KWin-скрипт; `PinnedTranslationSurface` — layer-shell с маской ввода |
| Тесты | `tests/inplace/{run.sh,rust,python,qml,qt}`, `crates/app/tests/*.qml`, `tests/overlay/` | offscreen/`QT_QUICK_BACKEND=software`; PySide6 `grabWindow()`; реплей записанного RapidOCR; живые тесты — вручную |

Положительное, что **не нужно переписывать** (оно уже на уровне или лучше Kizurium): типизированные единицы `FramePx/CropPx/DesktopPx` (`units.rs`), `Viewport`/`safe_rect`, кэш размещения `PlacementCache`, трекер полей с TTL, детектор курсорных пятен, `ort` без spin-wait, `set_hook` для panic (`logging.rs:66`), `catch_unwind` вокруг Qt-воркера (`model_worker.rs:20,105`), state machine фаз региона, `generation`-штамп событий.

### 1.2 Выявленные слабости (с источником)

| ID | Слабость | Статус | Источник |
|---|---|---|---|
| W1 | Окна in-place **не имеют маски ввода и не `TransparentForInput`**: `configureTranslationWindowInput` обрабатывает только `objectName == "translationWindow"`, а у поля `objectName == "inplaceTranslation"`; `MouseArea` принимает только среднюю кнопку | [VERIFIED] код; последствие «клики не доходят до игры» — [ASSUMPTION], [NOT RUN] | `app_icon.h:97-109`, `InplaceTranslation.qml:14,81-86` |
| W2 | Режим `Auto` **никогда не выбирает `Reconstruct`** («замена текста без плашки»); он выбирает SolidFill / InpaintBlur / AdaptivePaddingFill. «Без боксов» работает только в явном режиме `TextReplacement` | [VERIFIED] | `background.rs:143-156` |
| W3 | Bergamot запускается **отдельным процессом на каждый вызов перевода** (spawn + загрузка модели + EOF), `translate_changes` порождает N процессов на N абзацев | [VERIFIED] | `bergamot.rs:49-70`, `pipeline.rs:738-749` |
| W4 | Во время OCR каждые 150 мс выполняется **полный повторный захват** (D-Bus + чтение + конвертация + crop) только чтобы сравнить кадр | [VERIFIED] | `pipeline.rs:451,701-716` |
| W5 | Нет проверки качества перевода (пустой/равный исходнику, потеря хвоста, мусор), нет защиты плейсхолдеров/чисел | [ASSUMPTION] — по отсутствию в grep по `crates/core/src`; полноту проверить | — |
| W6 | Кэш перевода: только в памяти, LRU на `VecDeque::retain` (O(n) на каждый hit), пустой после рестарта; ключ собирается `format!` на каждый вызов | [VERIFIED] | `cache.rs:20-42` |
| W7 | Плавающее окно перевода — обычный toplevel: KWin может его snap/tile/maximize; защита — только KWin-скрипт, находящий окно по подписи заголовка | [VERIFIED] | `FloatingTranslationSurface.qml:5-7,36`, `geometry.js:128-208` |
| W8 | PNG через диск: подложки полей и превью пишутся в `$XDG_RUNTIME_DIR/lipa` (`img.save`), QML перечитывает по `?r=revision`; портал-путь: gst `pngenc` → pipe → PNG-decode на каждый grab | [VERIFIED] | `bridge.rs:1450-1461`, `portal.rs:252-274,409` |
| W9 | Мелкие аллокации/копии: `rgba_from_raw` — скалярное построчное копирование и отбрасывание alpha; `crop_imm` копирует даже для полнокадровой области; `settings_fingerprint` сериализует `InplaceSettings` в JSON на каждый вызов `place_regions` | [VERIFIED] | `kwin.rs:305-336`, `place.rs:253-257` |
| W10 | CI вероятно не собирает приложение: `PKGBUILD` зависит от `layer-shell-qt` и `kwindowsystem`, а в `pacman -Syu` в `checks.yml:20` их нет; `build.rs:75` жёстко задаёт `/usr/include/KF6/KWindowSystem` | [VERIFIED] расхождение списков; сборочное последствие — [ASSUMPTION] (могут прийти транзитивно) | `checks.yml:20`, `PKGBUILD:8`, `build.rs:75` |
| W11 | Живое «XWayland-доказательство» сомнительно: тест задаёт `QT_QPA_PLATFORM=xcb` (`live_overlay_pipeline.py:135`), но лог показывает `window_role=xdg_toplevel` (QML печатает `x11_toplevel` при `pluginName === "xcb"`) и предупреждение `qt.qpa.wayland` | [VERIFIED] расхождение в артефактах; причина — [ASSUMPTION] | `tests/overlay/evidence/xwayland/application.log:13-14` |
| W12 | Хеши в `tests/overlay/evidence/manifest.json` устарели относительно `app_icon.h` (`badab011…` ≠ `0a91886e…`) — свидетельство не относится к текущему коду | [VERIFIED] | `manifest.json` |
| W13 | Не проверено вживую (по собственным документам): несколько физических мониторов, дробный scale KWin, физическое перетаскивание плавающего окна, живой Portal | [NOT RUN] | `tests/overlay/README.md:33-37`, `docs/RELEASE-1.3.9.md:18-21` |
| W14 | Область, пересекающая два монитора, рамкой не режется (`RegionFrame` выбирает экран по центру); in-place движок режет по `Viewport` корректно | [VERIFIED] | `RegionFrame.qml:93-99`, `place.rs:186-195` |
| W15 | Документы расходятся с кодом: `INPLACE_VALIDATION.md §7` говорит о бюджете 16 блоков (в коде 64), `ARCHITECTURE.md:194` описывает маски для X11, а у in-place масок нет вовсе | [VERIFIED] | `block_detector.rs:108`, docs |
| W16 | OCR коротких токенов (HUD: `15` vs `is`, `0/O`, `1/l`) без голосования/проверки на путаемые пары | [ASSUMPTION] — grep по `confus|lookalike` в `crates/core/src` пуст | — |

---

## 2. Kizurium: что это и чем отличается

Kizurium — Python 3.11+ приложение (≈35 тыс. строк), GTK4 + `gtk4-layer-shell`, **только wlroots/Hyprland-семейство** (захват `grim`, выбор области `slurp` и Quickshell-QML). Это не Qt и не KDE. Поэтому переносится в основном **идеями**, а не кодом.

| Область | Kizurium [VERIFIED] | LipaX-QT | Вердикт |
|---|---|---|---|
| Захват | один бэкенд `grim -g` через subprocess, PNG в stdout (`capture.py:96-117`); KWin/portal/spectacle не используются; масштаб монитора не учитывается | KWin ScreenShot2 + Portal, native-resolution, client-area crop | LipaX **сильнее**; код Kizurium для KDE непригоден |
| Дешёвое обнаружение изменений | 320-px grayscale thumbnail, per-block dirty, маскирование собственных карточек на thumbnail, motion-vs-text классификатор (`live/change.py:51-81,366-453,502-542,557-763`) | битовая карта граней 4×4 (`detect.rs`), без per-block dirty | идея **per-block dirty и маски собственного оверлея** полезна; общий детектор LipaX уже хорош |
| Синхронизация захвата | скрыть оверлей → ждать paint → grim (`live/snap.py:46-67`); все карточки мигают | KWin ScreenShot2 снимает **окно игры** без оверлеев (слои оверлея не входят в буфер окна) | **не копировать** (LipaX не нуждается) |
| Очистка исходного текста | непрозрачная карточка цвета, измеренного на экране, alpha ≥ 0.985, без рамки/скругления, если цвет измерен (`render/overlay.py:663-697`; тест `test_card_covers_text.py`) | `Reconstruct` (патч с заполнением дыры + exemplar), `InpaintBlur`, плашки | идея **минимального порога alpha и теста «призрака»** — взять; сам метод карточки хуже Reconstruct |
| Выбор цвета фона | квантование 5 бит, самая частая корзина, сверка с кольцом 2 px по краю, медиана вместо среднего (`core/text.py:645-826`) | `BackgroundAnalyzer` (медианный цвет, texture, edge_density) | сопоставимо; перепроверить краевое кольцо (см. P-03) |
| Подгонка текста | порядок «растянуть → сжать начертание (≥0.62) → уменьшить кегль → перенос» (`render/cards.py:1236-1481`) | `fit.rs`: лестница, QML-точные метрики через `QFontMetricsF`/`QTextLayout`, `condensed` | у LipaX уже тот же порядок, метрики точнее |
| Коллизии | SAT для повёрнутых квадов (`overlay.py:223-330`) | `collision.rs` на осевых прямоугольниках (198 строк) | повороты в LipaX не поддерживаются — низкий приоритет |
| Окно оверлея | одно layer-shell окно размером с область, `exclusive_zone=-1`, пустой input region, переустановка на `realize/map/redraw` (`live/app.py:146-267`, `cards.py:1484-1508`) | отдельное layer-shell окно на каждое поле, `exclusionZone: -1`, **ввод не отключён** | **брать идею пустого input region и переустановки** |
| OCR | Rapid → Meiki (CJK) → Tesseract (только если пусто); `det.limit_side_len=640`, 2 потока ORT, глобальный `_OCR_LOCK`, апскейл мелких, «core-ink» повторное чтение слабых результатов, голосование коротких токенов (`ocr/*`, `live/reconcile.py:147-274,663`) | Rapid/Meiki/Paddle/Tesseract, фильтры `Preprocess`, `auto` с эскалацией на Paddle по полю | отдельные приёмы (голосование коротких токенов, порог повторного чтения) — применимы |
| Трекинг | композитный счёт соответствия, `content_revision`, grace 300-1000 мс, список разрешённых причин глобальной очистки (`live/tracking.py`, `live/scheduler.py`, `clear_policy.py`) | `TextBlockTracker` IoU≥0.3, TTL 1.5 с | LipaX проще; многокритериальный счёт — Low |
| Перевод | gtx → MyMemory → deep-translator; CT2/OPUS-MT **в процессе**; SQLite-кэш + translation memory; плейсхолдеры `ZZNAME{n}ZZ`; проверки `looks_like_a_translation`, `translation_dropped_tail`; числовые guard'ы; офлайн-режим без утечек (`translate.py`, `translation/*`) | Google/Yandex/DeepL/Microsoft/custom + Bergamot-процесс; кэш 512 в памяти | идеи проверок качества, кэша, постоянного локального движка — брать; онлайн-цепочку gtx — **нет** |
| Потоки | единый бюджет по физическим ядрам + cgroup (`threads.py:51`); OCR ORT 2 потока | `effective_threads`, ORT без spin | сопоставимо |
| Жизненный цикл | pidfile + открытый fd, остановка lock → SIGTERM → SIGKILL (`cli.py:365`), faulthandler, ротация логов, `--doctor` | `instance.rs` (290 строк), `logging.rs`, panic hook | сопоставимо; `--doctor` аналог есть в `diagnostics.rs` |
| Тесты | ≈130 pytest на синтетике PIL/cairo, фикстуры шрифтов + manifest, «рендер каждой карточки уникальным цветом» (`test_every_card_is_drawn.py`), `scripts/screencheck.py` (реальный compositor, **только Hyprland**, оценка по логу, не по пикселям), `benchcheck` («нет измерения» ≠ «пройдено»); **нет** вложенного композитора и CI | offscreen QML + PySide6 grab + replay OCR; живые тесты вручную; CI-job `structural` | LipaX **сильнее** структурно; берём: тест остаточного «призрака», unique-colour «нарисовано ли», правило benchcheck |

Что в Kizurium **плохо** и копировать нельзя: скрытие всего оверлея на каждый цикл OCR (мерцание); нет учёта scale монитора; `card_cache.py` фактически мёртв (`overlay.py:647`); проверка устаревшего кадра по `frame_queue.version` не срабатывает в однопоточном цикле (`session.py:581,664`); тесты, ищущие подстроки в исходниках; жёстко прошитые строки конкретных игр; полная перезапись SQLite-кэша при каждом flush.

---

## 3. Предложения по улучшению

Формат каждого пункта: **Проблема → Подход Kizurium → Решение для LipaX-QT → Файлы → Риски → Проверка**.
Приоритеты: **Critical / High / Medium / Low**. Идентификаторы `P-NN` используются в дорожной карте.

### P-01 [Critical] Достоверная проверка Wayland и XWayland (фундамент для остальных пунктов)

- **Проблема.** W11, W12, W13: единственное «живое доказательство» для XWayland, вероятно, не исполнялось на xcb, а манифест привязан к устаревшим хешам. Без этого нельзя утверждать «стабильно на KDE Wayland и XWayland» и нельзя безопасно менять оконный код (P-02, P-07).
- **Kizurium.** `scripts/screencheck.py` — настоящий compositor, но Hyprland-only и судит по логу; вложенного композитора нет. Прямого аналога нет — это зона, где LipaX должен пойти дальше сам.
- **Решение.**
  1. Найти причину расхождения в `tests/overlay/evidence/xwayland/application.log:13-14`: проверить, как `live_overlay_pipeline.py:135` передаёт окружение (унаследует ли `main.rs:14-29` / Qt `QT_QPA_PLATFORM=xcb`; нет ли `-platform` в аргументах, `QT_QPA_PLATFORM` переопределяется ли в `main.rs`). Добавить в приложение строку лога `platform=<QGuiApplication::platformName()>` при старте и заставить live-тест **падать**, если запрошенная и фактическая платформы различаются.
  2. Автоматизировать вложенный KWin: `kwin_wayland --virtual --socket <имя> --width 1920 --height 1080` (**[ASSUMPTION]**: `kwin_wayland 6.7.5` установлен — `/usr/bin/kwin_wayland`; запуск в `--virtual` режиме, поддержка layer-shell и ScreenShot2 внутри него нужно проверить). Если работает — live-тесты идут в изолированной сессии без участия пользовательского рабочего стола, с отдельным `XDG_RUNTIME_DIR` внутри `tmp/`.
  3. Пересчитывать хеши источников в `manifest.json` автоматически (скрипт сравнивает со `sha256sum` текущих файлов, красный тест при расхождении).
- **Файлы.** `tests/inplace/python/live_overlay_pipeline.py`, `tests/inplace/python/live_capture.py`, `tests/overlay/evidence/manifest.json`, новый `tests/inplace/live/run_nested_kwin.sh`, `crates/app/src/main.rs` (одна строка лога).
- **Риски.** Вложенный KWin может не поддерживать D-Bus `ScreenShot2` без авторизованного `.desktop` (в `kwin.rs:31-34` захват разрешён только бинарю из `Exec=`) — тогда для захвата в тесте использовать Portal или тестовую подмену `Capture`. Это длинный пункт; выполняется как исследование с чётким критерием остановки.
- **Проверка / приёмка.** (а) лог запуска содержит `platform=xcb` ↔ запрос xcb, `platform=wayland` ↔ запрос wayland; тест красный при несовпадении; (б) `manifest.json` содержит хеши, совпадающие с `sha256sum` файлов, иначе тест падает; (в) один скриптовый запуск `tests/inplace/live/run_nested_kwin.sh` проходит без вмешательства человека или документирует конкретное ограничение.

### P-02 [Critical*] In-place окна не должны перехватывать ввод игры

\* Critical **при условии** подтверждения шагом 0; если клики проходят — понизить до Medium и закрыть тестом.

- **Проблема.** W1. Окно поля — layer-shell Overlay размером с текст; `MouseArea` нужна только для средней кнопки (скрыть поле). Область окна, лежащая над игрой, при непустом input region получает указатель, и игра её не видит. [NOT RUN]
- **Kizurium.** Пустые input/opaque region у окна оверлея (`render/cards.py:1484-1508`), переустановка на `realize`, `map` и после каждой перерисовки (`live/app.py:260-267`), потому что GTK может сбросить регион. Это единственная часть, где подход Kizurium однозначно надёжнее.
- **Решение.**
  0. **Шаг 0 (проверка):** собрать приложение, открыть in-place, вручную проверить клик по тексту поля над кнопкой в игре; зафиксировать результат в `tests/overlay/README.md`.
  1. Добавить для окон поля `Qt.WindowTransparentForInput` (так уже сделано в `RegionFrame.qml:115` и `FrameOverlay.qml:41`) — для Wayland Qt применяет пустой input region.
  2. Скрытие поля по средней кнопке заменить на то, что не требует ввода на самом поле: существующий глобальный хоткей (`core/src/hotkeys.rs`) / кнопка в главном окне. Проверить, что именно сейчас вызывает `hideRequested` (`InplaceTranslation.qml:85`) и какой пользовательский сценарий теряется; если пользователям нужен именно MMB на поле — вариант Б: маска ввода из одного небольшого «ручного» прямоугольника (по аналогии с `configureTranslationWindowInput`), по умолчанию выключенная.
  3. Распространить `configureTranslationWindowInput` на `inplaceTranslation` либо добавить второй helper.
- **Файлы.** `crates/app/qml/InplaceTranslation.qml`, `crates/app/src/app_icon.h`, `crates/app/qml/main.qml` (откуда создаются поля), `crates/app/tests/tst_inplace.qml`, `docs/ARCHITECTURE.md:186-238`.
- **Риски.** Потеря MMB-скрытия; на xcb `TransparentForInput` реализуется через XShape — проверить, не обрезает ли он отрисовку (комментарий в `app_icon.h:100-101` предупреждает про «некоторые X11-бэкенды»); регрессия поведения «закрепить» у Pinned-окна не затрагивается.
- **Проверка / приёмка.** QML-тест: `win.flags & Qt.WindowTransparentForInput` для `inplaceTranslation`; живой тест из P-01: синтетический клик (`ydotool`/`wtype`-аналог в вложенном KWin или `QTest`) по пикселю под текстом поля доставляется нижележащему окну (нижнее окно-приёмник фиксирует событие). Ручная проверка на KDE Wayland + XWayland.

### P-03 [High] Режим `Auto` и «замена текста без плашки» + тест «призрачного» исходного текста

- **Проблема.** W2. По умолчанию (`Auto`) пользователь получает плашки, а не `Reconstruct`. Кроме того, у `Reconstruct` альфа-канал на шве равен 128 (`background.rs` — `reconstruct_patch`), что при неудачной маске может оставить остаток исходных глифов. [ASSUMPTION] — численный остаток не измерялся.
- **Kizurium.** Для замеренного фона карточка рисуется **непрозрачной (alpha ≥ 0.985)**; комментарий `overlay.py:675-682` объясняет: при alpha 0.9 от глифа на 170 уровней остаётся «призрак» ~17 уровней, при 0.985 — менее 3. Тест `test_card_covers_text.py` закрепляет порог. Цвет панели выбирается по голосованию с кольцом 2 px (`core/text.py:700-740`), чтобы жирный шрифт не принимали за панель.
- **Решение.**
  1. **Сначала измерить**: добавить Rust-тест-метрику «остаток исходных чернил»: на синтетических и записанных кадрах (`tests/inplace/fixtures`, `docs/screenshots/test-game`) отрисовать итоговую композицию (оригинал + патч Reconstruct) и посчитать долю пикселей маски исходного текста, где разница с окружающим фоном превышает порог (например, >12 уровней по яркости).
  2. По результатам: (а) увеличить alpha шва там, где остаток выше порога; (б) в `resolve_mode(Auto)` добавить ветку `Reconstruct` для блоков, у которых `analysis.reliable` и `texture` в «средней» зоне, где Kizurium-подход и SolidFill дают видимую геометрию (кандидат — граница `flat < 0.5`), с обязательным откатом на SolidFill/AdaptivePadding, если метрика остатка выше порога.
  3. Не менять поведение явных режимов `PaddedFill`/`TransparentOutline`.
  4. Переиспользовать идею краевого кольца при вычислении `BackgroundAnalysis` (проверить, что `analyze` уже исключает глифы; если нет — добавить кольцо 2 px как контрольную оценку).
- **Файлы.** `crates/core/src/layout/background.rs` (`resolve_mode`, `reconstruct_patch`), `tests/inplace/rust/placement_quality.rs`, `tests/inplace/python/test_inplace_screenshots.py`, `docs/INPLACE_VALIDATION.md`.
- **Риски.** Изменение поведения по умолчанию — визуальная регрессия на играх, которых нет в фикстурах; `Reconstruct` дороже (до 48-64 раундов диффузии, но только при пересчёте фона — `engine.rs` `bg_changed/moved/reshaped`). Решение: переключатель в настройках, A/B на всех фикстурах, откат одной строкой.
- **Проверка / приёмка.** Метрика «остаток» (определение в шаге 1) для всех фикстур ≤ порога, зафиксированного отдельным числом после первого прогона; метрика входит в `placement_quality` и красная при регрессии; в `Auto` на фикстурах без текстуры результат пиксельно не изменился (или изменение явно одобрено).

### P-04 [High] Bergamot: один долгоживущий процесс / пакетный запрос вместо процесса на вызов

- **Проблема.** W3. Задержка каждого перевода включает старт процесса и загрузку модели; `translate_changes` умножает это на число абзацев (`join_all`).
- **Kizurium.** CT2 держится в процессе, ограничение потоков `intra=min(4, cores//8)`, батчинг строк с маркерами `@@KZT000@@ …` (`translation/batch.py:77`, `parse_marked_translation:365`), не более 28 новых строк за цикл. Замечание: локальная пачка у него на практике не используется (вызов по одной строке) — копировать надо **идею**, а не реализацию.
- **Решение.** Два шага, второй — по результатам первого.
  1. *Пакет в один процесс (без новых зависимостей).* Собрать все непереведённые абзацы тика в один stdin, разделяя строками, и разбирать stdout по строкам. **[ASSUMPTION]**: CLI Bergamot построчный (перевод строки соответствует строке входа) — подтвердить на `crates/core/tests/bergamot_native.rs` (opt-in `LIPAX_BERGAMOT_TEST_BINARY/MODELS`). Если есть многострочные абзацы — экранировать (замена `\n` внутри абзаца на пробел до перевода и восстановление разбиения по исходным пропорциям, как `pack_translation_to_line_boxes` у Kizurium).
  2. *Постоянный процесс.* Если CLI не умеет работать потоково (текущий код ждёт EOF — `bergamot.rs:108-112` комментарий), смотреть на режим сервиса в используемой сборке bergamot-translator или на тонкий воркер по образцу `paddle.rs::Worker` (долгоживущий процесс + кадровый протокол). Любой вариант требует правок поставки бинаря (`packaging/`, `PKGBUILD`).
  3. Ограничение потоков: сейчас `--cpu-threads 2` (`bergamot.rs:55`); связать с общим бюджетом потоков (`ocr/rapid.rs::effective_threads`).
- **Файлы.** `crates/core/src/translate/bergamot.rs`, `crates/core/src/translate/mod.rs` (`Translate::translate` принимает один текст — потребуется `translate_many` с реализацией по умолчанию через цикл), `crates/core/src/pipeline.rs` (`translate_changes`, `translate_shared`), `crates/core/tests/bergamot_native.rs`.
- **Риски.** Изменение трейта `Translate` затрагивает тестовые моки (`pipeline.rs:1100+`); ошибка разбиения строк даёт сдвиг переводов между абзацами — нужны тесты на соответствие порядка; потерянный абзац не должен ломать остальные.
- **Проверка / приёмка.** (а) тест соответствия «N входных строк → N выходных» на пустых строках, многострочных абзацах, юникоде; (б) измерение латентности **до/после** на одной машине (`tests/inplace/rust/bench.rs` расширить стадией `translate_local`), цель: время перевода k абзацев не растёт линейно по k запусков процесса — критерий формулируется после замера базовой линии; (в) в логе фиксируется число запущенных процессов на тик.

### P-05 [High] Проверка качества перевода и защита токенов

- **Проблема.** W5 [ASSUMPTION]. Онлайн-сервисы и особенно Bergamot (как и OPUS-MT) могут обрезать хвост длинного текста, вернуть исходник, исказить `Lv. 12`, `12/24`, имена. Сейчас в `pipeline.rs` результат принимается как есть (повторные проверки — только `unsure` для уверенности OCR).
- **Kizurium.** `translation/service.py`/`translate.py`: отбраковка пустого результата, эха исходника, мусора по гласным, `translation_dropped_tail` (>60 символов и потеря точек/длина <0.42 исходника), `placeholders_survived`, ASCII-плейсхолдеры `ZZNAME{n}ZZ` (юникодные «【0】» OPUS-MT превращал в кириллический мусор — `test_placeholder_survival`), числовые guard'ы (`_numeric_guard:521`).
- **Решение.** Новый модуль `core/src/translate/guard.rs` (без зависимостей): (1) `is_acceptable(source, translated, src_lang, dst_lang) -> Result<(), Rejection>` — только структурные проверки (пусто; равно исходнику после нормализации при разных языках; длина/потеря терминаторов; доля неизменённых токенов); (2) защита числовых токенов и плейсхолдеров: подмена на ASCII-токены и обратная проверка; (3) при отказе — не кэшировать, использовать резервный сервис (уже есть для DeepL/Microsoft→Google в `translate_shared`) либо показывать исходник; (4) все пороги — константы с комментарием «эмпирические» и покрытие тестами. **Не копировать** списки слов и игровые строки Kizurium.
- **Файлы.** новый `crates/core/src/translate/guard.rs`; `translate/mod.rs`; `pipeline.rs` (`translate_shared` — после `translator.translate`, перед `cache.put`); `tests/` новые Rust-тесты.
- **Риски.** Ложные срабатывания на коротких словах/именах (поэтому порог длины), на языках без пробелов (CJK) — отдельная ветка; задержка из-за повторного запроса к резервному сервису.
- **Проверка / приёмка.** Набор таблично заданных кейсов (хорошие/плохие переводы, числа, CJK, пустые) — 100% прохождение; отказ не попадает в кэш (тест на `TranslationCache`); существующие тесты `pipeline.rs` остаются зелёными.

### P-06 [High] Плавающее окно перевода без нежелательного snap/tile/maximize

- **Проблема.** W7. Окно — обычный `xdg_toplevel` (`FloatingTranslationSurface.qml:36`), KWin сам делает edge-snapping/tiling при перетаскивании к краю; защиты в коде нет, комментарии прямо сообщают, что это «ответственность компоновщика» (`:5-7`, `geometry.js:128`).
- **Kizurium.** Все его окна — layer-shell, поэтому snap/tile не существует; готового решения для обычного toplevel нет.
- **Решение (два варианта, выбрать после проверки Б).**
  - **Вариант А (минимальный):** в `geometry.js` подписаться на `tileChanged`, `maximizedChanged`/`fullScreenChanged` окна LipaX и сразу сбрасывать (`window.tile = null`, `setMaximize(false, false)`), возвращая последнюю свободную геометрию из `FloatingMoved`. **[ASSUMPTION]**: имена свойств/сигналов KWin Scripting API 6.x — сверить с документацией API KWin 6.7 / с `qdbus`-интроспекцией до реализации.
  - **Вариант Б:** сделать плавающее окно layer-shell (как Pinned), перетаскивание реализовать через изменение `LayerShell.Window.margins` по `MouseArea` с зажимом по экрану (код зажима уже есть: `PinnedTranslationSurface.qml:74-93`). Гарантирует отсутствие snap/tile, но теряет системное перетаскивание/изменение размера (`startSystemMove`/`startSystemResize`), поведение в Alt-Tab/панели задач и на xcb (где layer-shell не действует).
- **Файлы.** `crates/core/src/capture/geometry.js`, `crates/core/src/capture/geometry.rs`, `crates/app/qml/FloatingTranslationSurface.qml`, `tests/inplace/python/live_floating_window.py`.
- **Риски.** Вариант А — борьба с компоновщиком (мерцание «щёлкнуло → вернули»); Б — регрессия UX и разное поведение на XWayland. XTest не принимается KWin (`manifest.json` — «unverified» для перетаскивания), поэтому автоматическая проверка перетаскивания ограничена.
- **Проверка / приёмка.** В вложенном KWin (P-01) программно (через D-Bus KWin `org.kde.KWin /KWin`/скрипт) поставить окно в `tile`/`maximize` и убедиться, что состояние сброшено ≤ N мс (N задать после замера); ручная проверка drag к краям на реальном KDE Wayland — записать в `tests/overlay/README.md`.

### P-07 [High] Изоляция каталогов приложения для тестов (`LIPAX_RUNTIME_DIR`) и политика `tmp/`

- **Проблема.** §0.1 п.2: `backdrop_path`, `preview_dir`, KWin-скрипт и сокеты пишутся в `$XDG_RUNTIME_DIR`, что нарушает политику «всё только в `tmp/`» при любых live-прогонах.
- **Kizurium.** Каталоги выбираются через XDG с переопределением `KIZURIUM_TRANSLATOR_*` (`paths.py:198-206`), состояние/логи — не в tmpfs, права 0700/0600.
- **Решение.** Единая функция `lipa_runtime_dir()` в `lipa-core` (читает `LIPAX_RUNTIME_DIR`, иначе `dirs::runtime_dir()/lipa`), использовать в `bridge.rs`, `portal.rs`, `geometry.rs`; права 0700 при создании; тесты и `tmp/env.sh` выставляют `LIPAX_RUNTIME_DIR=$PWD/tmp/runtime`.
- **Файлы.** `crates/core/src/lib.rs` (новая функция), `crates/app/src/bridge.rs:1445-1462`, `crates/core/src/capture/{portal,geometry}.rs`, `tests/inplace/run.sh`.
- **Риски.** Низкие; не менять поведение по умолчанию.
- **Проверка / приёмка.** Тест: при заданной `LIPAX_RUNTIME_DIR` ни один файл не создаётся вне неё (проверка `strace -f -e trace=openat` или сравнение списка файлов `XDG_RUNTIME_DIR` до/после — выбрать вариант без новых зависимостей: сравнение каталогов).

### P-08 [High] Автоматическое GUI-регрессионное тестирование: что добавить

- **Проблема.** Текущие автотесты offscreen и не видят поведения компоновщика (W11-W13); эталонных изображений нет (`LIPAX_VISUAL_REFERENCES` пуст, `test_inplace_screenshots.py:262`).
- **Kizurium.** Берём: (1) *unique-colour тест* — каждое поле рисуется своим цветом, и тест проверяет по пикселям, что оно нарисовано (`test_every_card_is_drawn.py`); (2) *проверка остатка/покрытия исходного текста* (`test_card_covers_text.py`) — перенесено в P-03; (3) фикстуры + manifest, где пороги читаются **из тестируемого кода**, а не копируются (`test_type_fixtures.py`) — у LipaX уже `crates/core/tests/fixtures/fonts/*.png`, проверить, что manifest читает пороги из кода; (4) `benchcheck`: «нет измерения» ≠ «пройдено» (`scripts/benchcheck.py:78-92`). Не берём: тесты по подстрокам исходников.
- **Решение.**
  1. Утвердить набор эталонных скриншотов: `tests/inplace/references/*.png` для 16 кадров `docs/screenshots/test-game`; сравнение по допуску (максимум ΔE и доля изменённых пикселей), рендер всегда `QT_QUICK_BACKEND=software` + фиксированный шрифт-набор из бандла (детерминизм).
  2. Тест «поле нарисовано» (unique-colour) для QML in-place: рендер в `QQuickView.grabWindow()` с подстановкой цвета.
  3. Matrix по `QT_SCALE_FACTOR` (1/1.25/1.5/2) для геометрии (`packaging/test-ui.sh` уже запускает QML-тесты по стилям и scale — расширить на screenshot-тест).
  4. Скрипт отчёта `tests/inplace/report.py`: явный список `NOT RUN` (как уже делает `run.sh`) + пересчёт хешей (P-01).
  5. CI: добавить `layer-shell-qt kwindowsystem` в `pacman` (W10), проверить `cargo build` там; артефакты оставлять в `tmp/`.
- **Файлы.** `tests/inplace/python/test_inplace_screenshots.py`, `tests/inplace/run.sh`, `packaging/test-ui.sh`, `.github/workflows/checks.yml`, новый `tests/inplace/references/`.
- **Риски.** Эталоны хрупки при смене Qt-версии (в проекте минимально 6.12 — `packaging/check-qt.sh`); решение — привязать каталог эталонов к версии Qt и шрифтов, хранить допуск, обновлять отдельной командой `--update-references`.
- **Проверка / приёмка.** `bash tests/inplace/run.sh` в чистом `tmp/` проходит дважды подряд с одинаковым результатом (детерминизм); CI-job `structural` собирает приложение (подтвердить на реальном прогоне — **[NOT RUN]** в этом анализе).

### P-09 [Medium] Не делать полный повторный захват каждые 150 мс во время OCR

- **Проблема.** W4. `unless_stale` вызывает `capture.grab` целиком.
- **Kizurium.** Сравнивает уменьшенную 320-px grayscale копию; сам захват при этом тоже дорог (`grim`), поэтому решение Kizurium — не уменьшение стоимости захвата, а редкий опрос и per-block dirty (`change.py:502-542`). Прямого прироста от копирования нет.
- **Решение.** (а) Адаптивный интервал: `STALE_PROBE` растёт (150 → 300 → 600 мс) по мере длительности OCR; (б) для KWin-пути вызывать `grab` с уже известным `rect`, но без `crop_imm` полной копии для области, совпадающей с кадром; (в) измерить стоимость `grab` на реальной машине **до** любых изменений (стадия `capture` уже в `Timings`) — если она мала, пункт закрыть как «не требуется».
- **Файлы.** `crates/core/src/pipeline.rs:451,701-716`, `crates/core/src/capture/kwin.rs:332-336`.
- **Риски.** Реже опрос → дольше показ устаревшего результата; сохранять `MAX_STALE`.
- **Проверка / приёмка.** Лог `pipeline.ocr` показывает число зондов за OCR; цель — не более N зондов на секунду OCR (после замера). Существующие тесты `pipeline.rs` про устаревший кадр (`RecordingOcr`, `LateOcr`) остаются зелёными.

### P-10 [Medium] Кэш перевода: O(1) LRU и необязательное сохранение между запусками

- **Проблема.** W6.
- **Kizurium.** LRU в памяти (по умолчанию 4000) + SQLite с WAL, flush раз в 20 с, отдельная translation memory. Недостаток: полная перезапись таблицы при flush (O(n)).
- **Решение.** (1) Заменить `VecDeque::retain` на LRU со счётчиком поколений (`HashMap<Key,(String,u64)>` + эвикция по минимуму только при переполнении) — без новой зависимости; ключ — `Box<str>`/хеш вместо `format!`. (2) Необязательно: файл кэша в `XDG_CACHE_HOME/LipaX/` в формате JSONL (append-only, компакция при старте), включаемый настройкой; **не** добавлять `rusqlite`/SQLite только ради этого. (3) Увеличить ёмкость по умолчанию с 512 после замера объёма памяти.
- **Файлы.** `crates/core/src/cache.rs`, `crates/core/src/pipeline.rs:484`, `settings.rs` (флаг).
- **Риски.** Утечка текста экрана на диск → права 0600, документировать, по умолчанию выключено; запись только для успешных переводов, прошедших P-05.
- **Проверка / приёмка.** Существующие тесты `cache.rs` + новый тест на 10^5 операций `get` (время не растёт с размером — сравнение отношения времён, не абсолютных); тест восстановления после рестарта и после повреждённого файла.

### P-11 [Medium] Голосование для коротких OCR-токенов (HUD)

- **Проблема.** W16 [ASSUMPTION]. Короткие метки (`15`/`is`, `0/O`) читаются непредсказуемо.
- **Kizurium.** `resolve_short_element` (`live/reconcile.py:663`): ≤14 символов, веса прочтений (Rapid 0.5+(conf−40)/100, Tesseract 0.6), бонус предыдущего кадра **только** если различия лишь на путаемых позициях (`CONFUSABLE_PAIRS:585`), множитель правдоподобия токена, победитель должен вести на ≥0.25. Тесты: `test_ocr_lookalikes`.
- **Решение.** В `engine.rs`, где поле перечитывается (есть `field_engine`, `field_ocr`): для коротких полей запускать второе прочтение другим движком/предобработкой и применять правило «предыдущий кадр лишь подтверждает». Таблица путаемых пар — маленькая константа, без замен по словарю.
- **Файлы.** `crates/core/src/layout/engine.rs`, `crates/core/src/pipeline.rs` (`field_crops`, `read_field`), новый модуль `ocr/vote.rs`.
- **Риски.** Удвоение стоимости OCR для коротких полей (только при низкой уверенности, как «re-read под 80» у Kizurium); поэтому порог по `confidence`.
- **Проверка / приёмка.** Табличный тест на сценарии «`is`→`15`» и «`15`→`is`» (переключение не залипает); регресс-тест реплея `tests/inplace/fixtures/*-rapidocr.json` без изменений на уже верных полях.

### P-12 [Medium] Мониторы и дробный масштаб: закрыть непроверенное

- **Проблема.** W13, W14.
- **Kizurium.** Ключ сохранённой области `DP-1@2560x1440` (`selector.py:248-256`), поля margin относительно выхода, а не раскладки (`live/app.py:168-188`, комментарий о реальной ошибке второго монитора). Учёта scale нет — не копировать.
- **Решение.** (1) Проверить хранение региона привязкой к `имя вывода + размер` (сейчас — относительные доли окна; убедиться на `settings.rs`, что смена разрешения не даёт устаревшую область). (2) Для `RegionFrame`/`FrameOverlay` рассмотреть разбиение по выходам, как делает `Viewport::bounds_for`. (3) Тесты в вложенном KWin (P-01) с двумя виртуальными выходами разных размеров и scale 1.5 (**[ASSUMPTION]** `--virtual` позволяет несколько выходов — проверить `kwin_wayland --help`).
- **Файлы.** `crates/app/qml/{RegionFrame,FrameOverlay}.qml`, `crates/core/src/layout/place.rs`, `crates/core/src/capture/geometry.rs`.
- **Риски.** Средние; большая доля работы — настройка стенда, а не кода.
- **Проверка / приёмка.** Автотест: поле на втором выходе с origin ≠ (0,0) и scale 1.5 лежит внутри своего выхода ±1 px; сценарий отключения монитора не оставляет окон на несуществующем выходе.

### P-13 [Medium] Убрать PNG-обмены через диск и gst/PNG на портале

- **Проблема.** W8.
- **Kizurium.** Прямого аналога нет (он рисует через cairo без промежуточных файлов, но захват делает PNG-пайпом). Идея «ничего не писать на диск, рисовать из памяти» применима.
- **Решение.** (1) Для подложек/превью — `QQuickImageProvider` через существующий C++ слой (`app_icon.h`), выдающий `QImage` из общей памяти Rust (`Arc<RgbaImage>` уже есть); ревизия остаётся в URL. (2) Портал: заменить `pngenc` на сырой RGBA (`fdsink` + кадровая обёртка/`appsink`) — **[ASSUMPTION]** нужно проверить доступность элементов и формат границ кадров у используемой связки gst; если нельзя без новых зависимостей, оставить как есть и записать в «ограничения».
- **Файлы.** `crates/app/src/bridge.rs:1450-1461`, `crates/app/src/app_icon.h`, `crates/core/src/capture/portal.rs:252-274,409`.
- **Риски.** Время жизни `QImage` между потоками, совместимость с `cxx-qt`; портал — не трогать, пока не измерена доля стоимости в задержке.
- **Проверка / приёмка.** После (1) в `$LIPAX_RUNTIME_DIR` не появляется `inplace-*.png` (тест на отсутствие файлов), `qmltestrunner` зелёный; время публикации `publish_inplace` до/после (лог `tracing`).

### P-14 [Low] Мелкие аллокации и копии

- **Проблема.** W9.
- **Решение.** `rgba_from_raw`: `chunks_exact(4)`-цикл с перестановкой каналов без построчного индексирования, предвыделение `Vec::with_capacity`; пропуск `crop_imm`, если область = весь кадр; `settings_fingerprint` — вычислять при изменении настроек и хранить в `Settings`/`PlacementCache`; `TranslationCache::key` без `format!` (см. P-10).
- **Файлы.** `crates/core/src/capture/kwin.rs:305-336`, `crates/core/src/layout/place.rs:253-257`.
- **Риски.** Минимальные.
- **Проверка.** Замер до/после через `inplace_bench` (`tests/inplace/rust/bench.rs`); без изменения поведения — все тесты зелёные. Если выигрыш не измеряется — не принимать изменение.

### P-15 [Low] Примеры, которые рассмотрены и **не** рекомендуются

| Приём Kizurium | Почему не берём |
|---|---|
| Скрытие оверлея перед захватом + ожидание paint | KWin `CaptureWindow` снимает буфер окна игры без оверлеев LipaX; Kizurium вынужден так делать, и получает мерцание |
| `grim`/`slurp`/wlr-screencopy | KWin не предоставляет wlr-screencopy; у LipaX KWin D-Bus и Portal лучше |
| Онлайн-цепочка gtx / MyMemory / deep-translator | у LipaX уже есть DeepL/Microsoft/Yandex/custom и cooldown 429; неофициальные эндпоинты — риск |
| SQLite-кэш с полной перезаписью | заменяется на append-only JSONL (P-10) |
| Карточка (прямоугольник) как основной способ скрытия текста | уступает `Reconstruct`; берём только порог непрозрачности и тест |
| Python-классификатор «движение vs текст» | у LipaX `detect.rs` уже подавляет курсорные пятна; идея connected-components для Rust — отдельная задача только если появится реальный кейс ложных срабатываний |
| SAT-коллизии для повёрнутых карточек | LipaX не рисует повёрнутый текст; Low, откладывается |
| Тесты по подстрокам исходников, жёсткие игровые строки | антипаттерны |

---

## 4. Таблица приоритетов

| ID | Название | Приоритет | Усилие | Зависит от |
|---|---|---|---|---|
| P-01 | Достоверная проверка Wayland/XWayland, вложенный KWin | Critical | L | — |
| P-02 | In-place окна не перехватывают ввод | Critical (после шага 0) | S | P-01 для автопроверки |
| P-03 | `Auto` → `Reconstruct`, метрика «призрака» | High | M | — |
| P-04 | Bergamot: пакет/постоянный процесс | High | M–L | — |
| P-05 | Проверки качества перевода и токены | High | M | — |
| P-06 | Плавающее окно без snap/tile | High | M | P-01 |
| P-07 | `LIPAX_RUNTIME_DIR` и политика `tmp/` | High | S | — |
| P-08 | GUI-регрессия: эталоны, unique-colour, CI-зависимости | High | M | P-07, частично P-01 |
| P-09 | Редкий повторный захват при OCR | Medium | S | замер |
| P-10 | Кэш перевода O(1) + опциональная персистентность | Medium | S–M | P-05 |
| P-11 | Голосование коротких токенов | Medium | M | — |
| P-12 | Мониторы/scale | Medium | M | P-01 |
| P-13 | Без PNG через диск | Medium | M | P-07 |
| P-14 | Мелкие аллокации | Low | S | замер |

Усилие: S ≈ до 1 дня, M ≈ 2–4 дня, L ≈ неделя+ (оценки мои, не измерения).

---

## 5. Дорожная карта

Каждый этап заканчивается зелёным `bash tests/inplace/run.sh` в окружении `tmp/env.sh` и отдельным коммитом. Не смешивать этапы в одном коммите.

### Этап 0 — Подготовка окружения и базовая линия
- [ ] Создать `tmp/env.sh` по §0.1 (подтвердить у владельца проекта решение про `CARGO_HOME`)
- [ ] Зафиксировать базовую линию: `cargo test --workspace` (число passed/ignored/failed), `qmltestrunner`, `pytest tests/inplace/python` — сохранить в `tmp/baseline/` и в `docs/` кратко
- [ ] Собрать `inplace_bench` и записать базовые времена стадий (capture, ocr, translate, place) — **только измеренные значения**
- [ ] Измерить стоимость `capture.grab` на KWin (для решения по P-09)
- [ ] Проверить, что `git status` чист, кроме `graphify-out/` и `settings-main.png` (не относятся к плану)

### Этап 1 — Доверие к проверкам (P-01, P-07, часть P-08)
- [ ] P-07: `LIPAX_RUNTIME_DIR` в `lipa-core`, применить в `bridge.rs`, `portal.rs`, `geometry.rs`
- [ ] P-01.1: лог `platform=` при старте, live-тест падает при несовпадении платформы
- [ ] P-01.1: выяснить причину `xdg_toplevel` в `evidence/xwayland/application.log`
- [ ] P-01.3: автопересчёт хешей `manifest.json`, красный тест при расхождении
- [ ] P-01.2: исследование вложенного `kwin_wayland --virtual` (результат — рабочий скрипт или документированное ограничение)
- [ ] P-08.5: добавить `layer-shell-qt kwindowsystem` в `checks.yml`; убедиться, что CI собирает
- [ ] Обновить устаревшие места в `docs/INPLACE_VALIDATION.md §7` и `docs/ARCHITECTURE.md:194` (W15)

### Этап 2 — Окна и ввод (P-02, P-06)
- [ ] P-02 шаг 0: ручная проверка перехвата кликов in-place окном, результат в `tests/overlay/README.md`
- [ ] P-02: `WindowTransparentForInput` (или маска) для `inplaceTranslation`, решение по MMB-скрытию
- [ ] P-02: QML-тест флага + live-тест доставки клика нижнему окну
- [ ] P-06: проверить в документации KWin 6.7 свойства/сигналы `tile`, `maximized`; выбрать вариант А или Б
- [ ] P-06: реализация + тест сброса tile/maximize в вложенном KWin
- [ ] Ручная проверка на KDE Wayland и XWayland, записать результаты как **[VERIFIED вручную]**

### Этап 3 — Качество замены текста (P-03)
- [ ] Метрика «остаток исходных чернил» в `placement_quality.rs`
- [ ] Прогон на всех фикстурах, зафиксировать порог (число из измерения)
- [ ] Изменение `resolve_mode(Auto)` / alpha шва по результату измерения
- [ ] Эталонные скриншоты (P-08.1) и unique-colour тест (P-08.2)
- [ ] Обновить `docs/INPLACE_VALIDATION.md`

### Этап 4 — Перевод (P-04, P-05, P-10)
- [ ] P-04.1: подтвердить построчность Bergamot CLI на `bergamot_native.rs`
- [ ] P-04: пакетный запрос `translate_many`, адаптация `translate_changes`
- [ ] P-04: замер латентности до/после на `inplace_bench`
- [ ] P-05: `translate/guard.rs` + таблица тестов
- [ ] P-05: подключение в `translate_shared` перед `cache.put`
- [ ] P-10: O(1) LRU; опциональный JSONL-кэш с правами 0600

### Этап 5 — Производительность и OCR (P-09, P-11, P-14)
- [ ] P-09: по замеру этапа 0 — адаптивный `STALE_PROBE` или закрыть как ненужный
- [ ] P-11: `ocr/vote.rs`, тесты «is↔15», реплей-фикстуры без регрессий
- [ ] P-14: микрооптимизации только при измеримом выигрыше

### Этап 6 — Мониторы, scale, остальное (P-12, P-13)
- [ ] P-12: тесты двух выходов и scale 1.5 в вложенном KWin (если возможно), иначе ручной протокол
- [ ] P-12: разрезание рамки по выходам
- [ ] P-13: `QQuickImageProvider` для подложек/превью
- [ ] Проверить, что в `$LIPAX_RUNTIME_DIR` нет лишних PNG

### Этап 7 — Закрытие
- [ ] Полный прогон: `cargo test --workspace`, `cargo clippy --workspace`, `qmltestrunner` при scale 1/1.25/1.5/2, `pytest`
- [ ] Обновить `docs/IMPROVEMENTS.md`, `docs/ARCHITECTURE.md`, `README` (без изменения публичного поведения, не описанного в плане)
- [ ] Список `NOT RUN` в итоговом отчёте — честно

---

## 6. Общие замечания о рисках и регрессиях

- **Сохранение поведения.** Любое изменение, затрагивающее режим по умолчанию (P-03), включается через настройку и обкатывается на фикстурах перед сменой дефолта.
- **Трейты и тестовые моки.** Изменение `Translate` (P-04) затронет моки в `pipeline.rs:1100+`; ввести метод с реализацией по умолчанию, чтобы моки не менялись.
- **Qt ≥ 6.12.** Эталоны привязываются к версии Qt и набору бандл-шрифтов.
- **Зависимости.** План **не добавляет** crate'ов: LRU, JSONL, guard и vote пишутся на std. Единственная возможная системная зависимость — доработка поставки Bergamot (P-04.2), её решает владелец проекта.
- **Что осталось неподтверждённым** (выше помечено [ASSUMPTION]/[NOT RUN]): последствия W1 для игры, причина W11, достаточность `pacman` в CI (W10), построчность Bergamot CLI, имена API KWin Scripting для tile/maximize, возможность нескольких выходов в `kwin_wayland --virtual`, отсутствие проверок качества перевода (W5) за пределами grep, поведение `Reconstruct` на шве (альфа 128).

---

## 7. Итог: самое важное и порядок внедрения

1. **Сначала доверие к проверкам** (P-01, P-07): без достоверного XWayland/Wayland-прогона и изоляции в `tmp/` остальные изменения не проверить честно. В частности, существующее «XWayland-свидетельство» по логу выглядит как Wayland.
2. **Затем ввод и окна** (P-02, P-06): in-place окна не отключают ввод (проверено по коду), плавающее окно подвержено snap/tile — это прямые требования к «свободным окнам» и стабильности на KWin.
3. **Качество замены текста** (P-03): режим по умолчанию не использует «замену без плашки»; сначала измерить остаток исходного текста, потом менять `Auto`.
4. **Латентность и надёжность перевода** (P-04, P-05, P-10): Bergamot запускается процессом на каждый абзац; результаты перевода не проверяются; кэш не переживает рестарт.
5. **Потом оптимизации и OCR-доработки** (P-09, P-11, P-14), **мониторы/scale** (P-12) и **устранение PNG на диске** (P-13) — по результатам замеров.

Из Kizurium имеет смысл брать *идеи и тесты* (пустой input region с переустановкой, порог непрозрачности + тест «призрака», проверки качества перевода и плейсхолдеры, голосование коротких токенов, правило «нет измерения ≠ пройдено»), а не код: он Python/GTK и привязан к wlroots, а по захвату, единицам координат, трекингу полей, кэшу размещения и CI-структуре LipaX-QT уже сильнее.
