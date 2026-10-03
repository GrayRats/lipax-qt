# PLAN — TASK4: разделение состояния, свободное окно перевода, фон/шрифты/коллизии «поверх оригинала», пакет `lipax`

## Context

`TASK4.md` описывает три класса проблем, подтверждённых чтением кода:

1. **Режим «поверх оригинала» зависит от галочки «Поверх игры» окна перевода.** В `main.qml`
   видимость `InplaceText` = `root.inplaceActive && overlayEnabled.checked …`; MMB по полю
   выполняет `overlayEnabled.checked = false`; горячая клавиша `toggle_overlay` переключает ту же
   галочку; `inplaceActive` требует `ctl.gameGeometry`, иначе показывается окно перевода. Одна
   флага управляет двумя рендерами.
2. **Открепленное окно перевода «прилипает» к краям и «прыгает».** Корневые причины (в коде, не KWin):
   - открепленное окно — всё ещё **layer-shell поверхность** (`LayerShell.Window.*` в
     `TranslationOverlay.qml`): позиция = якоря Top|Left + отступы от `targetScreen`; это не
     `xdg_toplevel`, KWin его не двигает, `startSystemMove()` невозможен;
   - **зажим к границам экрана**: `actualX/actualY = clamp(pos, 0 … screen − size)` и такой же зажим
     в `onPositionChanged` → окно упирается в край и не может уйти на другой монитор;
   - **перепривязка к монитору игры**: `updateScreen()` вызывается на `onGameGeometryChanged`,
     `onSettingsChanged` (в т. ч. после сохранения позиции из `onMoved`) и `onScreensChanged`,
     выбирает экран по наибольшему пересечению с окном игры и выставляет `win.screen`
     (+ `relocating` прячет/пересоздаёт поверхность) → «телепорт» на прежний монитор;
   - **старая позиция возвращается из настроек**: `onSettingsChanged` при любой записи настроек
     перезаписывает `posX/posY` из `overlay_pos`.
   Нативный snapping KWin сейчас не участвует (layer-shell он не двигает). После перехода на
   `xdg_toplevel` + `startSystemMove()` привязка к краям/зонам KWin — поведение композитора.
3. **«Поверх оригинала»**: в UI видны внутренние алгоритмы фона; шрифты берутся из системы
   (fontconfig); нет защиты от перекрытия соседних полей.

Плюс: пакет переименовать в **`lipax`**.

Архитектура `crates/core/src/layout/` (детектор, трекер, классификатор, fit, engine; «шрифт
выбирается один раз на поле») **сохраняется** — меняются только фон-стратегия, источник шрифтов
и добавляется `collision.rs`. Логирование уже есть (`crates/app/src/logging.rs`: tracing +
Qt message handler + panic hook) — используем его, второй логгер не вводим.

## Решения по умолчанию (подтвердить при утверждении)

- **Пакет `lipax`**, исполняемый файл `/usr/bin/lipax` (cargo bin `lipax`), `/usr/bin/lipa` —
  симлинк для совместимости; desktop ID `io.lipa.Translator` и каталог настроек `~/.config/lipa`
  не меняются (разрешение ScreenShot2 у KWin и настройки пользователей). `Exec=/usr/bin/lipax`
  (KWin сверяет реальный путь процесса). `provides/conflicts=('LipaXQT' 'lipa')`.
- **Встроенные шрифты** (все SIL OFL 1.1): Inter (variable), Noto Sans (variable), Noto Serif
  (variable), JetBrains Mono (variable), Noto Sans CJK и Noto Serif CJK (Regular + Bold, OTC:
  JP/KR/SC/TC в одном файле). Файлы **не хранятся в git**: PKGBUILD скачивает их с официальных
  релизов с sha256 и ставит в `/usr/share/lipax/fonts/`; для разработки —
  `packaging/fetch-fonts.sh` → `crates/app/assets/fonts/` (в `.gitignore`). Оценка размера:
  ≈ 5 МБ латиница/кириллица + ≈ 75–90 МБ CJK — проверить фактические размеры до коммита; если
  слишком много — вынести CJK в split-пакет `lipax-fonts-cjk` (тогда при отсутствии CJK-шрифта
  поле не рисуется поверх оригинала, а пишется WARN и перевод показывается в окне перевода).
