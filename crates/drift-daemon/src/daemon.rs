use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{fs, thread};

use niri_ipc::{Event as NiriEvent, Window, Workspace};
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use nix::unistd::Pid;

use drift_core::config;
use drift_core::events::{self, Event};
use drift_core::paths;
use crate::state::{DaemonState, WorkspaceProject};

const STATE_WRITE_INTERVAL: Duration = Duration::from_secs(5);

pub enum DaemonMsg {
    NiriEvent(NiriEvent),
    EmitEvent(Event),
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

fn install_signal_handlers() {
    unsafe {
        let action = SigAction::new(
            SigHandler::Handler(handle_signal),
            SaFlags::SA_RESTART,
            SigSet::empty(),
        );
        signal::sigaction(Signal::SIGTERM, &action).expect("install SIGTERM handler");
        signal::sigaction(Signal::SIGINT, &action).expect("install SIGINT handler");
    }
}

struct DaemonInner {
    workspaces: HashMap<u64, Workspace>,
    windows: HashMap<u64, Window>,
    workspace_to_project: HashMap<u64, String>,
    known_projects: HashSet<String>,
    active_project: Option<String>,
    focused_workspace_id: Option<u64>,
    events: HashMap<String, VecDeque<Event>>,
    buffer_size: usize,
    terminal_name: String,
    subscriber_tx: mpsc::Sender<Event>,
}

impl DaemonInner {
    fn new(subscriber_tx: mpsc::Sender<Event>, buffer_size: usize, terminal_name: String) -> Self {
        let known_projects: HashSet<String> = drift_core::registry::list_projects()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.project.name)
            .collect();

        Self {
            workspaces: HashMap::new(),
            windows: HashMap::new(),
            workspace_to_project: HashMap::new(),
            known_projects,
            active_project: None,
            focused_workspace_id: None,
            events: HashMap::new(),
            buffer_size,
            terminal_name,
            subscriber_tx,
        }
    }

