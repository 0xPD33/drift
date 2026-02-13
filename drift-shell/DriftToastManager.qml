import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

PanelWindow {
    id: toastManager

    required property QtObject driftState
    required property color bgColor
    required property color bgSecondary
    required property color textColor
    required property color textDim

    // Position: top-right corner
    anchors {
        top: true
        right: true
    }
    implicitWidth: 276
    implicitHeight: toastColumn.height + 16
    color: "transparent"

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "drift-toasts"
    exclusionMode: ExclusionMode.Ignore

    // Only visible when there are toasts
    visible: toastList.length > 0

    margins {
        top: 8
        right: 8
    }

    property var toastList: []

    // Which event types produce toasts
    function shouldToast(eventType) {
        return eventType === "agent.completed"
            || eventType === "agent.error"
            || eventType === "agent.needs_review"
            || eventType === "service.crashed"
            || eventType === "build.failed";
    }

    function titleForEvent(event) {
        if (event.title) return event.title;
        var parts = event.type ? event.type.split(".") : [];
        return parts.length > 1 ? parts[1] : (event.type || "event");
    }

    Connections {
        target: toastManager.driftState
        function onEventReceived(event) {
            var eventType = event.type || "";
            if (!toastManager.shouldToast(eventType)) return;

            var item = {
                id: Date.now(),
                eventType: eventType,
                source: event.source || "",
                project: event.project || "",
                title: toastManager.titleForEvent(event)
            };

            var list = toastManager.toastList.slice();
            list.unshift(item);
            if (list.length > 3) list = list.slice(0, 3);
            toastManager.toastList = list;
        }
    }

    function removeToast(toastId) {
        var list = toastManager.toastList.slice();
        toastManager.toastList = list.filter(function(t) { return t.id !== toastId; });
    }

    function navigateToProject(project) {
        if (project) navProc.command = ["drift", "to", project];
        navProc.running = true;
    }

    Process {
        id: navProc
        command: ["drift", "to", ""]
    }

    Column {
        id: toastColumn
        x: 8
        y: 8
        spacing: 8

        Repeater {
            model: toastManager.toastList
            delegate: DriftToast {
                required property var modelData
                eventType: modelData.eventType
                source: modelData.source
                project: modelData.project
                title: modelData.title
                bgColor: toastManager.bgColor
                bgSecondary: toastManager.bgSecondary
                textColor: toastManager.textColor
                textDim: toastManager.textDim
                onClicked: {
                    toastManager.navigateToProject(modelData.project);
                    toastManager.removeToast(modelData.id);
                }
                onDismissed: toastManager.removeToast(modelData.id)
            }
        }
    }
}
