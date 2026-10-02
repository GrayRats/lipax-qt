# Архитектура LipaX-QT

Переводчик текста в играх для Linux: KDE Plasma 6, KWin, Wayland/XWayland, игры через Wine/Proton.
Пользователь один раз выбирает окно и область с текстом, дальше программа сама следит за областью,
распознаёт появляющийся текст и обновляет перевод.

## Статус компонентов

| Компонент | Статус |
|---|---|
| Ядро: настройки, кэш, change detection, сравнение текста, pipeline | готово, покрыто тестами |
| Переводчики: Google, Yandex, свой API | готово; реальные запросы Yandex/custom не проверены |
| OCR: Tesseract через stdin/stdout | готово, проверено на живом кадре |
| Захват KWin ScreenShot2 + выбор окна `queryWindowInfo` | готово, проверено на живом окне (Konsole) |
| Qt GUI: главное окно, настройки, выбор области | написано, в offscreen загружается; в живой сессии не проверено |
| Overlay | layer-shell (слой Overlay); положение проверено снимком экрана на тестовом окне; сам overlay с переводом и перетаскивание в живой сессии не проверены |
| Рамка вокруг выбранного окна и области | layer-shell, цвет/толщина/время в настройках; отрисовка проверена снимком; точность геометрии окна KWin не проверена на живом окне |
| TesseractManager: обнаружение, языки, пакеты, установка | готово; Arch проверен на живой системе, Debian/Ubuntu — фикстурами и командами в контейнере |
| Запасной захват: xdg-desktop-portal ScreenCast + PipeWire | написан; вызовы портала приняты (дошли до диалога), нарезка PNG, конвейер GStreamer и передача fd проверены отдельно; сквозной захват окна после диалога не проверен |
| Глобальные горячие клавиши | KGlobalAccel, переназначение в настройках на лету, проверка конфликтов; проверено через `invokeShortcut`, реальное нажатие не проверено |

## Раскладка проекта

```
Cargo.toml                 workspace
crates/core/               логика без Qt (lib lipa-core)
  src/settings.rs          Settings, TOML в ~/.config/lipa/config.toml, права 0600
  src/capture/mod.rs       трейт Capture, rect_px (доли окна -> пиксели)
  src/capture/kwin.rs      KwinCapture: ScreenShot2, queryWindowInfo, getWindowInfo
  src/capture/portal.rs    PortalCapture: ScreenCast + PipeWire через gst-launch-1.0
  src/detect.rs            ChangeDetector
  src/ocr/mod.rs           трейт Ocr, Tesseract, preprocess
  src/translate/mod.rs     трейт Translate, Translator (Google/Yandex/Custom), HttpTranslate
  src/cache.rs             TranslationCache (LRU)
  src/text.rs              normalize, similarity, is_meaningful
  src/pipeline.rs          Pipeline, Event, Cmd
  src/hotkeys.rs           глобальные клавиши через org.kde.KGlobalAccel
  src/tesseract.rs         TesseractManager: Tesseract, языки, дистрибутив, пакеты
  examples/probe.rs        ручная проверка захвата и OCR
  examples/hotkeys.rs      ручная проверка горячих клавиш (аргумент rebind — смена на лету)
  examples/tesseract.rs    вывод TesseractInfo для текущей системы
  examples/portal.rs       диалог portal, затем кадр окна в PNG
crates/app/                бинарь lipa: cxx-qt + QML
  src/bridge.rs            Controller (QObject)
  qml/main.qml             главное окно
  qml/SettingsWindow.qml   настройки
  qml/RegionSelector.qml   выбор области на замороженном кадре
  qml/TranslationOverlay.qml  окно с переводом (layer-shell)
  qml/FrameOverlay.qml     рамка выбранного окна/области (layer-shell)
  qml/HotkeyButton.qml     поле захвата сочетания клавиш
packaging/                 .desktop (с X-KDE-DBUS-Restricted-Interfaces)
```

Ядро не зависит от Qt и тестируется отдельно (`cargo test -p lipa-core`). GUI-поток Qt только рисует:
захват, OCR и сеть работают в задачах tokio.

