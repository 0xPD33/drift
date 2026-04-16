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
            createMode = false;
            _hideTimer.start();
        }
    }

    function toggle() { showing = !showing; }
    function open() { showing = true; }
    function close() { showing = false; }

    function timeAgo(isoString) {
        if (!isoString) return "";
        var then = new Date(isoString);
        var now = new Date();
        var mins = Math.floor((now - then) / 60000);
        if (mins < 1) return "just now";
        if (mins < 60) return mins + "m ago";
        var hours = Math.floor(mins / 60);
        if (hours < 24) return hours + "h ago";
        return Math.floor(hours / 24) + "d ago";
    }

    function agentStateColor(state) {
        switch (state) {
            case "working":     return "#d2c46a";
            case "blocked":     return "#d26a6a";
            case "needs_review": return "#d2a46a";
            case "completed":   return "#a0d0a0";
            case "error":       return "#d26a6a";
            default:            return root.textDim;
        }
    }

    function agentStateLabel(state) {
        switch (state) {
            case "working":     return "working";
            case "blocked":     return "blocked";
            case "needs_review": return "review";
            case "completed":   return "done";
            case "error":       return "error";
            default:            return "idle";
        }
    }

    function agentsForProject(projectName) {
        if (!root.driftState) return [];
        var all = root.driftState.agents || [];
        var result = [];
        for (var i = 0; i < all.length; i++) {
            if (all[i].project === projectName) result.push(all[i]);
        }
        return result;
    }

    function workspaceDotColor(ws) {
        if (ws.project) {
            var tasks = ws.project.tasks || {};
            if ((tasks.failed || 0) > 0) return "#d26a6a";
            if ((tasks.needs_review || 0) > 0) return "#d2b46a";
            if ((tasks.running || 0) > 0) return "#a0d0a0";
            return root.textDim;
        }
        return ws.is_focused ? "#a0d0a0" : root.textDim;
    }

    function workspaceStatusLine(ws) {
        if (!ws.project) return "";
        var tasks = ws.project.tasks || {};
        if ((tasks.running || 0) > 0) {
            var rt = tasks.running_tasks || [];
            if (rt.length > 0) {
                var t = rt[0];
                var agent = t.agent || "agent";
                var desc = t.description || "";
                if (desc.length > 30) desc = desc.substring(0, 30) + "...";
                return agent + " \u25b8 " + desc;
            }
            return "agent running";
        }
        if ((tasks.needs_review || 0) > 0) return "\u26a0 review pending";
        if ((tasks.queued || 0) > 0) return "idle \u00b7 " + tasks.queued + " queued";
        return "idle";
    }

    function projectStatusColor(p) {
        var tasks = p.tasks || {};
        if ((tasks.failed || 0) > 0) return "#d26a6a";
        if ((tasks.needs_review || 0) > 0) return "#d2b46a";
        if ((tasks.running || 0) > 0) return "#a0d0a0";
        return root.textDim;
    }

    property var closedProjectList: {
        var openNames = {};
        var ws = root.driftState ? (root.driftState.workspaces || []) : [];
        for (var i = 0; i < ws.length; i++) {
            if (ws[i].project) openNames[ws[i].project.name] = true;
        }
        var closed = [];
        var folders = root.driftState ? (root.driftState.folders || {}) : {};
        for (var f in folders) {
            var projects = folders[f];
            for (var j = 0; j < projects.length; j++) {
                if (!openNames[projects[j].name]) {
                    closed.push(projects[j]);
                }
            }
        }
        return closed;
    }

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
                onClicked: {
                    root.createMode = false;
                    root.close();
                }
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
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand
        WlrLayershell.namespace: "drift-panel"
        exclusionMode: ExclusionMode.Ignore

        margins {
            top: 0
            left: 0
            bottom: 38
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

                DriftCommandBar {
                    id: commandBar
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    driftState: root.driftState
                    bgColor: root.bgColor
                    bgSecondary: root.bgSecondary
                    textColor: root.textColor
                    textDim: root.textDim
                    accentColor: root.accentColor
                }

                Flickable {
                    anchors.top: parent.top
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: commandBar.top
                    anchors.margins: 12
                    contentHeight: mainColumn.height
                    clip: true

                    Column {
                        id: mainColumn
                        width: parent.width
                        spacing: 6

                        // ═══════════════════════════════════
                        // WORKSPACES
                        // ═══════════════════════════════════

                        Text {
                            text: "WORKSPACES"
                            font.family: "JetBrainsMono Nerd Font"
                            font.pixelSize: 11
                            font.bold: true
                            color: root.textDim
                            x: 4
                        }

                        Column {
                            width: mainColumn.width
                            spacing: 2

                            Repeater {
                                model: root.driftState ? (root.driftState.workspaces || []) : []
                                delegate: Column {
                                    id: wsDelegate
                                    required property var modelData
                                    required property int index
                                    width: parent.width
                                    spacing: 0

                                    property var ws: modelData
                                    property bool hasProject: !!ws.project
                                    property bool hovered: false
                                    property string displayName: ws.name || ("Workspace " + (index + 1))

                                    // --- Working mode ---
                                    Rectangle {
                                        visible: !root.driftState || root.driftState.panelMode === "working"
                                        width: wsDelegate.width
                                        height: wsWorkingCol.height + 10
                                        radius: 6
                                        color: wsDelegate.ws.is_focused
                                            ? Qt.lighter(root.bgSecondary, 1.3)
                                            : wsMouse.containsMouse
                                                ? Qt.lighter(root.bgSecondary, 1.2)
                                                : "transparent"

                                        Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                        Rectangle {
                                            visible: wsDelegate.ws.is_focused
                                            width: 3
                                            height: parent.height - 8
                                            y: 4
                                            radius: 2
                                            color: root.accentColor
                                        }

                                        MouseArea {
                                            id: wsMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: (wsDelegate.hasProject || wsDelegate.ws.name) ? Qt.PointingHandCursor : Qt.ArrowCursor
                                            onClicked: {
                                                if (wsDelegate.hasProject) {
                                                    navProc.command = ["drift", "to", wsDelegate.ws.project.name];
                                                    navProc.running = true;
                                                } else if (wsDelegate.ws.name) {
                                                    navProc.command = ["niri", "msg", "action", "focus-workspace", wsDelegate.ws.name];
                                                    navProc.running = true;
                                                }
                                            }
                                            onContainsMouseChanged: wsDelegate.hovered = containsMouse
                                        }

                                        Column {
                                            id: wsWorkingCol
                                            x: 12
                                            y: 5
                                            width: parent.width - 24
                                            spacing: 1

                                            Row {
                                                width: parent.width
                                                spacing: 6

                                                Rectangle {
                                                    width: 8; height: 8; radius: 4
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    color: root.workspaceDotColor(wsDelegate.ws)
                                                }

                                                Text {
                                                    text: wsDelegate.hasProject ? wsDelegate.ws.project.name : wsDelegate.displayName
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: 13
                                                    color: wsDelegate.ws.is_focused ? root.textColor : root.textDim
                                                    anchors.verticalCenter: parent.verticalCenter
                                                }

                                                Item { height: 1; width: 1 }

                                                Row {
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    spacing: 6
                                                    layoutDirection: Qt.RightToLeft

                                                    Text {
                                                        visible: wsDelegate.hovered && wsDelegate.hasProject
                                                        text: "\u00d7"
                                                        font.family: "JetBrainsMono Nerd Font"
                                                        font.pixelSize: 18
                                                        color: wsCloseMouse.containsMouse ? "#d26a6a" : root.textDim
                                                        anchors.verticalCenter: parent.verticalCenter

                                                        MouseArea {
                                                            id: wsCloseMouse
                                                            anchors.fill: parent
                                                            anchors.margins: -6
                                                            hoverEnabled: true
                                                            cursorShape: Qt.PointingHandCursor
                                                            onClicked: {
                                                                closeProc.command = ["drift", "close", wsDelegate.ws.project.name];
                                                                closeProc.running = true;
                                                            }
                                                        }
                                                    }

                                                    Text {
                                                        text: wsDelegate.ws.window_count + " win"
                                                        font.family: "JetBrainsMono Nerd Font"
                                                        font.pixelSize: 10
                                                        color: root.textDim
                                                        anchors.verticalCenter: parent.verticalCenter
                                                    }

                                                    Row {
                                                        visible: wsDelegate.hasProject
                                                        spacing: 4
                                                        anchors.verticalCenter: parent.verticalCenter

                                                        Repeater {
                                                            model: wsDelegate.hasProject ? (wsDelegate.ws.project.services || []) : []
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
                                                }
                                            }

                                            Text {
                                                visible: wsDelegate.hasProject
                                                text: "  " + root.workspaceStatusLine(wsDelegate.ws)
                                                font.family: "JetBrainsMono Nerd Font"
                                                font.pixelSize: 11
                                                color: root.textDim
                                                width: parent.width
                                                elide: Text.ElideRight
                                            }
                                        }
                                    }

                                    // --- Director mode ---
                                    Rectangle {
                                        visible: root.driftState && root.driftState.panelMode === "director"
                                        width: wsDelegate.width
                                        height: wsDirCol.height + (wsDelegate.hasProject ? 16 : 10)
                                        radius: wsDelegate.hasProject ? 8 : 6
                                        color: wsDelegate.hasProject
                                            ? (wsDirMouse.containsMouse ? Qt.lighter(root.bgSecondary, 1.2) : root.bgSecondary)
                                            : (wsDelegate.ws.is_focused
                                                ? Qt.lighter(root.bgSecondary, 1.3)
                                                : wsDirMouse.containsMouse
                                                    ? Qt.lighter(root.bgSecondary, 1.2)
                                                    : "transparent")

                                        Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                        Rectangle {
                                            visible: wsDelegate.ws.is_focused
                                            width: 3
                                            height: parent.height - 8
                                            y: 4
                                            radius: 2
                                            color: root.accentColor
                                        }

                                        MouseArea {
                                            id: wsDirMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: (wsDelegate.hasProject || wsDelegate.ws.name) ? Qt.PointingHandCursor : Qt.ArrowCursor
                                            onClicked: {
                                                if (wsDelegate.hasProject) {
                                                    navProc.command = ["drift", "to", wsDelegate.ws.project.name];
                                                    navProc.running = true;
                                                } else if (wsDelegate.ws.name) {
                                                    navProc.command = ["niri", "msg", "action", "focus-workspace", wsDelegate.ws.name];
                                                    navProc.running = true;
                                                }
                                            }
                                            onContainsMouseChanged: wsDelegate.hovered = containsMouse
                                        }

                                        Column {
                                            id: wsDirCol
                                            x: 12
                                            y: wsDelegate.hasProject ? 8 : 5
                                            width: parent.width - 24
                                            spacing: wsDelegate.hasProject ? 4 : 1

                                            Row {
                                                width: parent.width
                                                spacing: 6

                                                Rectangle {
                                                    width: wsDelegate.hasProject ? 10 : 8
                                                    height: wsDelegate.hasProject ? 10 : 8
                                                    radius: width / 2
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    color: root.workspaceDotColor(wsDelegate.ws)
                                                }

                                                Text {
                                                    text: wsDelegate.hasProject ? wsDelegate.ws.project.name : wsDelegate.displayName
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: wsDelegate.hasProject ? 14 : 13
                                                    font.bold: wsDelegate.hasProject
                                                    color: wsDelegate.ws.is_focused ? root.textColor : root.textDim
                                                    anchors.verticalCenter: parent.verticalCenter
                                                }

                                                Item { width: 1; height: 1 }

                                                Row {
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    spacing: 6
                                                    layoutDirection: Qt.RightToLeft

                                                    Text {
                                                        visible: wsDelegate.hovered && wsDelegate.hasProject
                                                        text: "\u00d7"
                                                        font.family: "JetBrainsMono Nerd Font"
                                                        font.pixelSize: 18
                                                        color: wsDirCloseMouse.containsMouse ? "#d26a6a" : root.textDim
                                                        anchors.verticalCenter: parent.verticalCenter

                                                        MouseArea {
                                                            id: wsDirCloseMouse
                                                            anchors.fill: parent
                                                            anchors.margins: -6
                                                            hoverEnabled: true
                                                            cursorShape: Qt.PointingHandCursor
                                                            onClicked: {
                                                                closeProc.command = ["drift", "close", wsDelegate.ws.project.name];
                                                                closeProc.running = true;
                                                            }
                                                        }
                                                    }

                                                    Text {
                                                        text: wsDelegate.ws.window_count + " win"
                                                        font.family: "JetBrainsMono Nerd Font"
                                                        font.pixelSize: 10
                                                        color: root.textDim
                                                        anchors.verticalCenter: parent.verticalCenter
                                                    }
                                                }
                                            }

                                            Text {
                                                visible: wsDelegate.hasProject
                                                property var rt: {
                                                    var rts = wsDelegate.hasProject ? ((wsDelegate.ws.project.tasks || {}).running_tasks || []) : [];
                                                    return rts.length > 0 ? rts[0] : null;
                                                }
                                                text: rt ? "  " + (rt.agent || "agent") + " running" : "  no agent running"
                                                font.family: "JetBrainsMono Nerd Font"
                                                font.pixelSize: 11
                                                color: rt ? "#a0d0a0" : root.textDim
                                            }

                                            Text {
                                                visible: wsDelegate.hasProject && ((wsDelegate.ws.project.tasks || {}).running_tasks || []).length > 0
                                                text: {
                                                    if (!wsDelegate.hasProject) return "";
                                                    var rts = (wsDelegate.ws.project.tasks || {}).running_tasks || [];
                                                    return rts.length > 0 ? "  Task: " + rts[0].description : "";
                                                }
                                                font.family: "JetBrainsMono Nerd Font"
                                                font.pixelSize: 11
                                                color: root.textColor
                                                width: parent.width
                                                elide: Text.ElideRight
                                            }

                                            Text {
                                                visible: {
                                                    if (!wsDelegate.hasProject) return false;
                                                    var rts = (wsDelegate.ws.project.tasks || {}).running_tasks || [];
                                                    return rts.length > 0 && !!rts[0].started_at;
                                                }
                                                text: {
                                                    if (!wsDelegate.hasProject) return "";
                                                    var rts = (wsDelegate.ws.project.tasks || {}).running_tasks || [];
                                                    if (rts.length > 0 && rts[0].started_at)
                                                        return "  Started: " + root.timeAgo(rts[0].started_at);
                                                    return "";
                                                }
                                                font.family: "JetBrainsMono Nerd Font"
                                                font.pixelSize: 11
                                                color: root.textDim
                                            }

                                            Text {
                                                visible: wsDelegate.hasProject && ((wsDelegate.ws.project.tasks || {}).queued || 0) > 0
                                                text: "  Queue: " + ((wsDelegate.ws.project.tasks || {}).queued || 0) + " tasks remaining"
                                                font.family: "JetBrainsMono Nerd Font"
                                                font.pixelSize: 11
                                                color: root.textDim
                                            }

                                            Row {
                                                visible: wsDelegate.hasProject && (wsDelegate.ws.project.services || []).length > 0
                                                x: 12
                                                spacing: 4

                                                Repeater {
                                                    model: wsDelegate.hasProject ? (wsDelegate.ws.project.services || []) : []
                                                    delegate: Row {
                                                        required property var modelData
                                                        spacing: 4

                                                        Rectangle {
                                                            width: 6; height: 6; radius: 3
                                                            anchors.verticalCenter: parent.verticalCenter
                                                            color: {
                                                                switch (modelData.status) {
                                                                    case "running": return "#a0d0a0";
                                                                    case "failed":  return "#d26a6a";
                                                                    case "backoff": return "#d2b46a";
                                                                    default:        return root.textDim;
                                                                }
                                                            }
                                                        }

                                                        Text {
                                                            text: modelData.name
                                                            font.family: "JetBrainsMono Nerd Font"
                                                            font.pixelSize: 10
                                                            color: root.textDim
                                                            anchors.verticalCenter: parent.verticalCenter
                                                        }
                                                    }
                                                }
                                            }

                                            // --- Agent rows (director mode) ---
                                            Column {
                                                visible: wsDelegate.hasProject
                                                width: parent.width
                                                spacing: 2

                                                property var wsAgents: wsDelegate.hasProject ? root.agentsForProject(wsDelegate.ws.project.name) : []

                                                Repeater {
                                                    model: parent.wsAgents
                                                    delegate: Row {
                                                        id: agentRow
                                                        required property var modelData
                                                        required property int index
                                                        x: 4
                                                        spacing: 5
                                                        width: wsDirCol.width - 4

                                                        Rectangle {
                                                            width: 5; height: 5; radius: 3
                                                            anchors.verticalCenter: parent.verticalCenter
                                                            color: root.agentStateColor(agentRow.modelData.state)
                                                        }

                                                        Text {
                                                            text: agentRow.modelData.name
                                                            font.family: "JetBrainsMono Nerd Font"
                                                            font.pixelSize: 10
                                                            color: root.textColor
                                                            anchors.verticalCenter: parent.verticalCenter
                                                            elide: Text.ElideRight
                                                            width: Math.min(implicitWidth, 100)
                                                        }

                                                        Rectangle {
                                                            anchors.verticalCenter: parent.verticalCenter
                                                            width: driverLabel.implicitWidth + 6
                                                            height: 14
                                                            radius: 3
                                                            color: Qt.rgba(1, 1, 1, 0.07)

                                                            Text {
                                                                id: driverLabel
                                                                anchors.centerIn: parent
                                                                text: agentRow.modelData.driver || "claude"
                                                                font.family: "JetBrainsMono Nerd Font"
                                                                font.pixelSize: 9
                                                                color: root.textDim
                                                            }
                                                        }

                                                        Text {
                                                            text: root.agentStateLabel(agentRow.modelData.state)
                                                            font.family: "JetBrainsMono Nerd Font"
                                                            font.pixelSize: 10
                                                            color: root.agentStateColor(agentRow.modelData.state)
                                                            anchors.verticalCenter: parent.verticalCenter
                                                        }

                                                        MouseArea {
                                                            anchors.fill: parent
                                                            anchors.margins: -4
                                                            cursorShape: Qt.PointingHandCursor
                                                            onClicked: {
                                                                if (agentRow.modelData.workspace_id) {
                                                                    navProc.command = ["niri", "msg", "action", "focus-workspace-by-id", String(agentRow.modelData.workspace_id)];
                                                                } else if (wsDelegate.hasProject) {
                                                                    navProc.command = ["drift", "to", wsDelegate.ws.project.name];
                                                                }
                                                                navProc.running = true;
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            Rectangle {
                                                visible: wsDelegate.hovered && wsDelegate.hasProject && ((wsDelegate.ws.project.tasks || {}).queued || 0) > 0
                                                width: 90
                                                height: 22
                                                radius: 4
                                                x: 12
                                                color: dispatchBtnMouse.containsMouse ? Qt.lighter(root.accentColor, 1.3) : root.accentColor

                                                Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                                Text {
                                                    anchors.centerIn: parent
                                                    text: "\u25b6 dispatch"
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: 11
                                                    color: root.bgColor
                                                }

                                                MouseArea {
                                                    id: dispatchBtnMouse
                                                    anchors.fill: parent
                                                    hoverEnabled: true
                                                    cursorShape: Qt.PointingHandCursor
                                                    onClicked: {
                                                        dispatchProc.command = ["drift", "dispatch", wsDelegate.ws.project.name];
                                                        dispatchProc.running = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ═══════════════════════════════════
                        // UNMANAGED WORKSPACES
                        // ═══════════════════════════════════

                        Column {
                            visible: root.driftState && (root.driftState.unmanagedWorkspaces || []).length > 0
                            width: mainColumn.width
                            spacing: 2

                            Item { width: 1; height: 4 }

                            Text {
                                text: "UNMANAGED"
                                font.family: "JetBrainsMono Nerd Font"
                                font.pixelSize: 11
                                font.bold: true
                                color: root.textDim
                                x: 4
                                opacity: 0.7
                            }

                            Repeater {
                                model: root.driftState ? (root.driftState.unmanagedWorkspaces || []) : []
                                delegate: Rectangle {
                                    id: unmanagedDelegate
                                    required property var modelData
                                    width: mainColumn.width
                                    height: unmanagedRow.implicitHeight + 14
                                    radius: 6
                                    color: unmanagedMouse.containsMouse
                                        ? Qt.lighter(root.bgSecondary, 1.2)
                                        : root.bgSecondary
                                    opacity: 0.75

                                    Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                    MouseArea {
                                        id: unmanagedMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                    }

                                    Row {
                                        id: unmanagedRow
                                        x: 10
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 6
                                        width: parent.width - 20

                                        Rectangle {
                                            width: 6; height: 6; radius: 3
                                            anchors.verticalCenter: parent.verticalCenter
                                            color: root.textDim
                                            opacity: 0.5
                                        }

                                        Text {
                                            text: unmanagedDelegate.modelData.name || ("ws " + unmanagedDelegate.modelData.id)
                                            font.family: "JetBrainsMono Nerd Font"
                                            font.pixelSize: 12
                                            color: root.textDim
                                            anchors.verticalCenter: parent.verticalCenter
                                        }

                                        Text {
                                            text: unmanagedDelegate.modelData.window_count + "w"
                                            font.family: "JetBrainsMono Nerd Font"
                                            font.pixelSize: 10
                                            color: root.textDim
                                            opacity: 0.6
                                            anchors.verticalCenter: parent.verticalCenter
                                        }

                                        Item { width: 1; height: 1; Layout.fillWidth: true }

                                        Rectangle {
                                            visible: unmanagedMouse.containsMouse
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: adoptLabel.implicitWidth + 12
                                            height: 20
                                            radius: 4
                                            color: adoptMouse.containsMouse ? Qt.lighter(root.accentColor, 1.3) : root.accentColor

                                            Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                            Text {
                                                id: adoptLabel
                                                anchors.centerIn: parent
                                                text: "adopt"
                                                font.family: "JetBrainsMono Nerd Font"
                                                font.pixelSize: 10
                                                color: root.bgColor
                                            }

                                            MouseArea {
                                                id: adoptMouse
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: {
                                                    adoptProc.command = ["drift", "adopt", unmanagedDelegate.modelData.name];
                                                    adoptProc.running = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ═══════════════════════════════════
                        // REVIEWS (director mode)
                        // ═══════════════════════════════════

                        Column {
                            visible: root.driftState && root.driftState.panelMode === "director" && (root.driftState.reviewQueue || []).length > 0
                            width: mainColumn.width
                            spacing: 4

                            Rectangle {
                                width: parent.width
                                height: 28
                                color: "transparent"

                                Text {
                                    x: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "REVIEWS (" + (root.driftState ? (root.driftState.reviewQueue || []).length : 0) + ")"
                                    font.family: "JetBrainsMono Nerd Font"
                                    font.pixelSize: 11
                                    font.bold: true
                                    color: "#d2b46a"
                                }
                            }

                            Repeater {
                                model: root.driftState ? (root.driftState.reviewQueue || []) : []
                                delegate: Rectangle {
                                    id: reviewDelegate
                                    required property var modelData
                                    width: mainColumn.width
                                    height: reviewCol.height + 16
                                    radius: 8
                                    color: root.bgSecondary

                                    Rectangle {
                                        width: 3
                                        height: parent.height
                                        color: "#d2b46a"
                                        radius: 2
                                    }

                                    Column {
                                        id: reviewCol
                                        x: 14
                                        y: 8
                                        width: parent.width - 28
                                        spacing: 4

                                        Text {
                                            text: "\u26a0 " + reviewDelegate.modelData.project + " / " + reviewDelegate.modelData.task_id
                                            font.family: "JetBrainsMono Nerd Font"
                                            font.pixelSize: 12
                                            color: "#d2b46a"
                                            width: parent.width
                                            elide: Text.ElideRight
                                        }

                                        Text {
                                            text: "  " + reviewDelegate.modelData.description
                                            font.family: "JetBrainsMono Nerd Font"
                                            font.pixelSize: 11
                                            color: root.textColor
                                            width: parent.width
                                            elide: Text.ElideRight
                                        }

                                        Row {
                                            x: 4
                                            spacing: 8

                                            Rectangle {
                                                width: 80
                                                height: 22
                                                radius: 4
                                                color: approveMouse.containsMouse ? Qt.lighter("#a0d0a0", 1.2) : "#a0d0a0"

                                                Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                                Text {
                                                    anchors.centerIn: parent
                                                    text: "\u2713 approve"
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: 11
                                                    color: root.bgColor
                                                }

                                                MouseArea {
                                                    id: approveMouse
                                                    anchors.fill: parent
                                                    hoverEnabled: true
                                                    cursorShape: Qt.PointingHandCursor
                                                    onClicked: {
                                                        reviewApproveProc.command = ["drift", "review", "approve", reviewDelegate.modelData.task_id];
                                                        reviewApproveProc.running = true;
                                                    }
                                                }
                                            }

                                            Rectangle {
                                                width: 70
                                                height: 22
                                                radius: 4
                                                color: rejectMouse.containsMouse ? Qt.lighter("#d26a6a", 1.2) : "#d26a6a"

                                                Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                                Text {
                                                    anchors.centerIn: parent
                                                    text: "\u2717 reject"
                                                    font.family: "JetBrainsMono Nerd Font"
                                                    font.pixelSize: 11
                                                    color: root.bgColor
                                                }

                                                MouseArea {
                                                    id: rejectMouse
                                                    anchors.fill: parent
                                                    hoverEnabled: true
                                                    cursorShape: Qt.PointingHandCursor
                                                    onClicked: {
                                                        reviewRejectProc.command = ["drift", "review", "reject", reviewDelegate.modelData.task_id];
                                                        reviewRejectProc.running = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ═══════════════════════════════════
                        // CLOSED PROJECTS
                        // ═══════════════════════════════════

                        Column {
                            visible: root.closedProjectList.length > 0
                            width: mainColumn.width
                            spacing: 2

                            Item { width: 1; height: 6 }

                            Text {
                                text: "PROJECTS (not open)"
                                font.family: "JetBrainsMono Nerd Font"
                                font.pixelSize: 11
                                font.bold: true
                                color: root.textDim
                                x: 4
                                opacity: 0.6
                            }

                            Repeater {
                                model: root.closedProjectList
                                delegate: Rectangle {
                                    id: closedDelegate
                                    required property var modelData
                                    width: mainColumn.width
                                    height: 30
                                    radius: 6
                                    color: closedMouse.containsMouse
                                        ? Qt.lighter(root.bgSecondary, 1.2)
                                        : "transparent"

                                    Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }

                                    MouseArea {
                                        id: closedMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            openProc.command = ["drift", "open", closedDelegate.modelData.name];
                                            openProc.running = true;
                                        }
                                    }

                                    Row {
                                        x: 12
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 6

                                        Rectangle {
                                            width: 6; height: 6; radius: 3
                                            anchors.verticalCenter: parent.verticalCenter
                                            color: root.projectStatusColor(closedDelegate.modelData)
                                            opacity: 0.6
                                        }

                                        Text {
                                            text: closedDelegate.modelData.name
                                            font.family: "JetBrainsMono Nerd Font"
                                            font.pixelSize: 12
                                            color: root.textDim
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                    }

                                    Text {
                                        text: "closed"
                                        font.family: "JetBrainsMono Nerd Font"
                                        font.pixelSize: 10
                                        color: root.textDim
                                        opacity: 0.6
                                        anchors.verticalCenter: parent.verticalCenter
                                        anchors.right: parent.right
                                        anchors.rightMargin: 12
                                    }
                                }
                            }
                        }

                        // ═══════════════════════════════════
                        // CREATE PROJECT
                        // ═══════════════════════════════════

                        Rectangle {
                            width: mainColumn.width
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
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("nav failed:", exitCode);
            }
        }

        Process {
            id: closeProc
            command: ["drift", "close", ""]
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("drift close failed:", exitCode);
            }
        }

        Process {
            id: initProc
            command: ["drift", "init", ""]
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("drift init failed:", exitCode);
            }
        }

        Process {
            id: openProc
            command: ["drift", "open", ""]
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("drift open failed:", exitCode);
            }
        }

        Process {
            id: dispatchProc
            command: ["drift", "dispatch", ""]
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("drift dispatch failed:", exitCode);
            }
        }

        Process {
            id: reviewApproveProc
            command: ["drift", "review", "approve", ""]
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("drift review approve failed:", exitCode);
            }
        }

        Process {
            id: reviewRejectProc
            command: ["drift", "review", "reject", ""]
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("drift review reject failed:", exitCode);
            }
        }

        Process {
            id: adoptProc
            command: ["drift", "adopt", ""]
            onExited: (exitCode, exitStatus) => {
                if (exitCode !== 0) console.warn("drift adopt failed:", exitCode);
            }
        }
    }
}
