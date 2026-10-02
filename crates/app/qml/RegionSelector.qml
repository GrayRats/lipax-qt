import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Выбор области на замороженном кадре выбранного окна. Кадр показывается в нашем
// окне: рисовать поверх чужого окна Wayland не позволяет, а координаты всё равно
// нормализуются относительно окна игры.
Window {
    id: win
    property var controller
    property real selX: 0
    property real selY: 0
    property real selW: 0
    property real selH: 0
    readonly property bool hasSel: selW > 4 && selH > 4

    title: "Выбор области перевода"
    width: 960
    height: 640
    visible: false
    color: "#202020"

    function begin() {
        selW = 0; selH = 0
        controller.requestPreview()
    }

    Connections {
        target: win.controller
        function onPreviewReady() { win.show(); win.requestActivate() }
    }

    Shortcut { sequence: "Escape"; onActivated: win.close() }
    Shortcut { sequence: "Return"; enabled: win.hasSel; onActivated: win.accept() }

    function accept() {
        controller.setRegion(selX / frame.width, selY / frame.height, selW / frame.width, selH / frame.height)
        close()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Image {
                id: img
                anchors.fill: parent
                anchors.margins: 8
                source: win.controller ? win.controller.previewSource : ""
                fillMode: Image.PreserveAspectFit
                cache: false
                asynchronous: true
            }

            // Рамка ровно по отрисованному кадру: в ней считаются доли окна.
            Item {
                id: frame
                x: img.x + (img.width - img.paintedWidth) / 2
                y: img.y + (img.height - img.paintedHeight) / 2
                width: img.paintedWidth
                height: img.paintedHeight

                // Тонкая обводка кадра окна, заливки нет.
                Rectangle { anchors.fill: parent; color: "transparent"; border.width: 1; border.color: "#80ffffff" }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.CrossCursor
                    property real ox
                    property real oy
                    onPressed: (m) => { ox = m.x; oy = m.y; win.selX = m.x; win.selY = m.y; win.selW = 0; win.selH = 0 }
                    onPositionChanged: (m) => {
                        const x = Math.max(0, Math.min(frame.width, m.x))
                        const y = Math.max(0, Math.min(frame.height, m.y))
                        win.selX = Math.min(ox, x); win.selY = Math.min(oy, y)
                        win.selW = Math.abs(x - ox); win.selH = Math.abs(y - oy)
                    }
                }

                // Выделение: тонкая рамка и почти прозрачная заливка.
                Rectangle {
                    visible: win.hasSel
                    x: win.selX; y: win.selY; width: win.selW; height: win.selH
                    color: "#1a00aaff"
                    border.width: 1
                    border.color: "#cc00aaff"
                }
            }
        }

        RowLayout {
            Layout.margins: 8
            Label { Layout.fillWidth: true; text: "Выделите мышью область с текстом. Enter — сохранить, Esc — отмена." ; color: "white" }
            Button { text: "Сбросить"; onClicked: { win.selW = 0; win.selH = 0 } }
            Button { text: "Сохранить"; enabled: win.hasSel; highlighted: true; onClicked: win.accept() }
        }
    }
}