## Pipeline

```
Capture -> Change Detection -> debounce -> OCR -> сравнение текста -> кэш -> перевод -> GUI/Overlay
```

Реализован в `Pipeline::tick` (`crates/core/src/pipeline.rs`), а `Pipeline::run` вызывает его по таймеру.

1. **Capture.** Берётся кадр окна и обрезается по области (доли окна от 0 до 1).
2. **Change detection.** Кадр уменьшается до 64x64 в градациях серого и сравнивается с предыдущим по
   средней абсолютной разнице яркости. Порог — настройка `sensitivity`. Если кадр не менялся, OCR не вызывается.
3. **Debounce.** Пока кадр меняется (анимация появления текста), запоминается время последнего изменения.
   OCR запускается, когда кадр неизменен не меньше `debounce_ms`. Промежуточные кадры не переводятся.
4. **OCR.** Tesseract получает PNG на stdin (после увеличения 2x и перевода в серый) и отдаёт текст в stdout.
   Временных файлов нет, запуски не конфликтуют. Процесс убивается при отмене задачи (`kill_on_drop`).
5. **Сравнение текста.** Текст нормализуется (пробелы), мусор без букв и цифр отбрасывается.
   Если похожесть с прошлым текстом (Левенштейн) не ниже 0.92, считается, что текст прежний
   (шум OCR) и перевод не повторяется.
6. **Кэш.** LRU на 512 записей по ключу «язык источника + язык перевода + нормализованный текст».
7. **Перевод.** Запрос с таймаутом 10 с. При ошибке прежний текст сбрасывается, и следующая попытка повторит перевод.
8. **Событие.** `Event::Translation` отправляется в GUI-поток (в `bridge.rs` через `qt_thread().queue`).

### Отсутствие очереди

Цикл последовательный: пока выполняются OCR или перевод, тики таймера не обрабатываются
(`MissedTickBehavior::Skip`). Очередь запросов не копится, а следующий кадр всегда свежий,
поэтому устаревшие кадры отбрасываются сами.

### Ручной режим и сброс

`Cmd::TranslateOnce` делает один проход без change detection и debounce.
`Cmd::Reset` сбрасывает состояние при смене окна, области или настроек.
Авто-режим работает, пока включены «Запустить» и `auto_translate`.

## Wayland и безопасность

Wayland не позволяет приложению читать содержимое чужих окон, рисовать поверх них или
следить за их положением. Проект использует только разрешённые механизмы KDE и не обходит модель безопасности.

### Захват: KWin ScreenShot2

- Интерфейс `org.kde.KWin.ScreenShot2` (на проверенной системе версия 5), метод `CaptureWindow(uuid, options, fd)`.
- Кадр одного окна, поэтому overlay переводчика и другие окна в него не попадают.
- Кадр передаётся сырыми пикселями через pipe. Читатель работает параллельно с D-Bus-вызовом,
  иначе большой кадр блокирует запись. Форматы `QImage::Format` 4–6 (BGRA) и 16–18 (RGBA) преобразуются в RGBA.
- **Доступ ограничен.** KWin разрешает вызов только приложению, чей `.desktop` содержит
  `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2` и чей `Exec` совпадает с запущенным бинарником.
  Файл лежит в `packaging/io.lipa.Translator.desktop`. Для запуска из `target/` нужна отдельная
  запись в `~/.local/share/applications` с абсолютным путём.
- Захватывается всё окно, затем кадр обрезается. На окнах большого размера (например, 4K) это дорого.
  Если окажется узким местом, нужно переходить на захват области.

### Выбор окна

`org.kde.KWin.queryWindowInfo` запускает интерактивный выбор: пользователь кликает по окну, KWin возвращает
`uuid`, `resourceClass` и `caption`. Отмена (Esc) приходит как D-Bus-ошибка. `getWindowInfo(uuid)` проверяет,
что окно ещё существует. Скрипты KWin для списка окон не нужны.

Окна Wine и Proton работают через XWayland, но KWin идентифицирует их так же, как нативные Wayland-окна.

### Область

