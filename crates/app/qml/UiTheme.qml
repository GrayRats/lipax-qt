import QtQuick

// Logical units: Qt applies each screen's DPR. Never multiply these by devicePixelRatio.
QtObject {
    property color surface: "#ffffff"
    readonly property bool dark: surface.hslLightness < 0.5
    readonly property color success: dark ? "#7bd88f" : "#176638"
    readonly property color warning: dark ? "#ffc23d" : "#805000"
    readonly property color error: dark ? "#ff8f8f" : "#a12222"
    readonly property color information: dark ? "#9ecbff" : "#245c91"
    property string textScale: "normal"
    readonly property real scale: ({small: 0.95, normal: 1, large: 1.2, extra_large: 1.4})[textScale] || 1
    readonly property real systemPoints: Qt.application.font.pointSize > 0 ? Qt.application.font.pointSize : Qt.application.font.pixelSize * 0.75
    readonly property real bodyPoints: Math.max(11, systemPoints * scale)
    readonly property font body: Qt.font({family: Qt.application.font.family, pointSize: bodyPoints})
    readonly property font description: Qt.font({family: body.family, pointSize: Math.max(11, bodyPoints * 0.95)})
    readonly property font heading: Qt.font({family: body.family, pointSize: bodyPoints * 1.12, weight: Font.DemiBold})
    readonly property font title: Qt.font({family: body.family, pointSize: bodyPoints * 1.5, weight: Font.Bold})
    readonly property int gap: 8
    readonly property int margin: 12
    readonly property int radius: 6
    readonly property int iconButtonSize: 38
    readonly property int controlHeight: Math.ceil(bodyPoints * 1.34 + 16)
    readonly property int iconSize: 24
}