- **Незакоммиченные правки другого агента** (logging, translate, pipeline, docs, PKGBUILD,
  удалённый `PLAN.md` …): собрать, прогнать тесты и закоммитить **отдельным коммитом** до начала
  работы, чтобы отделить их от TASK4.

## Шаги (в порядке TASK4 §63)

### 0. Подготовка
- Закоммитить чужие правки отдельно (см. выше); скопировать этот план в `PLAN.md` (корень).

### 1. Состояние: Inplace и Window разделены (§1–4, §57)
- `crates/core/src/settings.rs`: `enum TranslationDisplay { Window, Inplace }`
  (serde `"window"`, `alias = "overlay"` для старых конфигов) вместо строки `translation_display`.
  Обновить все сравнения в `pipeline.rs`, `layout/engine.rs`, QML.
- `crates/app/src/bridge.rs`: runtime-свойства, принадлежащие Rust:
  `inplaceVisible`, `windowOverlayVisible` (+ invokables `setInplaceVisible`, `setWindowOverlayVisible`);
  `effectiveDisplay` = `inplace` | `window` (+ причина отката, напр. «portal: нет геометрии окна»,
  в лог `inplace.state`). Горячая клавиша `toggle_overlay` переключает видимость **активного**
  режима, `toggle_pin` действует только в Window.
- `main.qml`: `InplaceText.visible` = `ctl.inplaceVisible && effectiveDisplay === "inplace" && …`;
  MMB → `ctl.setInplaceVisible(false)`; окно перевода видимо только при `effectiveDisplay === "window"
  && ctl.windowOverlayVisible`. Галочка «Поверх игры» управляет только Window; в режиме Inplace
  вместо неё — «Перевод поверх оригинала» (`inplaceVisible`).

### 2. Окно перевода: две поверхности, одно состояние (§5–24, §58–59)
- Разделить `TranslationOverlay.qml`:
  - `OverlayContent.qml` — общий контент (фон, рамка, текст, ручка закрепления, крестик);
    без позиционирования окна.
  - `PinnedOverlayWindow.qml` — layer-shell (`LayerOverlay`, якоря Top|Left + отступы), click-through
    с маской ввода (ручка), радиус 0, без крестика, без LMB-перетаскивания. Экран и отступы
    вычисляются **один раз при закреплении** из последней свободной геометрии (экран по центру
    окна); `onGameGeometryChanged`/`onSettingsChanged` его **не двигают**.
  - `FloatingOverlayWindow.qml` — обычное `xdg_toplevel` (**без** `LayerShell.Window.*`),
    `flags: Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint`, радиус 12, рамка,
    крестик, ввод не пропускается; LMB → `startSystemMove()`, MMB → закрепить. Нет зажима
    к экрану, нет `screen =`, нет `onReleased`-перерасчёта.
  - `TranslationOverlay.qml` — контроллер: `Loader` создаёт ровно одну из двух поверхностей
    по `pinned` (смена роли = пересоздание платформенного окна — корректно для Wayland).
- Позиция свободного окна на Wayland (приложение её не знает и не задаёт):
  расширить `crates/core/src/capture/geometry.js` + `geometry.rs` — скрипт KWin находит наше
  свободное окно (pid + `objectName`/заголовок «LipaX — перевод»), при появлении запрашивает
  у нас сохранённую геометрию через `callDBus(..., callback)` и ставит `frameGeometry` +
  `keepAbove = true`; по `interactiveMoveResizeFinished` присылает итоговую геометрию и выход.
  Rust сохраняет `floating_geometry {x,y,w,h,output}` в настройки **после** перемещения.
  Восстановление: если выход/геометрия недоступны — только сдвиг в видимую область
  ближайшего экрана (без угловых пресетов). На X11 — обычные `x/y` (изолировано по платформе).
