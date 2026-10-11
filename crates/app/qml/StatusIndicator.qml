import QtQuick

// A compact state light: running (green), stopped (grey), processing (amber), error (red). Colours come from the
// semantic SVGs and are not tinted; the state is also carried by the shape of each SVG and by `Accessible`,
// so colour is never the only cue. Running/processing breathe slowly (~2 s); nothing moves for a hidden window,
// for a stopped or error state, or when motion is reduced.
Item {
    id: root
    // "running" | "stopped" | "processing" | "error"
    property string state: "stopped"
    property bool reducedMotion: false
    readonly property var names: ({ running: "Работает", stopped: "Остановлен", processing: "Обработка", error: "Ошибка" })
    readonly property string label: names[state] || names.stopped
    property string description: label
    property real pulse: 0
    readonly property bool animated: !reducedMotion && (state === "running" || state === "processing")
        && visible && Window.window !== null && Window.window.visible
    implicitWidth: 22
    implicitHeight: 22
    Accessible.role: Accessible.StaticText
    Accessible.name: label
    Accessible.description: description

    Image {
        anchors.centerIn: parent
        width: 20
        height: 20
        sourceSize: Qt.size(40, 40)
        smooth: true
        source: (Qt.resolvedUrl(".").toString().startsWith("qrc:") ? "qrc:/lipa/icons/" : "../assets/") + "status-" + root.state + ".svg"
        opacity: root.animated ? 0.62 + 0.38 * root.pulse : 1
        scale: root.animated ? 0.94 + 0.06 * root.pulse : 1
    }
    // One breath = 2 s (1 s up, 1 s down).
    SequentialAnimation {
        running: root.animated
        loops: Animation.Infinite
        onRunningChanged: if (!running) root.pulse = 0
        NumberAnimation { target: root; property: "pulse"; from: 0; to: 1; duration: 1000; easing.type: Easing.InOutSine }
        NumberAnimation { target: root; property: "pulse"; from: 1; to: 0; duration: 1000; easing.type: Easing.InOutSine }
    }
    HoverHint { control: root; feature: root.label; explanation: root.description }
}
