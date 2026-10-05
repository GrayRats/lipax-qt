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
    // A title bar the window draws itself (GTK/libadwaita): it is part of the frame, and the text
    // region belongs below it. The height is a hint found from the picture, never a crop.
    readonly property real barPx: win.controller && win.controller.previewTitleBar ? win.controller.previewTitleBar : 0
    // Pixels of the frame -> pixels of the painted picture.
    readonly property real pxScale: img.implicitHeight > 0 ? img.paintedHeight / img.implicitHeight : 1
    readonly property real barHeight: barPx * pxScale
    readonly property bool overlapsBar: barPx > 0 && hasSel && selY < barHeight - 1
    function belowTitleBar() {
        const bottom = selY + selH
        selY = barHeight
        selH = Math.max(0, bottom - barHeight)
    }

    title: "Выбрать область захвата"
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

                // Заголовок, нарисованный самим окном: область ниже него.
                Rectangle {
                    objectName: "titleBarHint"
                    visible: win.barPx > 0
                    x: 0; y: 0; width: parent.width; height: win.barHeight
                    color: "#55ffb300"
                    border.width: 1
                    border.color: "#ffffb300"
                    Label {
                        anchors.centerIn: parent
                        visible: parent.height > 14
                        text: "заголовок окна"
                        font.pixelSize: 11
                        color: "#ffe9a8"
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

        Label {
            objectName: "titleBarNote"
            Layout.fillWidth: true
            Layout.leftMargin: 8; Layout.rightMargin: 8; Layout.topMargin: 6
            visible: win.barPx > 0
            wrapMode: Text.Wrap
            color: win.overlapsBar ? "#ff8a80" : "#ffd54f"
            text: win.overlapsBar
                ? "Выделение захватывает собственный заголовок окна (≈" + win.barPx + " px сверху): его текст и кнопки будут читаться как текст игры."
                : "Окно рисует собственный заголовок (≈" + win.barPx + " px сверху, подсвечен): выделяйте область ниже него."
        }
        RowLayout {
            Layout.margins: 8
            Label { Layout.fillWidth: true; text: "Выделите мышью область с текстом. Enter — сохранить, Esc — отмена." ; color: "white" }
            Button { objectName: "belowTitleBar"; text: "Начать ниже заголовка"; visible: win.overlapsBar; onClicked: win.belowTitleBar() }
            Button { text: "Сбросить область захвата"; onClicked: { win.selW = 0; win.selH = 0 } }
            Button { text: "Сохранить"; enabled: win.hasSel; id: saveCaptureRegion; Accessible.description: saveCaptureRegionHint.explanation; UnavailableHint { id: saveCaptureRegionHint; control: saveCaptureRegion; feature: "Сохранить область захвата"; reason: "Область захвата ещё не выделена."; remedy: "Выделите мышью область с текстом." } highlighted: true; onClicked: win.accept() }
        }
    }
}
