import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: driftState

    // --- Exposed state ---
    property bool daemonRunning: false
    property string activeProject: ""
    property var folders: ({})       // { folderName: [{ name, icon, is_active, workspaces, services }] }
    property var recentEvents: []

    // --- Signals ---
    signal eventReceived(var event)

    // --- Polling: drift shell-data every 2s ---
    property var pollTimer: Timer {
        interval: 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: shellDataProc.running = true
    }

    property var shellDataProc: Process {
        command: ["drift", "shell-data"]
        stdout: SplitParser {
            splitMarker: ""
            onRead: data => {
                try {
                    var parsed = JSON.parse(data);
                    driftState.daemonRunning = parsed.daemon_running || false;
                    driftState.activeProject = parsed.active_project || "";
                    driftState.folders = parsed.folders || {};
                } catch (e) {}
            }
        }
    }

    // --- Event stream: subscribe.sock ---
    property string subscribeSockPath: {
        var xdg = Quickshell.env("XDG_RUNTIME_DIR") || "/tmp";
        return xdg + "/drift/subscribe.sock";
    }

    property var eventSocket: Socket {
        path: driftState.subscribeSockPath
        connected: true
        parser: SplitParser {
            onRead: data => {
                try {
                    var event = JSON.parse(data);
                    var evts = driftState.recentEvents.slice();
                    evts.push(event);
                    if (evts.length > 50) evts = evts.slice(-50);
                    driftState.recentEvents = evts;
                    driftState.eventReceived(event);
                } catch (e) {}
            }
        }
        onError: error => {
            eventSocket.connected = false;
            reconnectTimer.start();
        }
    }

    property var reconnectTimer: Timer {
        interval: 3000
        repeat: false
        onTriggered: {
            driftState.eventSocket.connected = true;
        }
    }
}
