import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: driftState

    // --- Exposed state ---
    property bool daemonRunning: false
    property string activeProject: ""
    property var folders: ({})       // { folderName: [{ name, icon, is_active, workspaces, services, tasks, project_state }] }
    property var workspaces: []
    property var recentEvents: []
    property var focus: ({ mode: "overview", active_project: null, niri_workspace_id: null })
    property var reviewQueue: []
    property var globalSummary: ({ total_agents_running: 0, total_tasks_queued: 0, total_reviews_pending: 0 })
    property string panelMode: focus.mode === "workspace" ? "working" : "director"

    // agents[]: { workspace_id, project, name, driver, state, last_event_ts }
    // Populated from agent.* events on subscribe.sock + synced from services on poll
    property var agents: []

    // unmanagedWorkspaces[]: { id, name, window_count }
    // Populated from workspace.unmanaged events
    property var unmanagedWorkspaces: []

    function _updateAgent(project, name, driver, state, ts) {
        var list = driftState.agents.slice();
        var found = false;
        for (var i = 0; i < list.length; i++) {
            if (list[i].project === project && list[i].name === name) {
                list[i] = { workspace_id: list[i].workspace_id, project: project, name: name, driver: driver || list[i].driver, state: state, last_event_ts: ts };
                found = true;
                break;
            }
        }
        if (!found) {
            list.push({ workspace_id: null, project: project, name: name, driver: driver || "claude", state: state, last_event_ts: ts });
        }
        driftState.agents = list;
    }

    function _removeAgent(project, name) {
        var list = driftState.agents.filter(function(a) {
            return !(a.project === project && a.name === name);
        });
        driftState.agents = list;
    }

    function _handleEvent(event) {
        var t = event.type;
        var project = event.project || "";
        var ts = event.ts || "";
        var meta = event.meta || {};

        if (t === "workspace.unmanaged") {
            var wsId = meta.workspace_id;
            var wsName = meta.workspace_name || "";
            var winCount = meta.window_count || 0;
            var list = driftState.unmanagedWorkspaces.slice();
            var idx = -1;
            for (var i = 0; i < list.length; i++) {
                if (list[i].id === wsId) { idx = i; break; }
            }
            if (idx >= 0) {
                list[idx] = { id: wsId, name: wsName, window_count: winCount };
            } else {
                list.push({ id: wsId, name: wsName, window_count: winCount });
            }
            driftState.unmanagedWorkspaces = list;
        } else if (t === "workspace.created" || t === "workspace.destroyed") {
            // Re-sync unmanaged list on workspace topology changes
            if (t === "workspace.destroyed") {
                driftState.unmanagedWorkspaces = driftState.unmanagedWorkspaces.filter(function(u) {
                    return u.name !== (meta.workspace_name || "");
                });
            }
        } else if (t === "agent.started" || t === "agent.working") {
            var agentName = (meta.agent) || (event.source) || "agent";
            var driver = meta.driver || "claude";
            _updateAgent(project, agentName, driver, "working", ts);
        } else if (t === "agent.blocked") {
            var agentName = (meta.agent) || (event.source) || "agent";
            _updateAgent(project, agentName, null, "blocked", ts);
        } else if (t === "agent.needs_review") {
            var agentName = (meta.agent) || (event.source) || "agent";
            _updateAgent(project, agentName, null, "needs_review", ts);
        } else if (t === "agent.completed") {
            var agentName = (meta.agent) || (event.source) || "agent";
            _updateAgent(project, agentName, null, "completed", ts);
        } else if (t === "agent.error") {
            var agentName = (meta.agent) || (event.source) || "agent";
            _updateAgent(project, agentName, null, "error", ts);
        }
    }

    // --- Signals ---
    signal eventReceived(var event)

    // --- Polling: drift shell-data every 1s ---
    property var pollTimer: Timer {
        interval: 1000
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
                    driftState.workspaces = parsed.workspaces || [];
                    driftState.folders = parsed.folders || {};
                    driftState.focus = parsed.focus || { mode: "overview", active_project: null, niri_workspace_id: null };
                    driftState.reviewQueue = parsed.review_queue || [];
                    driftState.globalSummary = parsed.global || { total_agents_running: 0, total_tasks_queued: 0, total_reviews_pending: 0 };

                    // Sync agent state from services (is_agent=true services give us live driver info)
                    var wsList = parsed.workspaces || [];
                    var seenKeys = {};
                    var updatedAgents = driftState.agents.slice();
                    for (var wi = 0; wi < wsList.length; wi++) {
                        var ws = wsList[wi];
                        var proj = ws.project ? ws.project.name : null;
                        if (!proj) continue;
                        var services = ws.project.services || [];
                        for (var si = 0; si < services.length; si++) {
                            var svc = services[si];
                            if (!svc.is_agent) continue;
                            var key = proj + "/" + svc.name;
                            seenKeys[key] = true;
                            var agentState = (svc.status === "running") ? "working" : (svc.status === "failed" ? "error" : "idle");
                            var found = false;
                            for (var ai = 0; ai < updatedAgents.length; ai++) {
                                if (updatedAgents[ai].project === proj && updatedAgents[ai].name === svc.name) {
                                    // Only override state if it's "idle" or service is not running (don't clobber event-driven states)
                                    if (agentState === "idle" || svc.status !== "running") {
                                        updatedAgents[ai] = {
                                            workspace_id: ws.id,
                                            project: proj,
                                            name: svc.name,
                                            driver: updatedAgents[ai].driver || "claude",
                                            state: agentState,
                                            last_event_ts: updatedAgents[ai].last_event_ts
                                        };
                                    } else {
                                        updatedAgents[ai] = {
                                            workspace_id: ws.id,
                                            project: proj,
                                            name: svc.name,
                                            driver: updatedAgents[ai].driver || "claude",
                                            state: updatedAgents[ai].state,
                                            last_event_ts: updatedAgents[ai].last_event_ts
                                        };
                                    }
                                    found = true;
                                    break;
                                }
                            }
                            if (!found) {
                                updatedAgents.push({ workspace_id: ws.id, project: proj, name: svc.name, driver: "claude", state: agentState, last_event_ts: null });
                            }
                        }
                    }
                    driftState.agents = updatedAgents;
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
                    driftState._handleEvent(event);
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
