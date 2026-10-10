# LipaX-QT — icons for compact toolbar

Иконки соответствуют эскизу «Предлагаемый внешний вид»:

- `ocr-preview.svg` — предпросмотр распознавания (scan + eye)
- `translation-history.svg` — история переводов (clock + history arrow)
- `settings.svg` — настройки (gear)
- `select-window.svg` — выбрать окно (application window)
- `select-capture-area.svg` — выбрать область захвата (scan frame)
- `start-translation.svg` — запуск перевода (play)

Все SVG: 24 × 24, без растровых вложений, прозрачный фон, однотонный stroke #252831, stroke-width 1.8.
Файл `preview.png` показывает набор для визуальной проверки.

Для Qt/QML: SVG можно подключить в ресурсы приложения и использовать как `Image`/`icon.source`.
При необходимости для темной темы перекрасьте stroke/fill в светлый цвет или используйте штатное темозависимое окрашивание в QML.
