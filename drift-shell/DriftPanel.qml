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
    property bool _visible: false
    property bool createMode: false

    onShowingChanged: {
        if (showing) {
            _visible = true;
        } else {
            _hideTimer.start();
        }
    }

    function toggle() { showing = !showing; }
    function open() { showing = true; }
    function close() { showing = false; }

    property var _hideTimer: Timer {
        interval: 220
        onTriggered: root._visible = false
    }

    property PanelWindow backdrop: PanelWindow {
        visible: root._visible
        color: "transparent"

        anchors {
            top: true
            left: true
            right: true
            bottom: true
        }

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "drift-backdrop"
        exclusionMode: ExclusionMode.Ignore

        Rectangle {
            anchors.fill: parent
            color: Qt.rgba(0, 0, 0, root.showing ? 0.15 : 0)

            Behavior on color { ColorAnimation { duration: 200; easing.type: Easing.OutCubic } }

            MouseArea {
                anchors.fill: parent
                onClicked: root.close()
            }
        }
    }

    property PanelWindow panel: PanelWindow {
        visible: root._visible

        anchors {
            top: true
            left: true
            bottom: true
        }
        implicitWidth: 320
        color: "transparent"

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "drift-panel"
        exclusionMode: ExclusionMode.Ignore

        margins {
            top: 0
            left: 0
            bottom: 38  // bar height
        }

        Item {
            anchors.fill: parent
            clip: true

            Rectangle {
                id: panelContent
                width: parent.width
                height: parent.height
                x: root.showing ? 0 : -width
                color: root.bgColor
                border.color: root.accentColor
                border.width: 1
                radius: 0

                Behavior on x {
                    NumberAnimation { duration: 200; easing.type: Easing.OutCubic }
                }

                // Content
                Flickable {
                    anchors.fill: parent
                    anchors.margins: 12
                    contentHeight: folderColumn.height
                    clip: true

                    Column {
                        id: folderColumn
                        width: parent.width
                        spacing: 6

                        Repeater {
                            model: Object.keys(root.driftState.folders)
                            delegate: Column {
                                id: folderDelegate
                                required property string modelData
                                width: folderColumn.width
                                spacing: 3

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
                                    height: 34
                                    radius: 8
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
                                        x: 10
                                        spacing: 8

                                        Text {
                                            text: folderDelegate.expanded ? "\u25bc" : "\u25b8"
                                            font.family: "JetBrainsMono Nerd Font"
                                            font.pixelSize: 11
                                            color: root.textDim
                                            anchors.verticalCenter: parent.verticalCenter
                                        }

                                        Text {
                                            text: folderDelegate.folderName === "_ungrouped" ? "ungrouped" : folderDelegate.folderName
                                            font.family: "JetBrainsMono Nerd Font"
                                            font.pixelSize: 13
                                            color: root.textColor
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                    }

                                    // Aggregate dots when collapsed
                                    Row {
                                        visible: !folderDelegate.expanded
                                        anchors.verticalCenter: parent.verticalCenter
                                        anchors.right: parent.right
                                        anchors.rightMargin: 10
                                        spacing: 4

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
                                                width: 7; height: 7; radius: 3.5
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
                                    spacing: 2

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
                                                height: 32
                                                radius: 6
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
                                                    height: parent.height - 8
                                                    y: 4
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
                                                    x: 26
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    spacing: 8

                                                    Text {
                                                        text: projectDelegate.isActive ? "\u25a0" : "\u25a1"
                                                        font.family: "JetBrainsMono Nerd Font"
                                                        font.pixelSize: 12
                                                        color: projectDelegate.isActive ? root.textColor : root.textDim
                                                        anchors.verticalCenter: parent.verticalCenter
                                                    }

                                                    Text {
                                                        text: projectDelegate.projectName
                                                        font.family: "JetBrainsMono Nerd Font"
                                                        font.pixelSize: 13
                                                        color: projectDelegate.isFocused ? root.textColor : root.textDim
                                                        anchors.verticalCenter: parent.verticalCenter
                                                    }
                                                }

                                                // Service dots + close button
                                                Row {
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    anchors.right: parent.right
                                                    anchors.rightMargin: 10
                                                    spacing: 8

                                                    Row {
                                                        spacing: 4
                                                        anchors.verticalCenter: parent.verticalCenter

                                                        Repeater {
                                                            model: projectDelegate.services
                                                            delegate: Rectangle {
                                                                required property var modelData
                                                                width: 7; height: 7; radius: 3.5
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
                                                        font.pixelSize: 18
                                                        color: closeMouse.containsMouse ? "#d26a6a" : root.textDim
                                                        anchors.verticalCenter: parent.verticalCenter

                                                        MouseArea {
                                                            id: closeMouse
                                                            anchors.fill: parent
                                                            anchors.margins: -6
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
                                                        height: 24
                                                        color: "transparent"

                                                        Row {
                                                            x: 44
                                                            anchors.verticalCenter: parent.verticalCenter
                                                            spacing: 10

                                                            Text {
                                                                text: modelData.name
                                                                font.family: "JetBrainsMono Nerd Font"
                                                                font.pixelSize: 11
                                                                color: root.textDim
                                                            }

                                                            Text {
                                                                text: modelData.status
                                                                font.family: "JetBrainsMono Nerd Font"
                                                                font.pixelSize: 11
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

                        // Create project row
                        Rectangle {
                            width: folderColumn.width
                            height: 34
                            radius: 8
                            color: root.createMode
                                ? root.bgSecondary
                                : createBtnMouse.containsMouse
                                    ? Qt.lighter(root.bgSecondary, 1.2)
                                    : "transparent"

                            Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                            Row {
                                visible: !root.createMode
                                anchors.verticalCenter: parent.verticalCenter
                                x: 10
                                spacing: 8

                                Text {
                                    text: "+"
                                    font.family: "JetBrainsMono Nerd Font"
                                    font.pixelSize: 14
                                    font.bold: true
                                    color: root.textDim
                                    anchors.verticalCenter: parent.verticalCenter
                                }

                                Text {
                                    text: "New project"
                                    font.family: "JetBrainsMono Nerd Font"
                                    font.pixelSize: 13
                                    color: root.textDim
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                            }

                            MouseArea {
                                id: createBtnMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                visible: !root.createMode
                                onClicked: {
                                    root.createMode = true;
                                    createInput.text = "";
                                    createInput.forceActiveFocus();
                                }
                            }

                            TextInput {
                                id: createInput
                                visible: root.createMode
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.leftMargin: 10
                                anchors.rightMargin: 10
                                font.family: "JetBrainsMono Nerd Font"
                                font.pixelSize: 13
                                color: root.textColor
                                onAccepted: {
                                    var name = text.trim();
                                    if (name !== "") {
                                        initProc.command = ["drift", "init", name];
                                        initProc.running = true;
                                    }
                                    root.createMode = false;
                                }
                                Keys.onEscapePressed: {
                                    root.createMode = false;
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

        Process {
            id: initProc
            command: ["drift", "init", ""]
        }
    }
}