Область хранится в долях окна (x, y, w, h от 0 до 1), а не в координатах рабочего стола, поэтому переживает
перемещение и масштабирование окна. При смене окна область сбрасывается.

Выбор области выполняется на замороженном кадре в собственном окне переводчика (`RegionSelector.qml`).
Это осознанное решение: рисовать окно выбора поверх чужого окна на Wayland нельзя.

### Overlay

На Wayland обычное окно не может надёжно оставаться поверх остальных: флаг `WindowStaysOnTopHint`
не поддерживается протоколом xdg-shell. Поэтому `TranslationOverlay.qml` — поверхность wlr-layer-shell
(KWin его поддерживает), которую даёт библиотека layer-shell-qt через QML-модуль `org.kde.layershell`.

- Слой `LayerOverlay`, `scope = "lipa-overlay"`, `exclusionZone = -1` (не зависит от панелей), фокус клавиатуры не берёт.
- Положение задаётся отступами от левого верхнего угла (`anchors` Top|Left, `margins`). У layer-поверхности
  свойства `x`/`y` не действуют, поэтому `overlay_pos` попадает именно в `margins`.
- Click-through: `Qt.WindowTransparentForInput`. Если он выключен, overlay таскается мышью: отступы
  меняются вручную (`startSystemMove` для layer-поверхности недоступен), на отпускании сигнал `moved`
  сохраняет `overlay_pos` через `applySettings`.
- Не проверено вживую: поведение поверх полноэкранной игры (Proton/XWayland) и плавность перетаскивания.
- Требуется пакет `layer-shell-qt`. QML-модуль подключается без `optional`, на системе без него приложение не запустится.

### Запасной захват: xdg-desktop-portal + PipeWire

`PortalCapture` (`crates/core/src/capture/portal.rs`) — второй бэкенд за трейтом `Capture`. Работает на любом
композиторе с порталом и не требует доступа к KWin ScreenShot2.

1. `CreateSession` → `SelectSources` (окно или экран, курсор скрыт, `persist_mode = 2`) → `Start`: системный диалог,
   согласие пользователя. Ответы приходят сигналом `Request::Response`; подписка создаётся до вызова, чтобы не потерять ответ.
2. `OpenPipeWireRemote` возвращает fd. Его получает дочерний `gst-launch-1.0 pipewiresrc fd=… path=<node> ! videorate
   drop-only=true max-rate=4 ! videoconvert ! pngenc ! fdsink`: GStreamer сам договаривается с PipeWire о формате буферов.
3. Поток склеенных PNG режется на кадры (`PngSplitter`, по чанкам до `IEND`), хранится только последний.
   Portal отдаёт кадры лишь при изменении окна, поэтому последний кадр всегда актуален. Не более 4 PNG в секунду.
4. `restore_token` сохраняется в `portal_token`: при следующем запуске окно выбирается без диалога. Если процесс
   `gst-launch-1.0` упал, сессия восстанавливается по токену при следующем кадре.

**Выбор бэкенда.** `AnyCapture` решает по ключу окна: `portal:window` — portal, любой другой — uuid KWin. Поэтому
переключение в настройках не ломает уже выбранное окно. «Авто» (по умолчанию) сначала пробует KWin, а при ошибке
предлагает выбор через portal.

Ограничения:
- Нужны `gstreamer` и `gst-plugin-pipewire` (плюс `gst-plugins-base/good` для `videoconvert` и `pngenc`).
- Окно выбирается только в диалоге портала; название окна и геометрия портал не сообщает, поэтому рамка выбора
  для portal-окна не показывается, а в списке окно называется «Окно (portal)».
- Область хранится в долях кадра, как и для KWin.
- Без проверки на живой сессии: `OpenPipeWireRemote`, запуск `gst-launch-1.0` с настоящим узлом и первый кадр. Для
  проверки: `cargo run -p lipa-core --example portal` (диалог, затем PNG в `/tmp/lipa-portal.png`).

## Горячие клавиши

Wayland не даёт приложению читать клавиатуру в обход композитора, поэтому используется штатный
механизм KDE `org.kde.KGlobalAccel` (`crates/core/src/hotkeys.rs`).

