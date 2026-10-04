# LipaX

LipaX помогает читать текст в играх: захватывает выбранное окно, распознаёт надписи и показывает перевод. Приложение рассчитано на KDE Plasma 6 с KWin и Wayland; интерфейс написан на Qt 6/QML, обработка кадров — на Rust.

[English](#english)

![Главное окно LipaX](docs/screenshots/main.png)
![Настройки LipaX — внешний вид перевода](docs/screenshots/settings.png)

## Что умеет

- Распознаёт текст через Tesseract; при желании можно подключить PaddleOCR. Переводит через Google, Yandex или настраиваемый совместимый сервис. Доступность перевода зависит от выбранного сервиса и сети.
- Позволяет задать до трёх областей: например, отдельно для субтитров, диалогов и меню. Каждую область можно временно отключить и настроить отдельно.
- Показывает результат в отдельном окне поверх игры или накладывает переведённый текст поверх оригинала. Для второго режима нужна геометрия окна от KWin: при захвате через Portal приложение использует обычное окно перевода.
- Даёт настроить фон, рамку, прозрачность, скругление и шрифты из комплекта LipaX. Закреплённое окно оставляет доступ к игре; откреплённое можно двигать, видеть в панели задач и прокручивать колёсиком, если текст длинный.
- Сохраняет историю реплик, если включено её сохранение. Во вкладке «Статус» показывает готовность OCR и компонентов захвата; ошибки и диагностические сообщения доступны в терминале.

## Установка на Arch Linux

Понадобятся инструменты сборки Rust и зависимости из `PKGBUILD`. Скрипт собирает пакет из текущих файлов проекта:

```bash
git clone https://github.com/GrayRats/lipax-qt.git
cd lipax-qt
./packaging/build-local.sh
sudo pacman -U dist/lipax-1.0.1-1-x86_64.pkg.tar.zst
```

Для сборки из клонированного репозитория также подходит `makepkg -si`. Пакет устанавливает команду `lipax`, ярлык приложения и встроенные шрифты. Для распознавания нужного языка установите соответствующий пакет данных Tesseract; [настройка PaddleOCR](docs/PaddleOCR.md) описана отдельно.

## Первый запуск

1. Запустите `lipax` из меню приложений или терминала. В настройках проверьте язык распознавания, язык перевода и сервис перевода.
2. Нажмите «Выбрать окно», затем «Выбрать область» и обведите текст в игре. Вкладка «Область» позволяет добавить другие зоны и выбрать активную.
3. Выберите вид перевода в настройках и нажмите «Запустить». Перевод появится при обнаружении текста. При ошибке используйте кнопку «Повторить» и проверьте вкладку «Статус».

В режиме окна средняя кнопка мыши на Overlay переключает закрепление. В режиме наложения поверх оригинала средняя кнопка скрывает перевод; вернуть его можно переключателем в главном окне.

**Известные ограничения.** Иногда изменение настройки сохраняется, но не применяется к уже открытому окну или захвату. Если это произошло, полностью закройте LipaX и запустите его снова. Наложение поверх оригинала недоступно при Portal-захвате, поскольку Portal не сообщает положение выбранного окна. Отображение поверх конкретной полноэкранной игры и перенос между мониторами с разным масштабом стоит проверять в своей конфигурации.

## Диагностика и документация

При запуске из терминала LipaX пишет ошибки и диагностические сообщения в консоль. Более подробный журнал включается так:

```bash
RUST_LOG=debug lipax
RUST_LOG=debug lipax 2>&1 | tee lipax.log
```

По умолчанию включён уровень `INFO`; `ERROR` идёт в stderr, остальные сообщения — в stdout. Подробнее: [журналирование и уровни](docs/LOGGING.md), [архитектура проекта](docs/ARCHITECTURE.md), [доработки и ограничения](docs/IMPROVEMENTS.md).

## English

LipaX translates text captured from a game window on KDE Plasma 6 / KWin / Wayland. It uses Tesseract OCR, or optional PaddleOCR, and can translate through Google, Yandex, or a configured compatible service. You can define up to three independent capture regions and display translations in a movable window or over the original text. The latter mode needs KWin window geometry; Portal capture falls back to the translation window.

On Arch Linux, build and install the package with:

```bash
git clone https://github.com/GrayRats/lipax-qt.git
cd lipax-qt
./packaging/build-local.sh
sudo pacman -U dist/lipax-1.0.1-1-x86_64.pkg.tar.zst
```

Start `lipax`, choose a game window and text region, configure OCR languages and a translation service, then press “Запустить” (Start). The settings include appearance, regions, shortcuts, and dependency status. The translation window can be pinned or moved; a long translation can be scrolled with the mouse wheel. History storage can be disabled.

**Known limitation:** a changed setting may be saved without taking effect immediately. If that happens, close LipaX completely and start it again. Check overlay placement with your particular fullscreen game or mixed-scale monitor setup.

Run `RUST_LOG=debug lipax` to see more diagnostic output, or use `2>&1 | tee lipax.log` to save it. See [logging](docs/LOGGING.md), [architecture](docs/ARCHITECTURE.md), and [PaddleOCR setup](docs/PaddleOCR.md).