- Ввод: сохранить `configureOverlayInput` (маска только для Pinned); Floating — маска пустая.
- Удалить из старого кода: `actualX/actualY`-зажим, ручной drag через `posX/posY`,
  `updateScreen()`-перепривязку на каждое изменение, перезапись `posX/posY` в `onSettingsChanged`.
- Логи (`tracing` в Rust, `LoggingCategory` в QML → существующий Qt handler):
  `overlay.state` (pinned/floating, роль поверхности), `overlay.drag` (system_move_started),
  `overlay.geometry` (move_finished + кто изменил: restore_geometry/pin_transition),
  `overlay.layershell` (active, anchors, margins, screen), `overlay.screen`.

### 3. Фон «поверх оригинала»: 4 публичных режима (§26–30, §54)
- `settings.rs`: `InplaceBackgroundStyle { Auto, TextReplacement, TransparentOutline, PaddedFill }`
  (миграция: inpaint_blur/solid_fill → text_replacement, adaptive_padding_fill → padded_fill,
  transparent → transparent_outline). Настройки контура: `outline_color: PropertyMode<String>`,
  `outline_width`, `shadow`, `text_opacity`; заливки: `fill_color: PropertyMode<String>`,
  `fill_opacity`, `padding_x`, `padding_y`, `extra_margin`, `corner_radius`.
- `layout/background.rs`: внутренние алгоритмы `InpaintBlur/SolidFill/AdaptivePaddingFill/Transparent`
  остаются внутренними. Новая функция выбора стратегии **по каждому полю** — оценка (score)
  по типу блока, однородности/сложности фона, контрасту, уверенности восстановления, запасу
  места; TextReplacement внутри выбирает InpaintBlur или SolidFill.
- Цвета контура/текста — по относительной яркости и контрасту с фоном (WCAG), белый/чёрный —
  только запасной вариант.
- QML `InplaceText.qml`: только рендер (контур через `Text.Outline`, тень — второй `Text` со
  сдвигом, без шейдеров), без логики выбора.
- UI: ровно 4 пункта «Фон под переводом»; поля контура/заливки показываются по режиму.

### 4. Встроенные шрифты вместо системных (§31–36, §55, §60)
- `layout/font_database.rs` → реестр встроенных шрифтов `BundledFontRegistry` (статическая таблица
  `BundledFont {family, file, category, scripts, weights, italic, monospace, condensed}`),
  `fc-list` больше не используется для inplace. `font_matcher.rs` выбирает только из реестра;
  кураторские списки сокращаются до встроенных семейств; системного fallback нет — если
  покрытия нет, `WARN inplace.font: no bundled font has full glyph coverage`.
- Приложение (`app_icon.h`/`icon.rs`): при старте `QFontDatabase::addApplicationFont` для
  каждого файла из `/usr/share/lipax/fonts` (или `assets/fonts` при разработке); ошибка
  загрузки → `ERROR inplace.font failed to load bundled font family=… path=… error=…`;
  реестр помечает недоступные семейства.
- «Шрифт выбирается один раз на поле» (`tracker.rs`, `engine.rs::lock_font`) — не трогаем;
  логи DEBUG/WARN при выборе и fallback (`block_id`, `block_type`, `script`, `category`, `confidence`).

### 5. Коллизии (§37–41, §56)
- Новый `layout/collision.rs`: для каждого поля `source_rect`, `safe_rect` (до соседей),
  `text_rect`, `background_rect`, `visual_rect` (текст + контур + тень + фон + поля).
  Порядок: перенос → межстрочный → трекинг → кегль (с разумным минимумом) → узкий
  встроенный вариант → меньше поля → сдвиг внутри safe_rect → ограничение фона.
  Не удалось — поле **не рисуется**, `WARN inplace.collision unable to place block … collision_with=…`
  + DEBUG с прямоугольниками и попытками.
