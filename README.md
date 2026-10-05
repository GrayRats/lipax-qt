# LipaX

LipaX помогает читать текст в играх: захватывает выбранное окно, распознаёт надписи и показывает перевод. Приложение рассчитано на KDE Plasma 6 с KWin и Wayland; интерфейс написан на Qt 6/QML, обработка кадров - на Rust.

[English](#english)

![Главное окно LipaX](docs/screenshots/main.png)
![Настройки LipaX — внешний вид перевода](docs/screenshots/settings.png)

## Что умеет

- Распознаёт текст через Tesseract; при желании можно подключить PaddleOCR. Переводит через Google, Yandex или настраиваемый совместимый сервис. Доступность перевода зависит от выбранного сервиса и сети.
- Позволяет задать до трёх областей: например, отдельно для субтитров, диалогов и меню. Каждую область можно временно отключить и настроить отдельно.
- Показывает результат в отдельном окне поверх игры или накладывает переведённый текст поверх исходного текста. Для второго режима нужна геометрия окна от KWin: при захвате через Portal приложение использует обычное окно перевода.
- Даёт настроить фон, рамку, прозрачность, скругление и шрифты из комплекта LipaX. Закреплённое окно оставляет доступ к игре; откреплённое можно двигать, видеть в панели задач и прокручивать колёсиком, если текст длинный.
- Сохраняет историю реплик, если включено её сохранение. Во вкладке «Статус» показывает готовность OCR и компонентов захвата; ошибки и диагностические сообщения доступны в терминале.

## Установка на Arch Linux

Понадобятся инструменты сборки Rust и зависимости из `PKGBUILD`.
Скрипт собирает пакет из текущих файлов проекта:

```bash
git clone https://github.com/GrayRats/lipax-qt.git
cd lipax-qt
./packaging/build-local.sh
sudo pacman -U dist/lipax-1.0.2-1-x86_64.pkg.tar.zst
```

Для сборки из клонированного репозитория также подходит `makepkg -si`. Пакет устанавливает команду `lipax`, ярлык приложения и встроенные шрифты. Для распознавания нужного языка установите соответствующий пакет данных Tesseract; [настройка PaddleOCR](docs/PaddleOCR.md) описана отдельно.

Локальную сборку для проверки захвата KWin запускайте через `packaging/run-local.sh`.
Скрипт собирает приложение и регистрирует отдельную скрытую desktop-запись для пути
локального бинарника, не меняя установленный ярлык. Для уже собранного бинарника:
`LIPAX_LOCAL_BINARY=/полный/путь/к/lipax packaging/run-local.sh`.
Сначала закройте другой экземпляр LipaX: повторный запуск передаёт команды уже работающему приложению.

Ошибка `ScreenShot2.Error.NoAuthorized` означает отказ KWin в разрешении на захват.
В KWin с проверкой desktop-файлов разрешение связано с путём исполняемого файла:
ярлык пакета разрешает `/usr/bin/lipax`, но не `target/debug/lipax` или `target/release/lipax`.
После обновления пакета перезапустите приложение; при устаревшем кэше KDE выполните
`kbuildsycoca6`. В режиме «Авто» отказ при проверке доступа приводит к выбору через portal.
Для удаления регистрации локальной сборки выполните `packaging/run-local.sh --unregister`
(или удалите `${XDG_DATA_HOME:-~/.local/share}/applications/io.lipa.Translator.Development.desktop`
и выполните `kbuildsycoca6`). Вкладка «Статус» показывает, разрешён ли захват текущему бинарнику.

## Первый запуск

1. Запустите `lipax` из меню приложений или терминала. В настройках проверьте язык распознавания, язык перевода и сервис перевода.
2. Нажмите «Выбрать окно», затем «Выбрать область» и обведите текст в игре. Вкладка «Область» позволяет добавить другие зоны и выбрать активную.
3. Выберите вид перевода в настройках и нажмите «Запустить». Перевод появится при обнаружении текста. При ошибке используйте кнопку «Повторить» и проверьте вкладку «Статус».

В режиме окна средняя кнопка мыши на окне перевода переключает закрепление. В режиме наложения поверх исходного текста средняя кнопка скрывает перевод; вернуть его можно переключателем в главном окне.

**Известные ограничения.** Иногда изменение настройки сохраняется, но не применяется к уже открытому окну или захвату, по этой причине может и быть ошибка перевода 429 (timeout). Если это произошло, полностью закройте LipaX и запустите его снова. Наложение поверх исходного текста недоступно при Portal-захвате, поскольку Portal не сообщает положение выбранного окна.
Отображение поверх конкретной полноэкранной игры и перенос между мониторами с разным масштабом стоит проверять в своей конфигурации.

## Диагностика и документация

При запуске из терминала LipaX пишет ошибки и диагностические сообщения в консоль. Более подробный журнал включается так:

```bash
RUST_LOG=debug lipax
RUST_LOG=debug lipax 2>&1 | tee lipax.log
```

По умолчанию включён уровень `INFO`; `ERROR` идёт в stderr, остальные сообщения — в stdout. Подробнее: [журналирование и уровни](docs/LOGGING.md), [архитектура проекта](docs/ARCHITECTURE.md), [доработки и ограничения](docs/IMPROVEMENTS.md).

## Инфо
Автор: GrayRat

Другие исходники и зависимости проекта:
• Изначальный проект / форк:
https://github.com/satix-one/lipa.git
• Зависимость:
https://github.com/rtr46/meikipop
• Tesseract OCR:
https://github.com/tesseract-ocr/tesseract
• Языковые данные Tesseract:
https://github.com/tesseract-ocr/tessdata
• PaddleOCR:
https://github.com/PaddlePaddle/PaddleOCR

## English

LipaX translates text captured from a game window on KDE Plasma 6 / KWin / Wayland. It uses Tesseract OCR, or optional PaddleOCR, and can translate through Google, Yandex, or a configured compatible service. You can define up to three independent capture regions and display translations in a movable window or over the original text. The latter mode needs KWin window geometry; Portal capture falls back to the translation window.

On Arch Linux, build and install the package with:

```bash
git clone https://github.com/GrayRats/lipax-qt.git
cd lipax-qt
./packaging/build-local.sh
sudo pacman -U dist/lipax-1.0.2-1-x86_64.pkg.tar.zst
```

Start `lipax`, choose a game window and text region, configure OCR languages and a translation service, then press “Запустить” (Start). The settings include appearance, regions, shortcuts, and dependency status. The translation window can be pinned or moved; a long translation can be scrolled with the mouse wheel. History storage can be disabled.

**Known limitation:** a changed setting may be saved without taking effect immediately, for this reason, there may be a translation error 429 (timeout). If that happens, close LipaX completely and start it again.
Check overlay placement with your particular fullscreen game or mixed-scale monitor setup.

To test a local build against KWin screen capture, start it with `packaging/run-local.sh` (or
`LIPAX_LOCAL_BINARY=/full/path/to/lipax packaging/run-local.sh` for a binary you already built; `--unregister` removes
the registration). KWin allows `ScreenShot2` only to the executable path named in a desktop file: the package
allows `/usr/bin/lipax`, not `target/debug/lipax`, which fails with `ScreenShot2.Error.NoAuthorized`. The script
registers a separate hidden desktop entry for the local path and leaves the installed launcher alone. Close any
other LipaX first: a second start only forwards its command. In “Авто” mode a refusal makes LipaX choose the
window through the portal; the “Статус” tab shows whether the current binary is allowed to capture.

Run `RUST_LOG=debug lipax` to see more diagnostic output, or use `2>&1 | tee lipax.log` to save it. See [logging](docs/LOGGING.md), [architecture](docs/ARCHITECTURE.md), and [PaddleOCR setup](docs/PaddleOCR.md).

