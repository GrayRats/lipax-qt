import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: preview
    required property var settings
    required property var controller
    property bool compact: false
    property color surfaceColor: palette.window
    property color foregroundColor: palette.windowText
    readonly property bool inplace: settings.display_mode === "inplace"
    padding: 10
    clip: true
    background: Rectangle { color: preview.surfaceColor; radius: 6; border.color: preview.palette.mid }
    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        Label {
            id: previewTitle
            color: preview.foregroundColor
            Layout.fillWidth: true
            text: "Предпросмотр · " + (preview.inplace ? "поверх текста" : "отдельное окно")
            font.bold: true
            wrapMode: Text.Wrap
        }
        Label {
            Layout.fillWidth: true
            text: "Пример, не результат OCR"
            color: preview.foregroundColor
            wrapMode: Text.Wrap
        }
        Rectangle {
            id: canvas
            objectName: "previewCanvas"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 60
            color: "#26313d"
            radius: 4
            clip: true
            // Neutral sample background: compositor blur and automatic analysis depend on the real game.
            TranslationWindowContent {
                objectName: "windowAppearancePreview"
                anchors.fill: parent
                anchors.margins: 6
                visible: !preview.inplace
                settings: preview.settings
                translation: "Путь свободен. Продолжим наше путешествие."
                original: "The path is clear. Let’s continue our journey."
                pinned: preview.settings.translation_window.mode === "pinned"
                previewOnly: true
            }
            readonly property var resolved: {
                if (!preview.inplace || !preview.visible || !preview.controller || typeof preview.controller.appearancePreview !== "function") return null
                try { return JSON.parse(preview.controller.appearancePreview(JSON.stringify(preview.settings.appearance.inplace), width, height)) }
                catch (e) { return null }
            }
            InplaceTranslationContent {
                objectName: "inplaceAppearancePreview"
                visible: preview.inplace && !!entry
                entry: canvas.resolved
                x: entry ? entry.box[0] * canvas.width : 0
                y: entry ? entry.box[1] * canvas.height : 0
                width: entry ? entry.box[2] * canvas.width : 0
                height: entry ? entry.box[3] * canvas.height : 0
            }
            Label {
                anchors.fill: parent
                anchors.margins: 8
                visible: preview.inplace && !canvas.resolved
                text: "Предпросмотр недоступен: не удалось подготовить пример. Настройки сохранены."
                color: "white"
                wrapMode: Text.Wrap
            }
        }
        Label {
            Layout.fillWidth: true
            visible: !preview.compact
            color: preview.foregroundColor
            wrapMode: Text.Wrap
            text: preview.inplace ? "Автоподбор показан на условном поле. В игре шрифт, фон и размер зависят от изображения."
                : "Размер примера условный. Размытие фона в игре выполняет KWin."
        }
    }
    HoverHint {
        control: previewTitle
        feature: "Живой пример оформления"
        explanation: "Обновляется при изменении оформления без захвата игры и обращения к переводчику. Геометрия примера не меняет область захвата. При полной прозрачности фон исчезает; прозрачность текста также учитывается."
    }
}