    fn handle_niri_event(&mut self, event: NiriEvent) {
        match event {
            NiriEvent::WorkspacesChanged { workspaces } => {
                let old_projects: HashSet<String> = self.workspace_to_project.values().cloned().collect();

                self.workspaces = workspaces.into_iter().map(|ws| (ws.id, ws)).collect();
                if let Ok(projects) = drift_core::registry::list_projects() {
                    self.known_projects = projects.into_iter().map(|p| p.project.name).collect();
                }
                self.rebuild_workspace_project_map();

                let new_projects: HashSet<String> = self.workspace_to_project.values().cloned().collect();

                for project in new_projects.difference(&old_projects) {
                    self.process_event(Event {
                        event_type: "workspace.created".into(),
                        project: project.clone(),
                        source: "daemon".into(),
                        ts: events::iso_now(),
                        level: Some("info".into()),
                        title: None,
                        body: None,
                        meta: None,
                        priority: None,
                    });
                }
                for project in old_projects.difference(&new_projects) {
                    self.process_event(Event {
                        event_type: "workspace.destroyed".into(),
                        project: project.clone(),
                        source: "daemon".into(),
                        ts: events::iso_now(),
                        level: Some("info".into()),
                        title: None,
                        body: None,
                        meta: None,
                        priority: None,
                    });
                }

                self.update_active_project();
            }
            NiriEvent::WorkspaceActivated { id, focused } => {
                if focused {
                    if let Some(prev_id) = self.focused_workspace_id {
                        if prev_id != id {
                            if let Some(project) = self.workspace_to_project.get(&prev_id).cloned() {
                                let windows: Vec<drift_core::workspace::SavedWindow> = self.windows.values()
                                    .filter(|w| w.workspace_id == Some(prev_id))
                                    .map(|w| drift_core::workspace::SavedWindow {
                                        app_id: w.app_id.clone(),
                                        title: w.title.clone(),
                                    })
                                    .collect();
                                if let Err(e) = drift_core::workspace::write_snapshot(&project, windows) {
                                    eprintln!("auto-save workspace '{project}': {e}");
                                }

                                let running_windows: Vec<(String, Option<String>)> = self.windows.values()
                                    .filter(|w| w.workspace_id == Some(prev_id))
                                    .filter_map(|w| {
                                        let app_id = w.app_id.clone()?;
                                        Some((app_id, w.title.clone()))
                                    })
                                    .collect();
                                if let Err(e) = drift_core::sync::sync_windows_to_config(&project, &running_windows, &self.terminal_name) {
                                    eprintln!("auto-sync windows for '{project}': {e}");
                                }

                                self.process_event(Event {
                                    event_type: "workspace.deactivated".into(),
                                    project: project.clone(),
                                    source: "daemon".into(),
                                    ts: events::iso_now(),
                                    level: Some("info".into()),
                                    title: None,
                                    body: None,
                                    meta: None,
                                    priority: None,
                                });
                            }
                        }
                    }
                }

                if let Some(ws) = self.workspaces.get(&id) {
                    let output = ws.output.clone();
                    let ids_on_output: Vec<u64> = self.workspaces.values()
                        .filter(|ws| ws.output == output)
                        .map(|ws| ws.id)
                        .collect();
                    for ws_id in ids_on_output {
                        if let Some(ws) = self.workspaces.get_mut(&ws_id) {
                            ws.is_active = ws_id == id;
                            if focused {
                                ws.is_focused = ws_id == id;
                            }
                        }
                    }
                }
                if focused {
                    for ws in self.workspaces.values_mut() {
                        if ws.id != id {
                            ws.is_focused = false;
                        }
                    }
                    self.focused_workspace_id = Some(id);
                    self.update_active_project();

                    if let Some(project) = self.workspace_to_project.get(&id).cloned() {
                        self.process_event(Event {
                            event_type: "workspace.activated".into(),
                            project: project.clone(),
                            source: "daemon".into(),
                            ts: events::iso_now(),
                            level: Some("info".into()),
                            title: None,
                            body: None,
                            meta: None,
                            priority: None,
                        });
                    }
                }
            }
            NiriEvent::WindowsChanged { windows } => {
                self.windows = windows.into_iter().map(|w| (w.id, w)).collect();
            }
            NiriEvent::WindowOpenedOrChanged { window } => {
                self.windows.insert(window.id, window);
            }
            NiriEvent::WindowClosed { id } => {
                self.windows.remove(&id);
            }
            NiriEvent::WindowFocusChanged { id } => {
                for win in self.windows.values_mut() {
                    win.is_focused = Some(win.id) == id;
                }
            }
            NiriEvent::WindowUrgencyChanged { id, urgent } => {
                if urgent {
                    if let Some(win) = self.windows.get(&id) {
                        if let Some(ws_id) = win.workspace_id {
                            if let Some(project) = self.workspace_to_project.get(&ws_id).cloned() {
                                let is_active = self.active_project.as_deref() == Some(&project);
                                if !is_active {
                                    self.process_event(Event {
                                        event_type: "window.urgent".into(),
                                        project,
                                        source: "window".into(),
                                        ts: events::iso_now(),
                                        level: Some("warning".into()),
                                        title: Some("Window needs attention".into()),
                                        body: win.title.clone(),
                                        meta: None,
                                        priority: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn rebuild_workspace_project_map(&mut self) {
        self.workspace_to_project.clear();
        for ws in self.workspaces.values() {
            if let Some(name) = &ws.name {
                if self.known_projects.contains(name.as_str()) {
                    self.workspace_to_project.insert(ws.id, name.clone());
                }
            }
        }
    }

    fn update_active_project(&mut self) {
        self.active_project = self.focused_workspace_id
            .and_then(|id| self.workspace_to_project.get(&id))
            .cloned();
    }

    fn classify_priority(&self, event: &Event) -> &'static str {
        let is_active = self.active_project.as_deref() == Some(event.project.as_str());
        let level = event.level.as_deref().unwrap_or("info");
        match (is_active, level) {
            (true, "error") => "critical",
            (true, "success" | "warning") => "high",
            (true, _) => "low",
            (false, "error") => "high",
            (false, "success") => "medium",
            (false, _) => "silent",
        }
    }

    fn process_event(&mut self, mut event: Event) {
        let priority = self.classify_priority(&event);
        event.priority = Some(priority.into());

        let buffer = self.events
            .entry(event.project.clone())
            .or_insert_with(|| VecDeque::with_capacity(self.buffer_size + 1));
        buffer.push_back(event.clone());
        if buffer.len() > self.buffer_size {
            buffer.pop_front();
        }

        let _ = self.subscriber_tx.send(event.clone());

        if matches!(priority, "critical" | "high" | "medium") {
            self.send_desktop_notification(&event);
        }
    }

    fn handle_emit_event(&mut self, event: Event) {
        self.process_event(event);
    }

    fn send_desktop_notification(&self, event: &Event) {
        let priority = event.priority.as_deref().unwrap_or("low");
        let urgency = match priority {
            "critical" => "critical",
            "high" => "normal",
            "medium" => "low",
            _ => return,
        };
        let title_text = event.title.as_deref().unwrap_or("");
        let title = format!("[{}] {}", event.project, title_text);
        let body = event.body.as_deref().unwrap_or("");

        let _ = std::process::Command::new("notify-send")
            .args([
                "--app-name=drift",
                &format!("--urgency={urgency}"),
                &title,
                body,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    fn write_state_to_disk(&self) {
        let state = DaemonState {
            pid: std::process::id(),
            active_project: self.active_project.clone(),
            workspace_projects: self.workspaces.values()
                .filter_map(|ws| {
                    let name = ws.name.as_ref()?;
                    let project = self.workspace_to_project.get(&ws.id)?;
                    let window_count = self.windows.values()
                        .filter(|w| w.workspace_id == Some(ws.id))
                        .count() as u32;
                    Some(WorkspaceProject {
                        workspace_id: ws.id,
                        workspace_name: name.clone(),
                        project: project.clone(),
                        is_active: ws.is_active,
                        is_focused: ws.is_focused,
                        window_count,
                    })
                })
                .collect(),
            recent_events: self.events.iter()
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect(),
        };

        let path = paths::daemon_state_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = fs::write(&tmp, &json);
            let _ = fs::rename(&tmp, &path);
        }
    }
}

fn spawn_commander() -> Option<u32> {
    let pid_path = paths::commander_pid_path();
    // Check if already running
    if pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                if nix::sys::signal::kill(Pid::from_raw(pid), None).is_ok() {
                    eprintln!("commander already running (PID {pid})");
                    return Some(pid as u32);
                }
            }
        }
    }

    let drift_bin = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("commander: cannot determine binary path: {e}");
            return None;
        }
    };

    let log_path = paths::state_base_dir().join("commander.log");
    let log_file = match fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("commander: cannot open log: {e}");
            return None;
        }
    };
    let stderr_file = match log_file.try_clone() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("commander: cannot clone log fd: {e}");
            return None;
        }
    };

    match std::process::Command::new(&drift_bin)
        .args(["_commander"])
        .stdout(log_file)
        .stderr(stderr_file)
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            eprintln!("commander spawned (PID {pid})");
            Some(pid)
        }
        Err(e) => {
            eprintln!("commander: failed to spawn: {e}");
            None
        }
    }
}

