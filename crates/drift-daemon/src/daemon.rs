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
use crate::state::{DaemonState, FocusState, NiriWorkspace, WorkspaceProject};

const STATE_WRITE_INTERVAL: Duration = Duration::from_secs(1);

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
    global_persist_windows: bool,
    subscriber_tx: mpsc::Sender<Event>,
    #[cfg(feature = "dispatch")]
    dispatch_tx: mpsc::Sender<Event>,
}

impl DaemonInner {
    #[cfg(feature = "dispatch")]
    fn new(subscriber_tx: mpsc::Sender<Event>, dispatch_tx: mpsc::Sender<Event>, buffer_size: usize, terminal_name: String, global_persist_windows: bool) -> Self {
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
            global_persist_windows,
            subscriber_tx,
            dispatch_tx,
        }
    }

    #[cfg(not(feature = "dispatch"))]
    fn new(subscriber_tx: mpsc::Sender<Event>, buffer_size: usize, terminal_name: String, global_persist_windows: bool) -> Self {
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
            global_persist_windows,
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
                self.emit_unmanaged_workspaces();

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
                                self.save_workspace_snapshot(&project, prev_id);

                                let persist = drift_core::config::load_project_config(&project)
                                    .ok()
                                    .and_then(|cfg| cfg.persist_windows)
                                    .unwrap_or(self.global_persist_windows);

                                if !persist {
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
                let ws_id = self.windows.get(&id).and_then(|w| w.workspace_id);
                self.windows.remove(&id);

                // Save snapshot AFTER removing so the closed window is excluded
                if let Some(ws_id) = ws_id {
                    if let Some(project) = self.workspace_to_project.get(&ws_id).cloned() {
                        self.save_workspace_snapshot(&project, ws_id);

                        let has_windows = self.windows.values().any(|w| w.workspace_id == Some(ws_id));
                        if !has_windows {
                            self.auto_close_project(&project);
                        }
                    }
                }
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
                } else {
                    // Match renamed workspaces: "project · status" -> "project"
                    for project in &self.known_projects {
                        if name.starts_with(project.as_str())
                            && name[project.len()..].starts_with(" \u{00b7} ")
                        {
                            self.workspace_to_project.insert(ws.id, project.clone());
                            break;
                        }
                    }
                }
            }
        }
    }

    fn emit_unmanaged_workspaces(&mut self) {
        let unmanaged: Vec<(u64, String, u32)> = self.workspaces.values()
            .filter(|ws| {
                ws.name.is_some()
                    && !self.workspace_to_project.contains_key(&ws.id)
            })
            .map(|ws| {
                let window_count = self.windows.values()
                    .filter(|w| w.workspace_id == Some(ws.id))
                    .count() as u32;
                (ws.id, ws.name.clone().unwrap(), window_count)
            })
            .filter(|(_, _, count)| *count > 0)
            .collect();

        for (ws_id, name, window_count) in unmanaged {
            let _ = self.subscriber_tx.send(Event {
                event_type: "workspace.unmanaged".into(),
                project: String::new(),
                source: "daemon".into(),
                ts: events::iso_now(),
                level: Some("info".into()),
                title: Some(format!("Unmanaged workspace: {name}")),
                body: None,
                meta: Some(serde_json::json!({
                    "workspace_id": ws_id,
                    "workspace_name": name,
                    "window_count": window_count,
                })),
                priority: Some("silent".into()),
            });
        }
    }

    fn update_active_project(&mut self) {
        self.active_project = self.focused_workspace_id
            .and_then(|id| self.workspace_to_project.get(&id))
            .cloned();
    }

    fn save_workspace_snapshot(&self, project: &str, ws_id: u64) {
        let windows: Vec<drift_core::workspace::SavedWindow> = self.windows.values()
            .filter(|w| w.workspace_id == Some(ws_id))
            .map(|w| drift_core::workspace::SavedWindow {
                app_id: w.app_id.clone(),
                title: w.title.clone(),
                config_name: drift_core::workspace::extract_config_name(
                    w.title.as_deref(),
                    project,
                ),
                width: Some(w.layout.tile_size.0),
                height: Some(w.layout.tile_size.1),
                column_index: w.layout.pos_in_scrolling_layout.map(|(col, _)| col),
            })
            .collect();
        if let Err(e) = drift_core::workspace::write_snapshot(project, windows) {
            eprintln!("auto-save workspace '{project}': {e}");
        }
    }

    fn auto_close_project(&mut self, project_name: &str) {
        if let Ok(cfg) = drift_core::config::load_project_config(project_name) {
            if !cfg.auto_close {
                return;
            }
        }

        drift_core::lifecycle::teardown_project(project_name);

        if let Ok(mut client) = drift_core::niri::NiriClient::connect() {
            let _ = client.unset_workspace_name(project_name);
        }

        self.process_event(Event {
            event_type: "drift.project.closed".into(),
            project: project_name.to_string(),
            source: "daemon".into(),
            ts: events::iso_now(),
            level: Some("info".into()),
            title: Some(format!("Auto-closed project '{project_name}'")),
            body: None,
            meta: None,
            priority: None,
        });
    }

    #[cfg(feature = "dispatch")]
    fn update_workspace_name(&self, project: &str) {
        use drift_core::tasks::{TaskQueue, TaskStatus};
        use drift_core::workspace_names::{format_workspace_name, WorkspaceDisplayState};

        if !self.workspace_to_project.values().any(|p| p == project) {
            return;
        }

        let state = match TaskQueue::load(project) {
            Ok(queue) => {
                let mut ds = WorkspaceDisplayState::default();
                for task in &queue.tasks {
                    match task.status {
                        TaskStatus::Running => {
                            ds.agent_running = task.assigned_agent.clone();
                            ds.task_summary =
                                Some(task.description.chars().take(30).collect::<String>());
                        }
                        TaskStatus::Queued => ds.queued_count += 1,
                        TaskStatus::NeedsReview => ds.needs_review = true,
                        TaskStatus::Failed => ds.error = true,
                        _ => {}
                    }
                }
                ds
            }
            Err(_) => WorkspaceDisplayState::default(),
        };

        let name = format_workspace_name(project, &state);

        if let Ok(mut client) = drift_core::niri::NiriClient::connect() {
            let _ = client.rename_workspace(project, &name);
        }
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
        #[cfg(feature = "dispatch")]
        let _ = self.dispatch_tx.send(event.clone());

        if matches!(priority, "critical" | "high" | "medium") {
            self.send_desktop_notification(&event);
        }

        #[cfg(feature = "dispatch")]
        match event.event_type.as_str() {
            "task.running" | "task.completed" | "task.failed" | "task.needs_review"
            | "task.queued" | "agent.completed" | "agent.error" | "service.crashed" => {
                if !event.project.is_empty() {
                    self.update_workspace_name(&event.project);
                }
            }
            _ => {}
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
            all_workspaces: self.workspaces.values()
                .map(|ws| {
                    let window_count = self.windows.values()
                        .filter(|w| w.workspace_id == Some(ws.id))
                        .count() as u32;
                    NiriWorkspace {
                        workspace_id: ws.id,
                        name: ws.name.clone(),
                        is_active: ws.is_active,
                        is_focused: ws.is_focused,
                        window_count,
                        project: self.workspace_to_project.get(&ws.id).cloned(),
                    }
                })
                .collect(),
            recent_events: self.events.iter()
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect(),
            focus: FocusState {
                mode: if self.active_project.is_some() { "workspace".into() } else { "overview".into() },
                active_project: self.active_project.clone(),
                niri_workspace_id: self.focused_workspace_id,
            },
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

    match std::process::Command::new("drift-commander")
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
    #[cfg(feature = "dispatch")]
    let dispatch_enabled = global_config.features.dispatch;

    let pid_path = paths::daemon_pid_path();
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&pid_path, std::process::id().to_string())?;

    let (msg_tx, msg_rx) = mpsc::channel::<DaemonMsg>();
    let (sub_tx, sub_rx) = mpsc::channel::<Event>();
    #[cfg(feature = "dispatch")]
    let (dispatch_tx, dispatch_rx) = mpsc::channel::<Event>();

    let global_persist_windows = global_config.defaults.persist_windows;
    #[cfg(feature = "dispatch")]
    let mut inner = DaemonInner::new(sub_tx, dispatch_tx, events_config.buffer_size, terminal_name, global_persist_windows);
    #[cfg(not(feature = "dispatch"))]
    let mut inner = DaemonInner::new(sub_tx, events_config.buffer_size, terminal_name, global_persist_windows);

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

    #[cfg(feature = "dispatch")]
    let dispatch_thread = if dispatch_enabled {
        Some(thread::Builder::new()
            .name("dispatch-watcher".into())
            .spawn(move || run_dispatch_watcher(dispatch_rx, &SHUTDOWN))?)
    } else {
        drop(dispatch_rx);
        None
    };

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
    #[cfg(feature = "dispatch")]
    if let Some(t) = dispatch_thread {
        let _ = t.join();
    }

    Ok(())
}

