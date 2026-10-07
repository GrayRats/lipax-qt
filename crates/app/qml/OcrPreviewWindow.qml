import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Controls.Universal

// What the last OCR run saw: the frame of the region, the blocks found in it and their texts,
// before the translation is drawn anywhere. The Controller sends frames only while this window is open.
ApplicationWindow {
    id: win
    objectName: "ocrPreviewWindow"
    property var controller
    // Dark or light, as in the settings (the Universal style switches at once).
    property int universalTheme: Universal.Dark
    Universal.theme: universalTheme
    property int regionIndex: 0
    readonly property var regions: { try { return JSON.parse(controller.ocrPreviewJson || "[]") } catch (e) { return [] } }
    readonly property var region: regions.length > 0 ? regions[Math.min(regionIndex, regions.length - 1)] : null
    // Older or partial entries (no filters, no threshold) still work.
    readonly property var filters: region && region.filters ? region.filters : ({})
    readonly property int minimumConfidence: region && region.minimum_confidence !== undefined ? region.minimum_confidence : 30
    readonly property var boxes: region ? region.boxes : []

    title: "Просмотр OCR — LipaX"
    width: 860; height: 640
    minimumWidth: 560; minimumHeight: 420
    visible: false
    function stageName(stage) {
        return ({capture: "захват", detect: "детектор", ocr: "OCR", layout: "раскладка", translate: "перевод", autotune: "автоподбор"})[stage] || stage
    }
    // Raw frame or the frame after the filters of the recognition settings.
    property bool showFiltered: false
    readonly property bool filteredShown: showFiltered && !!region && !!region.filtered_image
    // Colour of a box by how sure the engine was: green, orange (close to the threshold), red (below it).
    function confidenceColor(confidence, minimum) {
        if (confidence === null || confidence === undefined) return "#00c8b4"
        if (confidence < minimum) return "#e5484d"
        return confidence < Math.max(minimum + 20, 60) ? "#f5a623" : "#3fb950"
    }
    function setRecognition(key, value) {
        const patch = {}
        patch["recognition." + key] = value
        controller.applySettingsPatch(JSON.stringify(patch))
    }
    // Result of the last auto-tune: the presets with their scores and the one that was applied.
    readonly property var tuned: { try { return JSON.parse(controller.autotuneJson || "null") } catch (e) { return null } }
    property bool tuning: false
    onTunedChanged: tuning = false
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
            text: "Кадра пока нет. Запустите слежение или нажмите «Распознать сейчас»: здесь появится кадр области захвата, найденные блоки текста и результат распознавания."
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
                source: !win.region ? "" : (win.filteredShown ? win.region.filtered_image : win.region.image)
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
                        readonly property color tone: win.confidenceColor(modelData.confidence, win.minimumConfidence)
                        color: Qt.rgba(tone.r, tone.g, tone.b, 0.12)
                        border.color: tone
                        border.width: 2
                        HoverHandler { id: boxHover }
                        ToolTip.visible: boxHover.hovered
                        ToolTip.delay: 200
                        ToolTip.text: (modelData.confidence !== null && modelData.confidence !== undefined
                                      ? "Уверенность: " + Math.round(modelData.confidence) + "% (порог " + win.minimumConfidence + "%)\n"
                                      : "Уверенность не сообщается\n") + modelData.original
                        Label {
                            anchors.left: parent.left
                            anchors.bottom: parent.top
                            text: String(parent.index + 1)
                            font.pixelSize: 11
                            font.bold: true
                            color: parent.tone
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
            text: !win.region ? "" : "Состояние области захвата: " + win.region.phase + "\n" + (win.region.timings.length === 0 ? "Времена этапов пока не измерены"
                : "Этапы (последний / p50 / p95, мс): " + win.region.timings.map(t => win.stageName(t.stage) + " "
                    + t.last_ms.toFixed(0) + " / " + t.p50_ms.toFixed(0) + " / " + t.p95_ms.toFixed(0)).join(" · "))
        }
        // Try the filters here: a change goes to the settings and the next frame shows the result.
        Flow {
            Layout.fillWidth: true
            visible: !!win.region
            spacing: 10
            Switch {
                objectName: "ocrShowFiltered"
                text: "Кадр после фильтров"
                enabled: !!win.region && !!win.region.filtered_image
                checked: win.showFiltered
                onToggled: win.showFiltered = checked
            }
            CheckBox {
                objectName: "ocrFilterBinarize"
                text: "Бинаризация"
                checked: !!win.filters.binarize
                onToggled: win.setRecognition("binarize", checked)
            }
            CheckBox {
                objectName: "ocrFilterInvert"
                text: "Авто-инверсия"
                checked: !!win.filters.auto_invert
                onToggled: win.setRecognition("auto_invert", checked)
            }
            CheckBox {
                objectName: "ocrFilterSharpen"
                text: "Резкость"
                checked: !!win.filters.sharpen
                onToggled: win.setRecognition("sharpen", checked)
            }
            CheckBox {
                objectName: "ocrFilterNoise"
                text: "Убирать мусор"
                checked: !!win.filters.filter_noise
                onToggled: win.setRecognition("filter_noise", checked)
            }
            Button {
                objectName: "ocrAutoTune"
                text: win.tuning ? "Подбор…" : "Автоподбор"
                enabled: !!win.region && !win.tuning
                ToolTip.visible: hovered
                ToolTip.delay: 400
                ToolTip.text: "Прогоняет текущий кадр через несколько наборов фильтров, оценивает результат (средняя уверенность × число осмысленных строк) и сохраняет лучший набор в настройках игры."
                onClicked: { win.tuning = true; win.controller.autoTuneFilters() }
            }
            Label { text: "Контраст" }
            Slider {
                objectName: "ocrFilterContrast"
                from: -100; to: 100; stepSize: 10
                width: 140
                value: win.filters.contrast || 0
                onPressedChanged: if (!pressed) win.setRecognition("contrast", Math.round(value))
            }
            Label { text: "Порог, %" }
            Slider {
                objectName: "ocrFilterMinConfidence"
                from: 0; to: 95; stepSize: 5
                width: 140
                value: win.minimumConfidence
                onPressedChanged: if (!pressed) win.setRecognition("minimum_confidence", Math.round(value))
            }
        }
        Label {
            objectName: "ocrFiltersAuto"
            Layout.fillWidth: true
            visible: !!win.region && win.region.filters_auto === true
            wrapMode: Text.Wrap
            font.pixelSize: 12
            opacity: 0.85
            text: "Для этого кадра автоматически включены бинаризация и инверсия: фон шумный. Если результат неуверенный, кадр читается ещё раз без них (подробности — в списке блоков ниже)."
        }
        Label {
            objectName: "ocrAutoTuneResult"
            Layout.fillWidth: true
            visible: !!win.region && !!win.tuned
            wrapMode: Text.Wrap
            font.pixelSize: 12
            opacity: 0.85
            text: !win.tuned ? "" : (win.tuned.applied ? "Применено: " + win.tuned.results[win.tuned.best].name + ".  " : "Текст не найден ни с одним набором.  ")
                  + win.tuned.results.map(r => r.name + " — " + Math.round(r.score)).join(" · ")
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
            text: win.region ? "Перевод текста области захвата:  " + win.region.translation : ""
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
                Accessible.description: copyRecognizedTextHint.accessibleExplanation
                UnavailableHint { id: copyRecognizedTextHint; control: copyRecognizedText; feature: "Копировать распознанный текст"; reason: "Нет распознанного текста выбранной области захвата."; remedy: "Выберите область с результатом распознавания." }
                onClicked: win.controller.copyText(win.region.original)
            }
            Item { Layout.fillWidth: true }
        }
    }
}