1. `doRegister` и `setShortcut(actionId, keys, flags)` для действий компонента `lipa`: `toggle`, `select_region`,
   `translate_once`, `toggle_overlay`. Умолчания: `Ctrl+Alt+P/R/Y/H`. Строки вида `Ctrl+Alt+P` переводятся
   в коды Qt функцией `parse_key`.
2. Нажатия приходят сигналом `globalShortcutPressed` на `/component/lipa`.
3. `Controller` выполняет `toggle` и `translate_once` сам, а `select_region` и `toggle_overlay`
   отдаёт в QML сигналами `selectRegionRequested` и `toggleOverlayRequested`.

**Переназначение.** Все четыре сочетания меняются в «Настройки → Горячие клавиши» (`HotkeyButton.qml`: нажать
кнопку, затем сочетание; Backspace очищает, Esc отменяет; без модификатора допустимы только F1–F24).
После «Применить» `Shared::update` пересылает новые клавиши слушателю через `watch`, и они назначаются
без перезапуска. Кроме того, их можно менять в «Системных настройках → Сочетания клавиш → LipaX».

**Конфликты.** `setShortcut` на занятую клавишу отвечает «успехом» и записывает неактивное сочетание,
поэтому перед назначением владелец проверяется через `action(key)`. Занятая чужим действием клавиша не
назначается, а в статусе появляется сообщение. Так обнаружено, что `Ctrl+Alt+O` занята Crow Translate,
поэтому умолчание для overlay — `Ctrl+Alt+H`.

Особенности:
- При запуске автозагрузка включена: клавиша, назначенная пользователем в системных настройках, важнее значения из `config.toml`.
  Выбор в форме настроек, наоборот, перекрывает сохранённое значение (`NoAutoloading`).
- Если KGlobalAccel недоступен, приложение работает, а причина показывается в статусе.

## Рамка выбора

После выбора окна и после выбора области вокруг них на несколько секунд появляется тонкая рамка
(`FrameOverlay.qml`): layer-shell поверхность без заливки и с `WindowTransparentForInput`, то есть внутри
рамки всё видно и кликается как раньше. Настройки: цвет `#rrggbb` (по умолчанию красный), толщина
(2 px), время показа (3 с, 0 — не показывать).

Геометрию окна отдаёт KWin (`getWindowInfo`: `x, y, width, height`), область считается долями окна
(`WindowGeometry::region`). Отступы layer-shell считаются от угла экрана, на котором лежит центр рамки.
Ограничение: захват идёт без рамки окна (`include-decoration = false`), а геометрия KWin — с ней, поэтому у окон
с серверной декорацией рамка области может быть смещена на высоту заголовка. Для игр без декораций совпадает.

## Tesseract и языки

`TesseractManager` (`crates/core/src/tesseract.rs`) — единственное место, которое запускает `tesseract`,
`pacman`, `dpkg` и `apt-cache`. GUI получает готовый JSON (`Controller.tesseractJson`).

Что определяется:
- наличие и путь `tesseract` — по PATH, без жёстких путей;
- версия — `tesseract --version`;
- языки и `tessdata` — `tesseract --list-langs` (заголовок содержит каталог); `osd` скрыт как служебный;
- дистрибутив — `/etc/os-release` (`ID`, `ID_LIKE`): Arch/CachyOS/EndeavourOS/Manjaro → `pacman`,
  Debian/Ubuntu/Mint и производные → `apt` (`dpkg`); остальные — `Family::Other` без пакетных действий;
- пакет Tesseract и данных — `pacman -Qoq` / `dpkg -S` (владелец файла), а не догадка по имени;
- языки, которых нет, но есть в репозитории, — один запрос `pacman -Ssq` / `apt-cache search`.

Имена пакетов зависят от системы: Arch `tesseract-data-<код>` (`tesseract-data-jpn_vert`),
Debian/Ubuntu `tesseract-ocr-<код>` с дефисом вместо подчёркивания (`tesseract-ocr-jpn-vert`); движок —
`tesseract` и `tesseract-ocr`. Новый дистрибутив добавляется вариантом `Family`/`PackageManager` и фикстурой в тестах.

