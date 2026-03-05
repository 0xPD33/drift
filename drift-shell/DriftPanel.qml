import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

QtObject {
    id: root

    required property QtObject driftState
    required property color bgColor
    required property color bgSecondary
    required property color textColor
    required property color textDim
    required property color accentColor

    property bool showing: false

    function toggle() { showing = !showing; }
    function open() { showing = true; }
    function close() { showing = false; }

    property PanelWindow backdrop: PanelWindow {
        visible: root.showing
        color: Qt.rgba(0, 0, 0, 0.01)

        anchors {
            top: true
            left: true
            right: true
            bottom: true
        }

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "drift-backdrop"
        exclusionMode: ExclusionMode.Ignore

        MouseArea {
            anchors.fill: parent
            onClicked: root.close()
        }
    }

    property PanelWindow panel: PanelWindow {
        visible: root.showing

        anchors {
            top: true
            right: true
            bottom: true
        }
        implicitWidth: 280
        color: "transparent"

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "drift-panel"
        exclusionMode: ExclusionMode.Ignore

        margins {
            top: 0
            right: 0
            bottom: 38  // bar height
        }

        // Background
        Rectangle {
            anchors.fill: parent
            color: root.bgColor
            border.color: root.accentColor
            border.width: 1
            radius: 0

            // Content
            Flickable {
                anchors.fill: parent
                anchors.margins: 8
                contentHeight: folderColumn.height
                clip: true

                Column {
                    id: folderColumn
                    width: parent.width
                    spacing: 4

                    Repeater {
                        model: Object.keys(root.driftState.folders)
                        delegate: Column {
                            id: folderDelegate
                            required property string modelData
                            width: folderColumn.width
                            spacing: 2

                            property string folderName: modelData
                            property var projects: root.driftState.folders[folderName] || []
                            property bool hasActive: {
                                for (var i = 0; i < projects.length; i++) {
                                    if (projects[i].is_active) return true;
                                }
                                return false;
                            }
                            property bool expanded: hasActive

                            // Folder header
                            Rectangle {
                                width: parent.width
                                height: 28
                                radius: 6
                                color: folderMouse.containsMouse ? Qt.lighter(root.bgSecondary, 1.2) : root.bgSecondary

                                Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                MouseArea {
                                    id: folderMouse
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: folderDelegate.expanded = !folderDelegate.expanded
                                }

                                Row {
                                    anchors.verticalCenter: parent.verticalCenter
                                    x: 8
                                    spacing: 6

                                    Text {
                                        text: folderDelegate.expanded ? "\u25bc" : "\u25b8"
                                        font.family: "JetBrainsMono Nerd Font"
                                        font.pixelSize: 9
                                        color: root.textDim
                                        anchors.verticalCenter: parent.verticalCenter
                                    }

                                    Text {
                                        text: folderDelegate.folderName === "_ungrouped" ? "ungrouped" : folderDelegate.folderName
                                        font.family: "JetBrainsMono Nerd Font"
                                        font.pixelSize: 11
                                        color: root.textColor
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                }

                                // Aggregate dots when collapsed
                                Row {
                                    visible: !folderDelegate.expanded
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.right: parent.right
                                    anchors.rightMargin: 8
                                    spacing: 3

                                    Repeater {
                                        model: {
                                            if (folderDelegate.expanded) return [];
                                            var dots = [];
                                            for (var i = 0; i < folderDelegate.projects.length; i++) {
                                                var svcs = folderDelegate.projects[i].services || [];
                                                for (var j = 0; j < svcs.length; j++) {
                                                    dots.push(svcs[j].status);
                                                }
                                            }
                                            return dots;
                                        }
                                        delegate: Rectangle {
                                            required property string modelData
                                            width: 6; height: 6; radius: 3
                                            color: {
                                                switch (modelData) {
                                                    case "running": return "#a0d0a0";
                                                    case "failed":  return "#d26a6a";
                                                    case "backoff": return "#d2b46a";
                                                    default:        return root.textDim;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Project list (when expanded)
                            Column {
                                visible: folderDelegate.expanded
                                width: parent.width
                                spacing: 1

                                Repeater {
                                    model: folderDelegate.projects
                                    delegate: Column {
                                        id: projectDelegate
                                        required property var modelData
                                        width: parent.width
                                        spacing: 0

                                        property string projectName: modelData.name
                                        property bool isActive: modelData.is_active
                                        property var services: modelData.services || []
                                        property bool hovered: false
                                        property bool isFocused: {
                                            var ws = modelData.workspaces || [];
                                            for (var i = 0; i < ws.length; i++) {
                                                if (ws[i].is_focused) return true;
                                            }
                                            return false;
                                        }

                                        // Project row
                                        Rectangle {
                                            width: parent.width
                                            height: 26
                                            radius: 4
                                            color: projectDelegate.isFocused
                                                ? Qt.lighter(root.bgSecondary, 1.3)
                                                : projectMouse.containsMouse
                                                    ? Qt.lighter(root.bgSecondary, 1.2)
                                                    : "transparent"

                                            Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                            // Left accent for focused
                                            Rectangle {
                                                visible: projectDelegate.isFocused
                                                width: 3
                                                height: parent.height - 6
                                                y: 3
                                                radius: 2
                                                color: root.accentColor
                                            }

                                            MouseArea {
                                                id: projectMouse
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: {
                                                    navProc.command = ["drift", "to", projectDelegate.projectName];
                                                    navProc.running = true;
                                                }
                                                onContainsMouseChanged: projectDelegate.hovered = containsMouse
                                            }

                                            Row {
                                                x: 24
                                                anchors.verticalCenter: parent.verticalCenter
                                                spacing: 6

                                                Text {
                                                    text: projectDelegate.isActive ? "\u25a0" : "\u25a1"
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: 10
                                                    color: projectDelegate.isActive ? root.textColor : root.textDim
                                                    anchors.verticalCenter: parent.verticalCenter
                                                }

                                                Text {
                                                    text: projectDelegate.projectName
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: 11
                                                    color: projectDelegate.isFocused ? root.textColor : root.textDim
                                                    anchors.verticalCenter: parent.verticalCenter
                                                }
                                            }

                                            // Service dots + close button
                                            Row {
                                                anchors.verticalCenter: parent.verticalCenter
                                                anchors.right: parent.right
                                                anchors.rightMargin: 8
                                                spacing: 6

                                                Row {
                                                    spacing: 3
                                                    anchors.verticalCenter: parent.verticalCenter

                                                    Repeater {
                                                        model: projectDelegate.services
                                                        delegate: Rectangle {
                                                            required property var modelData
                                                            width: 6; height: 6; radius: 3
                                                            color: {
                                                                switch (modelData.status) {
                                                                    case "running": return "#a0d0a0";
                                                                    case "failed":  return "#d26a6a";
                                                                    case "backoff": return "#d2b46a";
                                                                    default:        return root.textDim;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                // Close button (visible on hover for active projects)
                                                Text {
                                                    visible: projectDelegate.hovered && projectDelegate.isActive
                                                    text: "\u00d7"
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: 14
                                                    color: closeMouse.containsMouse ? "#d26a6a" : root.textDim
                                                    anchors.verticalCenter: parent.verticalCenter

                                                    MouseArea {
                                                        id: closeMouse
                                                        anchors.fill: parent
                                                        anchors.margins: -4
                                                        hoverEnabled: true
                                                        cursorShape: Qt.PointingHandCursor
                                                        onClicked: {
                                                            closeProc.command = ["drift", "close", projectDelegate.projectName];
                                                            closeProc.running = true;
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Service details on hover
                                        Column {
                                            visible: projectDelegate.hovered && projectDelegate.services.length > 0
                                            width: parent.width
                                            spacing: 0

                                            Repeater {
                                                model: projectDelegate.hovered ? projectDelegate.services : []
                                                delegate: Rectangle {
                                                    required property var modelData
                                                    width: parent.width
                                                    height: 20
                                                    color: "transparent"

                                                    Row {
                                                        x: 40
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        spacing: 8

                                                        Text {
                                                            text: modelData.name
                                                            font.family: "JetBrainsMono Nerd Font"
                                                            font.pixelSize: 9
                                                            color: root.textDim
                                                        }

                                                        Text {
                                                            text: modelData.status
                                                            font.family: "JetBrainsMono Nerd Font"
                                                            font.pixelSize: 9
                                                            color: {
                                                                switch (modelData.status) {
                                                                    case "running": return "#a0d0a0";
                                                                    case "failed":  return "#d26a6a";
                                                                    case "backoff": return "#d2b46a";
                                                                    default:        return root.textDim;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Process {
            id: navProc
            command: ["drift", "to", ""]
        }

        Process {
            id: closeProc
            command: ["drift", "close", ""]
        }
    }
}
