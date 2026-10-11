import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtTest
import "../qml" as Lipa
import "../qml/StatusText.js" as StatusText
import "../qml/WindowGeometry.js" as Geo

// Pieces of the redesigned main window: English status text, geometry restoration, the text panels that must
// not stretch the window, the status light, the process title and the toggle buttons.
TestCase {
    id: test
    name: "MainWindowParts"
    when: windowShown
    visible: true
    width: 640
    height: 420

    // ── Status line: English only ──
    function test_statusMessagesAreEnglish() {
        compare(StatusText.english("Ожидание текста", "info"), "Waiting for text")
        compare(StatusText.english("Распознавание окна…", "info"), "Scanning text…")
        compare(StatusText.english("Перевод…", "info"), "Translating…")
        compare(StatusText.english("Перевод обновлён", "info"), "Translation updated")
        compare(StatusText.english("Область захвата сброшена", "info"), "Capture region cleared")
        compare(StatusText.english("Загрузка языковой модели en-ru…", "info"), "Downloading language model: en-ru…")
        compare(StatusText.english("", "info"), "Ready")
    }
    function test_unknownRussianMessagesNeverLeakIntoTheEnglishLine() {
        for (const text of ["Какая-то новая ошибка сети", "Ошибка: нет доступа к сервису перевода", "Настройки не сохранены: диск полон"]) {
            for (const kind of ["info", "warning", "error"])
                verify(!/[Ѐ-ӿ]/.test(StatusText.english(text, kind)), text + " / " + kind + " → " + StatusText.english(text, kind))
        }
        compare(StatusText.english("Что-то случилось", "error"), "Error: see the log for details")
        compare(StatusText.english("Что-то случилось", "warning"), "Warning: see the log for details")
        compare(StatusText.english("Ошибка: нет доступа", "error"), "Error")
    }
    function test_englishTextPassesThrough() {
        compare(StatusText.english("Ready", "info"), "Ready")
        compare(StatusText.english("connection reset", "error"), "Error: connection reset")
        compare(StatusText.english("Error: connection reset", "error"), "Error: connection reset")
    }

    // ── Remembered geometry ──
    readonly property var two: [
        { name: "DP-1", x: 0, y: 0, width: 2560, height: 1080, availableWidth: 2560, availableHeight: 1040, scale: 1 },
        { name: "HDMI-A-1", x: -1920, y: 0, width: 1920, height: 1080, availableWidth: 1920, availableHeight: 1040, scale: 1 }]
    function test_sameScreenKeepsSizeAndPosition() {
        const r = Geo.restore({ x: 300, y: 200, w: 700, h: 520, screen: "DP-1", screen_w: 2560, screen_h: 1080, scale: 1 }, two, 480, 400, true)
        compare(r.screen, "DP-1")
        compare([r.x, r.y, r.w, r.h], [300, 200, 700, 520])
        verify(!r.moved && r.position)
    }
    function test_negativeCoordinatesOfASecondMonitorAreKept() {
        const r = Geo.restore({ x: -1500, y: 100, w: 700, h: 520, screen: "HDMI-A-1", screen_w: 1920, screen_h: 1080, scale: 1 }, two, 480, 400, true)
        compare(r.screen, "HDMI-A-1")
        compare([r.x, r.y], [-1500, 100])
    }
    function test_aDisconnectedMonitorFallsBackToAnAvailableScreenAndCentres() {
        const r = Geo.restore({ x: -1500, y: 100, w: 700, h: 520, screen: "DP-9", screen_w: 1920, screen_h: 1080, scale: 1 }, [two[0]], 480, 400, true)
        compare(r.screen, "DP-1")
        verify(r.x >= 0 && r.x + r.w <= 2560 && r.y >= 0 && r.y + r.h <= 1040, JSON.stringify(r))
        verify(r.moved)
    }
    function test_aWindowOutsideTheWorkAreaIsPulledInside() {
        const r = Geo.restore({ x: 5000, y: 4000, w: 700, h: 520, screen: "DP-1", screen_w: 2560, screen_h: 1080, scale: 1 }, two, 480, 400, true)
        compare(r.screen, "DP-1")
        verify(r.x + r.w <= 2560 && r.y + r.h <= 1040 && r.x >= 0 && r.y >= 0, JSON.stringify(r))
    }
    function test_anOversizedWindowIsLimitedToTheAvailableArea() {
        const r = Geo.restore({ x: 0, y: 0, w: 9000, h: 9000, screen: "HDMI-A-1", screen_w: 1920, screen_h: 1080, scale: 1 }, two, 480, 400, true)
        compare([r.w, r.h], [1920, 1040])
    }
    function test_aChangedScaleOrResolutionDistrustsThePosition() {
        const rec = { x: 100, y: 100, w: 700, h: 520, screen: "DP-1", screen_w: 1920, screen_h: 1080, scale: 2 }
        const r = Geo.restore(rec, two, 480, 400, true)
        compare(r.screen, "DP-1")
        compare([r.x, r.y], [Math.round((2560 - 700) / 2), Math.round((1040 - 520) / 2)], "centred, not at the stale position")
    }
    function test_minimumSizeWinsOverASmallRecord() {
        const r = Geo.restore({ x: 10, y: 10, w: 100, h: 100, screen: "DP-1", screen_w: 2560, screen_h: 1080, scale: 1 }, two, 480, 400, true)
        compare([r.w, r.h], [480, 400])
    }
    function test_waylandRestoresSizeOnlyAndTheRecordIsRejectedWhenBroken() {
        const r = Geo.restore({ x: 300, y: 200, w: 700, h: 520, screen: "DP-1", screen_w: 2560, screen_h: 1080, scale: 1 }, two, 480, 400, false)
        verify(!r.position)
        compare([r.w, r.h], [700, 520])
        compare(Geo.restore(undefined, two, 480, 400, true), null)
        compare(Geo.restore({ x: NaN, y: 0, w: 10, h: 10, screen: "" }, two, 480, 400, true), null)
        compare(Geo.restore({ x: 0, y: 0, w: 700, h: 520, screen: "DP-1" }, [], 480, 400, true), null)
    }

    // ── Long text never changes the layout ──
    Item {
        id: host
        width: 640; height: 360
        ColumnLayout {
            id: column
            anchors.fill: parent
            Label { text: "controls"; Layout.preferredHeight: 30 }
            Lipa.TranslationPanels {
                id: panels
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumWidth: 0
                // No preferred size here on purpose: the component itself must keep its text out of the layout.
                columns: 2
            }
            Label { text: "status"; Layout.preferredHeight: 20 }
        }
    }
    function longText(kind) {
        if (kind === "words") return "перевод ".repeat(4000)
        if (kind === "noSpaces") return "A".repeat(6000)
        if (kind === "cjk") return "これは長い日本語の文章です。".repeat(800)
        if (kind === "lines") return Array.from({length: 600}, (_, i) => "строка " + i).join("\n")
        return "short"
    }
    function test_longTextDoesNotChangeTheLayoutOrTheImplicitSize() {
        panels.translationText = "short"
        waitForRendering(host)
        const base = { w: panels.width, h: panels.height, ih: column.implicitHeight, iw: column.implicitWidth, x: panels.x }
        for (const kind of ["words", "noSpaces", "cjk", "lines"]) {
            panels.translationText = longText(kind)
            panels.originalText = longText(kind)
            waitForRendering(host)
            compare(panels.width, base.w, kind + ": panel width")
            compare(panels.height, base.h, kind + ": panel height")
            compare(column.implicitHeight, base.ih, kind + ": the layout's implicit height")
            compare(column.implicitWidth, base.iw, kind + ": the layout's implicit width")
            const scroll = findChild(panels, "translationScroll")
            verify(scroll.contentHeight > scroll.height, kind + ": the content scrolls instead of growing")
            verify(scroll.width <= panels.width / 2, kind + ": the block stays in its half")
            const area = findChild(panels, "mainTranslationText")
            verify(area.width <= scroll.width + 1, kind + ": the text is wrapped to the block (" + area.width + " vs " + scroll.width + ")")
        }
        panels.translationText = ""
        panels.originalText = ""
    }
    function test_stackedLayoutAlsoKeepsItsSize() {
        panels.columns = 1
        panels.translationText = longText("noSpaces")
        waitForRendering(host)
        const h = panels.height
        panels.translationText = longText("words")
        waitForRendering(host)
        compare(panels.height, h)
        compare(panels.width, host.width)
        panels.columns = 2
        panels.translationText = ""
    }

    // ── Process title ──
    Item { id: titleHost; width: 400; height: 40
        Lipa.ProcessTitle { id: title; width: parent.width; text: "very-long-process-name-without-end.exe — Window Title"; baseFont: Qt.font({family: "Inter"}); basePixelSize: 16 } }
    function test_processTitleIsUnderlinedShrinksToEightPixelsThenElides() {
        title.width = 600
        compare(title.fittedPixelSize, 16)
        verify(!title.shrunk)
        title.width = 300
        verify(title.fittedPixelSize < 16 && title.fittedPixelSize >= 8, "shrunk to " + title.fittedPixelSize)
        verify(title.shrunk)
        title.width = 60
        compare(title.fittedPixelSize, 8)
        waitForRendering(titleHost)
        verify(title.elided, "still too long at 8 px: elided")
        verify(title.fittedFont.underline)
        compare(title.text, "very-long-process-name-without-end.exe — Window Title", "the name itself is never modified")
        compare(title.implicitWidth, 0, "never asks the parent for more width")
        verify(!title.text.includes("_") || title.text === "very-long-process-name-without-end.exe — Window Title")
        title.width = 400
    }

    // ── Status light ──
    Item { id: lightHost; width: 40; height: 40
        Lipa.StatusIndicator { id: light; state: "stopped" } }
    function test_statusLightStatesAndAccessibleNames() {
        const expected = { running: "Работает", stopped: "Остановлен", processing: "Обработка", error: "Ошибка" }
        for (const state in expected) {
            light.state = state
            compare(light.Accessible.name, expected[state])
            verify(light.Accessible.description.length > 0)
        }
    }
    function test_statusLightPulsesGentlyOnlyWhileRunningOrProcessing() {
        light.reducedMotion = false
        light.state = "stopped"
        wait(120)
        compare(light.pulse, 0, "stopped is static")
        light.state = "error"
        wait(120)
        compare(light.pulse, 0, "error is static")
        light.state = "running"
        tryVerify(() => light.pulse > 0.2, 2000)
        light.state = "processing"
        tryVerify(() => light.pulse > 0.2, 2000)
        light.reducedMotion = true
        tryCompare(light, "pulse", 0, 2000)
        wait(150)
        compare(light.pulse, 0, "no motion when reduced motion is requested")
        light.reducedMotion = false
        light.state = "stopped"
    }

    // ── Toggle buttons ──
    Lipa.ActionButton { id: toggle; x: 10; y: 10; iconFile: "show-inplace-overlay.svg"; checkable: true; text: "Показывать поверх исходного текста"; helpText: "Переключатель" }
    function test_toggleIsACheckableButtonWithItsStateInAccessibility() {
        verify(toggle.checkable)
        verify(!toggle.checked)
        compare(toggle.Accessible.checkable, true)
        compare(toggle.Accessible.checked, false)
        toggle.forceActiveFocus()
        keyClick(Qt.Key_Space)
        verify(toggle.checked)
        compare(toggle.Accessible.checked, true)
        mouseClick(toggle)
        verify(!toggle.checked)
        compare(toggle.display, AbstractButton.IconOnly, "an SVG, never a check mark or a character")
        verify(toggle.icon.source.toString().endsWith("show-inplace-overlay.svg"))
        toggle.checked = true
        const hint = toggle.Window.window.contentItem.children.find(item => item.control === toggle)
        verify(hint.explanation.includes("включено"))
        toggle.checked = false
        verify(hint.explanation.includes("выключено"))
    }
    function test_disabledIconButtonStaysVisibleButDimmedAndDoesNotToggle() {
        toggle.enabled = false
        verify(toggle.opacity < 1)
        mouseClick(toggle)
        verify(!toggle.checked)
        toggle.enabled = true
        compare(toggle.opacity, 1)
    }
}
