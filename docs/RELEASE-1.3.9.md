# LipaX 1.3.9 — release notes

## Highlights

- **In-place translation** (`translation_display = "inplace"`) is more reliable on complex
  game screens: more text blocks are detected (limit raised from 16 to 64, configurable),
  low-contrast HUD text is found, and halftone dots are no longer read as a "0".
- **Honest placement outcomes.** A field is reported as *replaced*, *partial*, *degraded fallback*
  (a label beside the original) or *unrenderable*. A label beside the original is not counted
  as a replacement.
- **Main window redesign.** Compact top bar with icons, two checkable display switches,
  an adaptive capture-area row, a status light, an English status line, and text areas
  that never resize the window.
- **Remembered window geometry** (setting, on by default). On Wayland only the size is restored.
- **Bundled Noto Serif CJK SC** for serif-looking Japanese, Chinese and Korean translations.
- The OCR preview in *Settings* was removed.

## Known limitations

- Live overlay in KWin/Wayland and XWayland, multi-monitor and fractional scaling have not been
  verified in a live session. Verification in this release is offline and on recorded frames.
- Position restoration depends on the compositor; Wayland does not allow exact placement.
- The status line shows English messages only; unrecognised internal messages are shown
  generically and the details stay in the log.

## Upgrade

No configuration migration is needed. New settings have defaults:
`general.remember_window_geometry = true`, `appearance.inplace.max_blocks = 64`.

## Corrective package 1.3.9-2

The sections above describe the original release. The corrective package keeps
the application version and increments the Arch package revision.

- Floating and pinned surfaces remain separate. Floating movement uses Qt's
  compositor-managed move/resize operations; translation updates do not bind the
  outer window to text size or repeatedly force desktop coordinates.
- KWin geometry scripts start through the scripting manager and must report
  readiness. Failed geometry initialization is diagnosed instead of publishing
  an empty inplace viewport.
- Placement reconstructs complete locally detected lines before deduplication,
  keeps name and dialogue fields separate, and preserves real OCR crop bounds.
  Split fields are rescanned before translation and keep independent identities.
- Rust fitting and QML rendering share the measured shaping width. Fields that
  cannot display text have an explicit rejection reason.
- Automatic subtitle backgrounds obscure the original text instead of selecting
  transparency. Explicit transparent backgrounds remain available.
- Permanent game examples use actual RapidOCR and local Bergamot translations.
  The former generic stress sentence is confined to explicit stress tests.

Controlled compositor scenes, recorded frames, and offscreen rendering cover
different parts of the implementation. None proves every game's fonts or
fullscreen behavior. Physical floating dragging, multiple monitor arrangements,
and fractional scaling require further compositor checks. Exact Wayland position
restoration is attempted only through working KWin integration; a rejected
request leaves native compositor placement in control.

Current test procedures and remaining visual limitations are documented in
`tests/inplace/README.md`; test evidence is retained under `tests/overlay/`.
The attached `lipax-1.3.9-2.tar.gz` is the source snapshot for this corrective
package. GitHub's automatically generated archives for the original `v1.3.9`
tag describe the earlier release.

---

# LipaX 1.3.9 — заметки о выпуске

## Главное

- **Перевод поверх исходного текста** (`translation_display = "inplace"`) стал надёжнее на сложных
  игровых экранах: распознаётся больше блоков (лимит поднят с 16 до 64, настраивается),
  находится слабо контрастный текст HUD, полутоновые точки больше не читаются как «0».
- **Честный итог размещения.** Поле считается *заменённым*, *частично заменённым*,
  *упрощённым* (подпись рядом с оригиналом) или *не показанным*. Подпись рядом с оригиналом
  не считается заменой.
- **Переработка главного окна.** Компактная верхняя панель с иконками, два переключаемых
  режима отображения, адаптивная строка области захвата, индикатор состояния, нижняя строка
  на английском; текстовые области не растягивают окно.
- **Запоминание геометрии окна** (настройка, включена по умолчанию). На Wayland восстанавливается только размер.
- **Встроенный Noto Serif CJK SC** для японского, китайского и корейского перевода с засечками.
- Предпросмотр распознавания в настройках удалён.

## Известные ограничения

- Живой overlay в KWin/Wayland и XWayland, несколько мониторов и дробное масштабирование в живой
  сессии не проверялись. Проверка в этом релизе офлайн и на записанных кадрах.
- Восстановление положения зависит от композитора; Wayland не позволяет задать точное положение.
- Нижняя строка показывает только английские сообщения; неизвестные внутренние сообщения выводятся
  общим текстом, подробности — в журнале.

## Обновление

Миграция конфигурации не требуется. Новые настройки имеют значения по умолчанию:
`general.remember_window_geometry = true`, `appearance.inplace.max_blocks = 64`.

## Исправленный пакет 1.3.9-2

Разделы выше описывают первоначальный выпуск. Исправленный пакет сохраняет
версию приложения и повышает ревизию пакета Arch.

- Floating и pinned остаются отдельными поверхностями. Перемещение и изменение
  размера floating выполняет композитор через Qt; обновление перевода не
  привязывает размер внешнего окна к тексту и не навязывает координаты рабочего стола.
- Скрипты геометрии KWin запускаются через менеджер скриптов и подтверждают
  готовность. Ошибка инициализации диагностируется вместо пустого viewport inplace.
- Локально найденные строки собираются целиком перед удалением дубликатов;
  имя и реплика сохраняются отдельными полями. Учитываются реальные границы
  OCR-кропа, а разделённые поля повторно распознаются перед переводом.
- Расчёт Rust и отрисовка QML используют согласованную ширину текста.
  Поля, которые нельзя показать, получают явную причину отклонения.
- Автоматический фон субтитров скрывает исходный текст вместо выбора
  прозрачности. Ручной прозрачный режим остаётся доступным.
- Постоянные игровые примеры используют настоящий RapidOCR и локальный
  перевод Bergamot. Общая тестовая фраза остаётся только в явных стресс-тестах.

Контролируемая сцена композитора, записанные кадры и офлайн-отрисовка проверяют
разные части реализации. Они не доказывают корректность каждого игрового шрифта
и fullscreen-режима. Физическое перетаскивание floating, различные расположения
мониторов и дробное масштабирование требуют дополнительных проверок.
Точное восстановление позиции Wayland выполняется только через работающую
интеграцию KWin; при отклонении запроса расположение определяет композитор.

Актуальные процедуры проверки и оставшиеся визуальные ограничения описаны
в `tests/inplace/README.md`; доказательства сохранены в `tests/overlay/`.
Прикреплённый `lipax-1.3.9-2.tar.gz` содержит исходники исправленного пакета.
Автоматические архивы GitHub для первоначального тега `v1.3.9` относятся
к предыдущему выпуску.