**Выбор языков.** В настройках один основной язык и любое число дополнительных. Хранится одной строкой
в формате Tesseract (`source_lang = "jpn+eng"`): первый язык — основной, он же язык источника для сервиса
перевода (`primary_lang`). Язык перевода один.

**Отсутствующие языки.** Выбранный, но не установленный язык помечается в списке; под ним показывается
сообщение вида «Не установлен языковой пакет Tesseract (Japanese, jpn): tesseract-data-jpn» с именем пакета
для текущего дистрибутива. Ошибка OCR («Failed loading language», нет `tesseract`) превращается в то же
сообщение (`OcrError::Setup`); проверка запускается только после ошибки.

**Установка.** Ничего не ставится автоматически. Кнопка «Установить языковой пакет» сначала показывает диалог
с точной командой (`pkexec pacman -S --needed --noconfirm …` или `pkexec apt-get install -y …`), и только
после «Да» `Controller.installPackage` её выполняет; пароль запрашивает polkit. Допустимы только пакеты
Tesseract текущего дистрибутива (`is_allowed_package`). Пакеты вне репозиториев (например, из AUR)
не ставятся: они помечены как недоступные. После успеха список языков обновляется сам; кнопка
«Обновить список языков» делает то же вручную.

## Настройки

Файл `~/.config/lipa/config.toml` (запись атомарная, права 0600, потому что внутри могут быть API-ключи).
Отсутствующие поля заполняются значениями по умолчанию.

Поля: языки источника (код Tesseract) и перевода (ISO), OCR-движок, сервис перевода и его ключи,
бэкенд захвата (auto/kwin/portal) и токен portal, интервал, чувствительность, debounce, авто-перевод, режим и параметры overlay
(шрифт, прозрачность, положение, размер, click-through), рамка выбора (цвет, толщина, время), горячие клавиши, запомненное окно и область.

Окно и область меняются отдельными действиями, поэтому форма настроек их не затирает (`apply_settings`).

### Сервисы перевода

| Сервис | Запрос |
|---|---|
| Google | неофициальный endpoint `translate.googleapis.com/translate_a/single?client=gtx`, без ключа |
| Yandex | Cloud Translate v2: POST `translate.api.cloud.yandex.net/translate/v2/translate`, заголовок `Authorization: Api-Key`, поля `folderId`, `texts`, `targetLanguageCode`, `sourceLanguageCode` |
| Свой API | POST на заданный URL, формат LibreTranslate: `{q, source, target, format, api_key}`, ответ `{translatedText}` |

Код языка Tesseract (`eng`, `rus`) переводится в ISO функцией `tess_to_iso`. Google и неофициальный endpoint
могут ограничивать частоту запросов, поэтому нужны кэш и сравнение текста.

## Qt и зависимости

- Цель — Qt 6.12 LTS. Сейчас собирается и проверено на Qt 6.11.2 (Arch). Код не должен использовать API,
  доступный только в 6.12.
- Стек: Rust, cxx-qt 0.10, QML (Qt Quick Controls), tokio, zbus, reqwest (rustls), image.
- Внешние программы и библиотеки: `layer-shell-qt`, `gstreamer` + `gst-plugin-pipewire` (portal-захват), `pkexec` (для установки языков), `tesseract` с языковыми данными (`eng`, `rus` и другие по необходимости).
- GUI взаимодействует с ядром через один QObject `Controller`. Настройки передаются как JSON-строка
  (`settingsJson` / `applySettings`), что избавляет от десятков отдельных свойств.

## Известные ограничения

- Tesseract хуже справляется со стилизованными игровыми шрифтами. Движок спрятан за трейтом `Ocr`,
  чтобы позже добавить другой (например, PaddleOCR).
- Формат запроса Yandex и работа custom API проверены только тестами разбора ответов.
- KWin-бэкенд привязан к KDE (ScreenShot2). На других композиторах используется portal (см. выше), он проверен меньше.
- GUI проверен только в offscreen-режиме.
