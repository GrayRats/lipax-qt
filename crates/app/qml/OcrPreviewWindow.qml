import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// What the last OCR run saw: the frame of the region, the blocks found in it and their texts,
// before the translation is drawn anywhere. The Controller sends frames only while this window is open.
ApplicationWindow {
    id: win
    objectName: "ocrPreviewWindow"
    property var controller
    property int regionIndex: 0
    readonly property var regions: { try { return JSON.parse(controller.ocrPreviewJson || "[]") } catch (e) { return [] } }
    readonly property var region: regions.length > 0 ? regions[Math.min(regionIndex, regions.length - 1)] : null
    readonly property var boxes: region ? region.boxes : []

    title: "Просмотр OCR — LipaX"
    width: 860; height: 640
    minimumWidth: 560; minimumHeight: 420
    visible: false
    function stageName(stage) {
        return ({capture: "захват", detect: "детектор", ocr: "OCR", layout: "раскладка", translate: "перевод"})[stage] || stage
    }
    function openWindow() { show(); raise(); requestActivate() }
    // Frames are copied and encoded only while the window is visible.
    onVisibleChanged: controller.setOcrPreviewEnabled(visible)

    header: TabBar {
        id: tabs
        visible: win.regions.length > 1
        height: visible ? implicitHeight : 0
        currentIndex: Math.min(win.regionIndex, Math.max(0, win.regions.length - 1))
        onCurrentIndexChanged: win.regionIndex = currentIndex
        Repeater {
            model: win.regions
            TabButton { text: modelData.name; width: implicitWidth }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 10

        Label {
            objectName: "ocrPreviewEmpty"
            Layout.fillWidth: true
            visible: !win.region
            wrapMode: Text.Wrap
            opacity: 0.75
            text: "Кадра пока нет. Запустите слежение или нажмите «Распознать сейчас»: здесь появится кадр области, найденные блоки текста и результат распознавания."
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 120
            visible: !!win.region
            color: "#101010"
            border.color: palette.mid
            clip: true
            Image {
                id: shot
                objectName: "ocrPreviewImage"
                anchors.fill: parent
                anchors.margins: 4
                fillMode: Image.PreserveAspectFit
                cache: false
                smooth: false
                source: win.region ? win.region.image : ""
                // Frame pixel -> window pixel.
                readonly property real scale: win.region && win.region.width > 0 ? paintedWidth / win.region.width : 1
                readonly property real offsetX: (width - paintedWidth) / 2
                readonly property real offsetY: (height - paintedHeight) / 2
                Repeater {
                    model: win.boxes
                    delegate: Rectangle {
                        objectName: "ocrBox" + index
                        required property var modelData
                        required property int index
                        x: shot.offsetX + modelData.x * shot.scale
                        y: shot.offsetY + modelData.y * shot.scale
                        width: Math.max(2, modelData.w * shot.scale)
                        height: Math.max(2, modelData.h * shot.scale)
                        color: "#1f00c8b4"
                        border.color: "#00c8b4"
                        border.width: 2
                        Label {
                            anchors.left: parent.left
                            anchors.bottom: parent.top
                            text: String(parent.index + 1)
                            font.pixelSize: 11
                            font.bold: true
                            color: "#00c8b4"
                        }
                    }
                }
            }
        }

        Label {
            visible: !!win.region
            text: win.region ? "Кадр " + win.region.width + "×" + win.region.height + " px · блоков: " + win.boxes.length : ""
            opacity: 0.7
            font.pixelSize: 12
        }
        Label {
            objectName: "ocrPreviewTimings"
            visible: !!win.region
            wrapMode: Text.Wrap
            font.pixelSize: 12
            opacity: 0.8
            text: !win.region ? "" : "Состояние области: " + win.region.phase + "\n" + (win.region.timings.length === 0 ? "Времена этапов пока не измерены"
                : "Этапы (последний / p50 / p95, мс): " + win.region.timings.map(t => win.stageName(t.stage) + " "
                    + t.last_ms.toFixed(0) + " / " + t.p50_ms.toFixed(0) + " / " + t.p95_ms.toFixed(0)).join(" · "))
        }
        Label { visible: !!win.region; text: "Распознанный текст и перевод"; font.bold: true }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 170
            visible: !!win.region
            ListView {
                id: blockList
                objectName: "ocrBlockList"
                clip: true
                spacing: 6
                model: win.boxes
                delegate: Label {
                    required property var modelData
                    required property int index
                    width: ListView.view.width - 12
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: (index + 1) + ".  " + modelData.original + (modelData.translation ? "\n      →  " + modelData.translation : "")
                          + (modelData.details && modelData.details.length > 0 ? "\n" + modelData.details.map(d => "      · " + d).join("\n") : "")
                }
                Label {
                    anchors.centerIn: parent
                    visible: blockList.count === 0
                    opacity: 0.7
                    text: "Текст в кадре не найден"
                }
            }
        }
        // The translation window mode translates the region as a whole: its boxes are the lines
        // the engine found and carry no translation of their own.
        Label {
            objectName: "ocrWholeTranslation"
            Layout.fillWidth: true
            visible: !!win.region && win.region.translation.length > 0 && win.boxes.every(b => !b.translation)
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: win.region ? "Перевод области:  " + win.region.translation : ""
        }
        RowLayout {
            Button {
                text: "Распознать сейчас"
                objectName: "ocrPreviewRefresh"
                onClicked: win.controller.translateOnce()
            }
            Button {
                text: "Копировать оригинал"
                id: copyRecognizedText
                enabled: !!win.region && win.region.original.length > 0
                Accessible.description: copyRecognizedTextHint.explanation
                UnavailableHint { id: copyRecognizedTextHint; control: copyRecognizedText; feature: "Копировать распознанный текст"; reason: "Нет распознанного текста выбранной области."; remedy: "Выберите область с результатом распознавания." }
                onClicked: win.controller.copyText(win.region.original)
            }
            Item { Layout.fillWidth: true }
        }
    }
}