- Подгонка по-прежнему в мосте (метрики Qt): `bridge.rs::inplace_placement` → сначала fit всех
  полей области, затем `CollisionResolver` с тем же `TextMeasure` (`icon.rs::QtMeasure`).

### 6. Настройки UI (§42–43)
- `SettingsWindow.qml`: в режиме Inplace скрыть только настройки окна перевода (положение,
  закрепление, скругление, blur окна, рамка окна, монитор окна); значения не удаляются.
  Настройки шрифтов, фона, контура, цвета, коллизий «поверх оригинала» — видимы.

### 7. Пакет `lipax`
- `PKGBUILD`: `pkgname=lipax`, бинарник `lipax` + симлинк `lipa`, шрифты в `/usr/share/lipax/fonts`
  (source + sha256), лицензии шрифтов в `/usr/share/licenses/lipax/`, `provides/conflicts`.
- `crates/app/Cargo.toml`: `[[bin]] name = "lipax"`; desktop `Exec=lipax`; `build-local.sh`,
  README, docs (`ARCHITECTURE.md`, `LOGGING.md`, `IMPROVEMENTS.md`) — новые имена.

## Повторно используемое
- `layout/engine.rs` (`begin/complete/finish`, `lock_font`), `layout/fit.rs::fit_translation_to_box`,
  `icon.rs::QtMeasure`, `layout::contrast_ratio`, `background.rs` анализ кольца.
- `app_icon.h::configureOverlayInput` (маска ввода), `configureOverlayBlur`.
- `capture/geometry.{js,rs}` (KWin-скрипт и D-Bus сервис) — расширяется для своего окна.
- `logging.rs` (tracing + Qt handler + panic hook).

## Проверка
- `cargo test --workspace`, `cargo clippy --workspace --all-targets`.
- QML (`qmltestrunner -input crates/app/tests`, offscreen):
  - новый `tst_display_state.qml`: Inplace не зависит от `windowOverlayVisible`; MMB по полю меняет
    только `inplaceVisible`; переключение режимов сохраняет настройки окна;
  - `tst_overlay.qml` → Floating/Pinned: радиус, рамка, крестик, маска ввода, LMB (вызов
    `startSystemMove` через подменённое окно), MMB, сохранённая геометрия при переходах;
  - `tst_inplace.qml`: 4 режима фона, контур/тень, пропуск поля при коллизии.
- Rust: тесты `collision.rs` (имя+реплика, реплика+кнопка, субтитр+HUD, два пункта меню, длинный
  перевод, большие поля, контур+тень: `visual_rect(A) ∩ safe_rect(B) = ∅` или пропуск),
  стратегия фона по полю, реестр шрифтов не зависит от fontconfig, тот же шрифт для того же
  `block_id` при другом переводе, миграция настроек.
- Вручную (KDE Plasma 6 / KWin / Wayland, `RUST_LOG=debug lipax`): свободное окно в центре, у
  краёв, между мониторами, после перезапуска; закрепление/открепление; Inplace при выключенной
  «Поверх игры»; логи `overlay.*`/`inplace.*` в терминале. Одного монитора у этой машины
  (DP-1 2560×1080) — мультимонитор и разный масштаб отметить как непроверенные.
- `packaging/build-local.sh` → `dist/lipax-*.pkg.tar.zst`.

## Ограничения Wayland/KWin (честно в итоговом отчёте)
- Приложение не задаёт позицию `xdg_toplevel`; позиция и «поверх всех» — через KWin-скрипт
  (только KDE). На других композиторах место выбирает композитор, позиция не восстанавливается.
- Привязка к краям/зонам при `startSystemMove()` — настройки KWin, приложение их не отключает.
- Полноэкранная активная игра в KWin выше слоя keep-above: для игры в полном экране —
  закреплённый режим (layer-shell).