#[cfg(feature = "dispatch")]
fn run_dispatch_watcher(rx: mpsc::Receiver<Event>, shutdown: &AtomicBool) {
    let min_interval = Duration::from_secs(10);
    let max_tracked_projects = 100;
    let mut last_dispatch: HashMap<String, Instant> = HashMap::new();

    while !shutdown.load(Ordering::Relaxed) {
        let event = match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(e) => e,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        match event.event_type.as_str() {
            "task.completed" | "task.queued" => {}
            _ => continue,
        }

        let project = &event.project;
        if project.is_empty() {
            continue;
        }

        if let Some(last) = last_dispatch.get(project) {
            if last.elapsed() < min_interval {
                continue;
            }
        }

        if try_auto_dispatch(project) {
            last_dispatch.insert(project.clone(), Instant::now());

            // Prune stale entries to prevent unbounded growth
            if last_dispatch.len() > max_tracked_projects {
                last_dispatch.retain(|_, t| t.elapsed() < Duration::from_secs(3600));
            }
        }
    }
}

#[cfg(feature = "dispatch")]
fn try_auto_dispatch(project: &str) -> bool {
    use drift_core::tasks::{TaskQueue, TaskStatus};

    let project_config = match config::load_project_config(project) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let dispatcher = match project_config.dispatcher {
        Some(ref d) if d.auto_dispatch => d,
        _ => return false,
    };

    let queue = match TaskQueue::load(project) {
        Ok(q) => q,
        Err(_) => return false,
    };

    if dispatcher.review_gate_blocks && !queue.pending_reviews().is_empty() {
        eprintln!("auto-dispatch: {project} has pending reviews, skipping");
        return false;
    }

    let running = queue
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .count();
    if running >= dispatcher.max_concurrent_agents as usize {
        return false;
    }

    if queue.next().is_none() {
        return false;
    }

    eprintln!("auto-dispatch: dispatching next task for '{project}'");
    match std::process::Command::new("drift")
        .args(["dispatch", project])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(_) => true,
        Err(e) => {
            eprintln!("auto-dispatch: failed to spawn drift dispatch: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_inner() -> DaemonInner {
        let (sub_tx, _sub_rx) = mpsc::channel();
        #[cfg(feature = "dispatch")]
        let (dispatch_tx, _dispatch_rx) = mpsc::channel();
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
            global_persist_windows: false,
            subscriber_tx: sub_tx,
            #[cfg(feature = "dispatch")]
            dispatch_tx,
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
