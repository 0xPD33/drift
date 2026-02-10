use anyhow::{bail, Context};
use niri_ipc::socket::Socket;
use niri_ipc::{Action, Request, Response, Window, Workspace, WorkspaceReferenceArg};

pub struct NiriClient {
    socket: Socket,
}

impl NiriClient {
    pub fn connect() -> anyhow::Result<Self> {
        let socket = Socket::connect().context("connecting to niri socket")?;
        Ok(Self { socket })
    }

    pub fn workspaces(&mut self) -> anyhow::Result<Vec<Workspace>> {
        let reply = self.socket.send(Request::Workspaces)?;
        match reply {
            Ok(Response::Workspaces(ws)) => Ok(ws),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn windows(&mut self) -> anyhow::Result<Vec<Window>> {
        let reply = self.socket.send(Request::Windows)?;
        match reply {
            Ok(Response::Windows(wins)) => Ok(wins),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn focused_window(&mut self) -> anyhow::Result<Option<Window>> {
        let reply = self.socket.send(Request::FocusedWindow)?;
        match reply {
            Ok(Response::FocusedWindow(win)) => Ok(win),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn find_workspace_by_name(&mut self, name: &str) -> anyhow::Result<Option<Workspace>> {
        let workspaces = self.workspaces()?;
        Ok(workspaces
            .into_iter()
            .find(|ws| ws.name.as_deref() == Some(name)))
    }

    pub fn focus_workspace(&mut self, name: &str) -> anyhow::Result<()> {
        let reply = self.socket.send(Request::Action(Action::FocusWorkspace {
            reference: WorkspaceReferenceArg::Name(name.to_string()),
        }))?;
        match reply {
            Ok(Response::Handled) => Ok(()),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn spawn(&mut self, command: Vec<String>) -> anyhow::Result<()> {
        let reply = self
            .socket
            .send(Request::Action(Action::Spawn { command }))?;
        match reply {
            Ok(Response::Handled) => Ok(()),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn close_window(&mut self, id: u64) -> anyhow::Result<()> {
        let reply = self
            .socket
            .send(Request::Action(Action::CloseWindow { id: Some(id) }))?;
        match reply {
            Ok(Response::Handled) => Ok(()),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn set_workspace_name(&mut self, name: &str) -> anyhow::Result<()> {
        let reply = self.socket.send(Request::Action(Action::SetWorkspaceName {
            name: name.to_string(),
            workspace: None,
        }))?;
        match reply {
            Ok(Response::Handled) => Ok(()),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn focus_workspace_down(&mut self) -> anyhow::Result<()> {
        let reply = self
            .socket
            .send(Request::Action(Action::FocusWorkspaceDown {}))?;
        match reply {
            Ok(Response::Handled) => Ok(()),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }

    pub fn create_named_workspace(&mut self, name: &str) -> anyhow::Result<()> {
        // Focus past the last workspace to create a new empty one, then name it
        let workspaces = self.workspaces()?;
        let max_idx = workspaces.len();
        // Focus workspace beyond the last to create a new one
        for _ in 0..max_idx {
            self.focus_workspace_down()?;
        }
        self.set_workspace_name(name)?;
        Ok(())
    }

    pub fn unset_workspace_name(&mut self, name: &str) -> anyhow::Result<()> {
        let reply =
            self.socket
                .send(Request::Action(Action::UnsetWorkspaceName {
                    reference: Some(WorkspaceReferenceArg::Name(name.to_string())),
                }))?;
        match reply {
            Ok(Response::Handled) => Ok(()),
            Ok(other) => bail!("unexpected response: {other:?}"),
            Err(msg) => bail!("niri error: {msg}"),
        }
    }
}
