import Quickshell
import Quickshell.Wayland
import QtQuick

Scope {
    property string translationText: Quickshell.env("LIPA_TEXT") ?? ""

    PanelWindow {
        id: window
        visible: translationText !== ""

        anchors {
            top: true
            left: true
            right: true
        }
        margins.top: 10

        exclusiveZone: 0
        color: "transparent"
        
        implicitHeight: contentRect.height

        WlrLayershell.layer: WlrLayer.Overlay

        Item {
            anchors.fill: parent

            Rectangle {
                id: contentRect
                anchors.horizontalCenter: parent.horizontalCenter
                y: 0

                // Ширина подстраивается под текст, но никогда не превышает 750 пикселей
                width: Math.min(textItem.implicitWidth + 40, 750)
                height: textItem.height + 30

                color: "#CC181818"
                border.color: "#555555"
                border.width: 1
                radius: 16

                opacity: window.visible ? 1.0 : 0.0
                Behavior on opacity {
                    NumberAnimation { duration: 250; easing.type: Easing.OutQuad }
                }

                Text {
                    id: textItem
                    anchors.centerIn: parent
                    // Текст будет переноситься, если он шире чем 710 пикселей (750 минус отступы)
                    width: 710
                    text: translationText
                    color: "#ffffff"
                    font.pixelSize: 20
                    font.weight: Font.Normal
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: window.visible = false
                }
            }
        }
    }
}
