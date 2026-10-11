.pragma library

// The bottom status line of the main window shows English text only. The Controller still produces its status
// in Russian (settings and logs keep their language); this maps it to a short English message. Technical details
// stay in the log and in the line's tooltip (the original text), they are never invented here.
const exact = {
    "Выберите окно игры": "Select a game window",
    "Выберите окно и включите область": "Select a window and enable a region",
    "Выберите окно и область": "Select a window and a region",
    "Ожидание текста": "Waiting for text",
    "Распознавание окна…": "Scanning text…",
    "Перевод…": "Translating…",
    "Перевод обновлён": "Translation updated",
    "Остановлено": "Stopped",
    "Подбор фильтров…": "Tuning filters…",
    "Подбор фильтров: текст не найден ни с одним набором": "Filter tuning: no text found",
    "Область захвата сохранена": "Capture region saved",
    "Область захвата сброшена": "Capture region cleared",
    "Сначала выберите область": "Select a capture region first",
    "Сначала выберите окно": "Select a game window first",
    "Кликните по окну игры (Esc — отмена)": "Click the game window (Esc to cancel)",
    "Выбор окна для захвата отменён": "Window selection cancelled",
    "Шрифты полей будут определены заново": "Field fonts will be detected again",
    "Кадр изменился, распознавание отменено": "Frame changed, scan cancelled",
    "OCR: текст не обнаружен": "OCR: no text found",
    "Установка этого пакета не поддерживается": "Installing this package is not supported"
}

// [Russian prefix, English text, optional: keep the rest of the message after the prefix when it is ASCII]
const prefixes = [
    ["Фильтры подобраны:", "Filters selected"],
    ["Подбор фильтров: ошибка захвата", "Error: capture failed while tuning filters"],
    ["OCR: низкая уверенность", "OCR: low confidence, text skipped"],
    ["Загрузка языковой модели", "Downloading language model"],
    ["Не удалось применить", "Error: could not apply settings"],
    ["Не удалось установить", "Error: could not install the package"],
    ["Настройки не сохранены", "Error: settings were not saved"],
    ["История не сохранена", "Error: history was not saved"],
    ["Сочетание для", "Error: hotkey is taken or invalid"],
    ["Ошибка:", "Error"]
]

function hasCyrillic(text) { return /[Ѐ-ӿ]/.test(text) }

// `kind` is the Controller's statusKind: "info", "warning" or "error".
function english(status, kind) {
    const text = (status || "").trim()
    if (text.length === 0) return "Ready"
    if (exact[text] !== undefined) return exact[text]
    for (const [prefix, message] of prefixes) {
        if (!text.startsWith(prefix)) continue
        const rest = text.slice(prefix.length).replace(/^[\s:(]+|[)\s]+$/g, "").replace(/…$/, "")
        // A language code or model name after the prefix is ASCII and worth keeping; Russian details are not.
        const detail = rest.length > 0 && !hasCyrillic(rest) ? ": " + rest : ""
        const dots = prefix === "Загрузка языковой модели" ? "…" : ""
        return message + detail + dots
    }
    if (!hasCyrillic(text)) return (kind === "error" && !/^error/i.test(text) ? "Error: " : "") + text
    if (kind === "error") return "Error: see the log for details"
    if (kind === "warning") return "Warning: see the log for details"
    return "Status updated"
}
