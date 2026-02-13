import QtQuick

Rectangle {
    id: toast

    required property string eventType
    required property string source
    required property string project
    required property string title
    required property color bgColor
    required property color bgSecondary
    required property color textColor
    required property color textDim

    signal clicked()
    signal dismissed()

    property color borderColor: {
        if (eventType.indexOf("error") >= 0 || eventType === "service.crashed" || eventType === "build.failed")
            return "#d26a6a";
        if (eventType === "agent.needs_review")
            return "#d2b46a";
        return "#a0d0a0";
    }

    property bool persistent: {
        return eventType.indexOf("error") >= 0
            || eventType === "service.crashed"
            || eventType === "build.failed"
            || eventType === "agent.needs_review";
    }

    width: 260
    height: col.height + 16
    radius: 8
    color: bgSecondary
    clip: true

    // Colored left border
    Rectangle {
        width: 4
        height: parent.height
        color: toast.borderColor
        radius: 2
    }

    Column {
        id: col
        x: 14
        y: 8
        width: parent.width - 22
        spacing: 2

        Text {
            text: toast.source + " " + toast.title
            font.family: "JetBrainsMono Nerd Font"
            font.pixelSize: 11
            color: toast.textColor
            width: parent.width
            elide: Text.ElideRight
        }

        Row {
            spacing: 4
            Text {
                text: toast.project
                font.family: "JetBrainsMono Nerd Font"
                font.pixelSize: 10
                color: toast.textDim
            }
            Text {
                text: "\u2192"
                font.family: "JetBrainsMono Nerd Font"
                font.pixelSize: 10
                color: toast.textDim
                opacity: 0.6
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: toast.clicked()
    }

    // Auto-dismiss for non-persistent toasts
    Timer {
        interval: 6000
        running: !toast.persistent
        onTriggered: toast.dismissed()
    }

    // Enter animation
    opacity: 0
    Component.onCompleted: {
        opacity = 1;
    }
    Behavior on opacity { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
}