fn stop_commander() {
    let pid_path = paths::commander_pid_path();
    if let Ok(pid_str) = fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            let _ = signal::kill(Pid::from_raw(pid), Signal::SIGTERM);
        }
    }
}

pub fn run_daemon() -> anyhow::Result<()> {
    install_signal_handlers();

    let global_config = config::load_global_config().unwrap_or_default();
    let commander_enabled = global_config.commander.enabled;
    let events_config = global_config.events;
    let terminal_name = global_config.defaults.terminal;

    let pid_path = paths::daemon_pid_path();
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&pid_path, std::process::id().to_string())?;

    let (msg_tx, msg_rx) = mpsc::channel::<DaemonMsg>();
    let (sub_tx, sub_rx) = mpsc::channel::<Event>();

    let mut inner = DaemonInner::new(sub_tx, events_config.buffer_size, terminal_name);

    let tx_events = msg_tx.clone();
    let event_thread = thread::Builder::new()
        .name("event-stream".into())
        .spawn(move || crate::event_stream::run_event_stream(tx_events, &SHUTDOWN))?;

    let tx_emit = msg_tx;
    let emit_thread = thread::Builder::new()
        .name("emit-listener".into())
        .spawn(move || crate::emit_listener::run_emit_listener(tx_emit, &SHUTDOWN))?;

    let replay_count = events_config.replay_on_subscribe;
    let subscriber_thread = thread::Builder::new()
        .name("subscriber-manager".into())
        .spawn(move || crate::subscriber::run_subscriber_manager(sub_rx, &SHUTDOWN, replay_count))?;

    if commander_enabled {
        spawn_commander();
    }

    eprintln!("drift daemon started (PID {})", std::process::id());

    let mut last_state_write = Instant::now();
    inner.write_state_to_disk();

    while !SHUTDOWN.load(Ordering::Relaxed) {
        match msg_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(DaemonMsg::NiriEvent(event)) => {
                inner.handle_niri_event(event);
            }
            Ok(DaemonMsg::EmitEvent(event)) => {
                inner.handle_emit_event(event);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if last_state_write.elapsed() >= STATE_WRITE_INTERVAL {
            inner.write_state_to_disk();
            last_state_write = Instant::now();
        }
    }

    inner.write_state_to_disk();
    let _ = fs::remove_file(&pid_path);

    if commander_enabled {
        stop_commander();
    }

    eprintln!("drift daemon shutting down");

    drop(msg_rx);
    let _ = event_thread.join();
    let _ = emit_thread.join();
    let _ = subscriber_thread.join();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_inner() -> DaemonInner {
        let (tx, _rx) = mpsc::channel();
        DaemonInner {
            workspaces: HashMap::new(),
            windows: HashMap::new(),
            workspace_to_project: HashMap::new(),
            known_projects: HashSet::new(),
            active_project: None,
            focused_workspace_id: None,
            events: HashMap::new(),
            buffer_size: 200,
            terminal_name: "ghostty".into(),
            subscriber_tx: tx,
        }
    }

    fn test_event(project: &str, level: &str) -> Event {
        Event {
            event_type: "test".into(),
            project: project.into(),
            source: "test".into(),
            ts: "2026-01-01T00:00:00Z".into(),
            level: Some(level.into()),
            title: None,
            body: None,
            meta: None,
            priority: None,
        }
    }

    #[test]
    fn priority_active_error_is_critical() {
        let mut inner = test_inner();
        inner.active_project = Some("proj".into());
        assert_eq!(inner.classify_priority(&test_event("proj", "error")), "critical");
    }

    #[test]
    fn priority_active_success_is_high() {
        let mut inner = test_inner();
        inner.active_project = Some("proj".into());
        assert_eq!(inner.classify_priority(&test_event("proj", "success")), "high");
    }

    #[test]
    fn priority_active_warning_is_high() {
        let mut inner = test_inner();
        inner.active_project = Some("proj".into());
        assert_eq!(inner.classify_priority(&test_event("proj", "warning")), "high");
    }

    #[test]
    fn priority_active_info_is_low() {
        let mut inner = test_inner();
        inner.active_project = Some("proj".into());
        assert_eq!(inner.classify_priority(&test_event("proj", "info")), "low");
    }

    #[test]
    fn priority_background_error_is_high() {
        let mut inner = test_inner();
        inner.active_project = Some("other".into());
        assert_eq!(inner.classify_priority(&test_event("proj", "error")), "high");
    }

    #[test]
    fn priority_background_success_is_medium() {
        let mut inner = test_inner();
        inner.active_project = Some("other".into());
        assert_eq!(inner.classify_priority(&test_event("proj", "success")), "medium");
    }

    #[test]
    fn priority_background_info_is_silent() {
        let mut inner = test_inner();
        inner.active_project = Some("other".into());
        assert_eq!(inner.classify_priority(&test_event("proj", "info")), "silent");
    }

    #[test]
    fn priority_no_active_project_is_silent() {
        let inner = test_inner();
        assert_eq!(inner.classify_priority(&test_event("proj", "info")), "silent");
    }

    #[test]
    fn process_event_stores_in_buffer() {
        let mut inner = test_inner();
        inner.active_project = Some("proj".into());
        let event = test_event("proj", "info");
        inner.process_event(event);
        let buffer = inner.events.get("proj").unwrap();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer[0].event_type, "test");
    }

    #[test]
    fn process_event_sets_priority() {
        let mut inner = test_inner();
        inner.active_project = Some("proj".into());
        let event = test_event("proj", "error");
        inner.process_event(event);
        let buffer = inner.events.get("proj").unwrap();
        assert_eq!(buffer[0].priority.as_deref(), Some("critical"));
    }

    #[test]
    fn process_event_respects_buffer_size() {
        let mut inner = test_inner();
        inner.buffer_size = 2;
        inner.active_project = Some("proj".into());
        for i in 0..3 {
            let mut event = test_event("proj", "info");
            event.event_type = format!("event-{i}");
            inner.process_event(event);
        }
        let buffer = inner.events.get("proj").unwrap();
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].event_type, "event-1");
        assert_eq!(buffer[1].event_type, "event-2");
    }
}
