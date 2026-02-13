import QtQuick
import QtQuick.Layouts

Rectangle {
    id: driftStatus

    required property QtObject driftState
    required property color bgColor
    required property color bgSecondary
    required property color textColor
    required property color textDim
    required property color accentColor

    signal clicked()

    Layout.preferredHeight: 30
    Layout.preferredWidth: content.width + 16
    color: mouseArea.containsMouse ? Qt.lighter(bgSecondary, 1.2) : bgSecondary
    radius: 8

    Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: driftStatus.clicked()
    }

    Row {
        id: content
        anchors.centerIn: parent
        spacing: 6

        Text {
            text: {
                if (!driftStatus.driftState.daemonRunning) return "\uf071"; // warning icon
                var proj = driftStatus.driftState.activeProject;
                if (!proj) return "drift";

                // Find folder for active project
                var folders = driftStatus.driftState.folders;
                var folderName = "";
                for (var f in folders) {
                    var projects = folders[f];
                    for (var i = 0; i < projects.length; i++) {
                        if (projects[i].name === proj) {
                            folderName = (f !== "_ungrouped") ? f : "";
                            break;
                        }
                    }
                }

                if (folderName) return folderName + " / " + proj;
                return proj;
            }
            font.family: "JetBrainsMono Nerd Font"
            font.pixelSize: 11
            color: driftStatus.textColor
            anchors.verticalCenter: parent.verticalCenter
        }

        // Status dots for active project services
        Row {
            spacing: 3
            anchors.verticalCenter: parent.verticalCenter
            visible: dotRepeater.count > 0

            Repeater {
                id: dotRepeater
                model: {
                    if (!driftStatus.driftState.daemonRunning) return [];
                    var proj = driftStatus.driftState.activeProject;
                    if (!proj) return [];

                    var folders = driftStatus.driftState.folders;
                    for (var f in folders) {
                        var projects = folders[f];
                        for (var i = 0; i < projects.length; i++) {
                            if (projects[i].name === proj)
                                return projects[i].services || [];
                        }
                    }
                    return [];
                }
                delegate: Rectangle {
                    required property var modelData
                    width: 6
                    height: 6
                    radius: 3
                    color: {
                        switch (modelData.status) {
                            case "running": return "#a0d0a0";
                            case "failed":  return "#d26a6a";
                            case "backoff": return "#d2b46a";
                            default:        return driftStatus.textDim;
                        }
                    }
                }
            }
        }
    }
}
