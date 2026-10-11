import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The original and the translation side by side (or stacked). Size comes only from the space the parent gives:
// every preferred size is 0, so the implicit size of a long text cannot raise the layout or the window; the text
// wraps inside its block (a word with no spaces breaks anywhere) and a taller text scrolls.
GridLayout {
    id: panels
    property int gap: 8
    property real radius: 6
    property font descriptionFont
    property string originalText: ""
    property int originalFormat: TextEdit.PlainText
    property string translationText: ""
    property int translationFormat: TextEdit.PlainText
    property color textColor: "#e8e8e8"
    property color panelColor: "#2b2b2b"
    property string fontFamily: "Inter"
    property int fontSize: 16
    columnSpacing: gap
    rowSpacing: gap
    ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: 1
        Layout.preferredHeight: 0
        Layout.minimumWidth: 0
        Label { text: "Оригинал"; font: panels.descriptionFont }
        ScrollView {
            id: originalScroll
            objectName: "originalScroll"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 40
            Layout.minimumWidth: 0
            Layout.preferredWidth: 0
            Layout.preferredHeight: 0
            clip: true
            TextArea {
                objectName: "mainOriginalText"
                text: panels.originalText
                textFormat: panels.originalFormat
                placeholderText: "Текст появится после распознавания"
                readOnly: true
                wrapMode: Text.Wrap
                color: panels.textColor
                font.family: panels.fontFamily
                font.pixelSize: panels.fontSize
                background: Rectangle { color: panels.panelColor; radius: panels.radius }
            }
        }
    }
    ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: 1
        Layout.preferredHeight: 0
        Layout.minimumWidth: 0
        Label { text: "Перевод"; font.bold: true }
        ScrollView {
            id: translationScroll
            objectName: "translationScroll"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 40
            Layout.minimumWidth: 0
            Layout.preferredWidth: 0
            Layout.preferredHeight: 0
            clip: true
            TextArea {
                objectName: "mainTranslationText"
                text: panels.translationText
                textFormat: panels.translationFormat
                placeholderText: "Здесь будет перевод"
                readOnly: true
                wrapMode: Text.Wrap
                color: panels.textColor
                font.family: panels.fontFamily
                font.pixelSize: panels.fontSize
                background: Rectangle { color: panels.panelColor; radius: panels.radius }
            }
        }
    }
}
