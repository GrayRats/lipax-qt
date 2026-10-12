# LipaX-QT • UI/UX SVG Icon Pack

This package extends the previous six-icon set with new controls and state indicators.

## SVG file map

| File | Russian control | Intended action |
|---|---|---|
| settings.svg | Настройки | Open application settings |
| translation-history.svg | История переводов | Open history |
| ocr-preview.svg | Предпросмотр распознавания | Show OCR preview |
| select-window.svg | Выбрать окно | Select capture window |
| select-capture-area.svg | Выбрать область захвата | Select rectangular screen region |
| reset-capture-area.svg | Сбросить область | Clear current capture region |
| show-inplace-overlay.svg | Показать поверх исходного текста | Toggle in-place overlay visibility |
| show-translation-window.svg | Показать окно перевода | Toggle independent translation-window visibility |
| start-translation.svg | Запустить перевод | Start translation |
| stop-translation.svg | Остановить перевод | Stop translation |
| status-running.svg | Работает | Running indicator (green) |
| status-stopped.svg | Остановлен | Stopped indicator (grey) |
| status-processing.svg | Обработка | Processing indicator (amber) |
| status-error.svg | Ошибка | Error indicator (red) |

## Integration notes

- ViewBox is `0 0 24 24`, transparent canvas. Six previous icons are unchanged.
- Avoid using CSS `currentColor` in QtSvg. Base action glyphs use `#252831` and should be colored using Qt Quick Controls' `icon.color` or a suitable tinting provider. Test with QtSvg in the target Qt version.
- The four status SVG files use explicit semantic colors; do not tint them with the generic text color.
- Toggle icons must use an actual `checkable` + `checked` state. Neither a new checkmark nor a selected-looking SVG is needed.
- Smooth status animation and reduced-motion behavior must be implemented in QML, not through SVG animation.
- QML resources should be included using the project's existing CMake/qrc setup.
- Asset license: original illustrative icons in this package; no third-party icon font dependencies.
