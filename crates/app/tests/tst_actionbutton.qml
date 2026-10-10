import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Lipa

TestCase {
    id: test
    name: "SvgActions"
    when: windowShown
    visible: true
    width: 400
    height: 240
    Lipa.ActionButton {
        id: button
        x: 40; y: 40
        text: "Предпросмотр распознавания"
        helpText: "Показывает кадр, блоки и распознанный текст."
        iconFile: "ocr-preview.svg"
    }
    SignalSpy { id: clickSpy; target: button; signalName: "clicked" }
    function test_translationIndicatorFollowsRuntimeState() {
        button.activity = "stopped"
        compare(button.icon.color, button.stoppedColor)
        compare(button.pulse, 0)
        button.activity = "running"
        tryVerify(() => button.pulse > 0.1, 2000)
        button.activity = "stopped"
        tryCompare(button, "pulse", 0)
        compare(button.icon.color, button.stoppedColor)
        button.activity = ""
    }
    function test_svgActionsAreAccessibleAndDoNotInterceptClicks() {
        for (const file of ["ocr-preview.svg", "translation-history.svg", "settings.svg", "select-window.svg", "select-capture-area.svg", "start-translation.svg"]) {
            button.iconFile = file
            compare(button.display, AbstractButton.IconOnly)
            verify(button.icon.source.toString().endsWith(file))
            verify(button.width >= 38 && button.height >= 38)
        }
        compare(button.Accessible.name, button.text)
        clickSpy.clear()
        mouseClick(button, button.width / 2, button.height / 2)
        compare(clickSpy.count, 1)
        button.forceActiveFocus()
        keyClick(Qt.Key_Space)
        compare(clickSpy.count, 2)
        const hint = button.Window.window.contentItem.children.find(item => item.control === button)
        verify(hint !== undefined)
        mouseMove(button, button.width / 2, button.height / 2)
        tryVerify(() => hint.popupVisible, 2000)
        verify(hint.explanation.includes(button.text))
        verify(hint.explanation.includes("кадр"))
    }
}
