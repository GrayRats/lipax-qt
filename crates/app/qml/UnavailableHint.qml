import QtQuick

HoverHint {
    required property string reason
    property string remedy: ""
    readonly property string accessibleExplanation: active ? explanation : feature
    active: !control.enabled
    explanation: feature + "\n\nФункция недоступна.\n\n" + reason + (remedy.length ? "\n\n" + remedy : "")
}
