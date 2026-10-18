#![allow(dead_code)]
use std::{env, error::Error, path::PathBuf, str::FromStr, sync::Arc};

use smol::{
    channel::Sender,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::unix::UnixStream,
};

use crate::{
    Compositor, CompositorError, HandlingError,
    compositor_event::CompositorEvent,
    state::{Output, Window, Workspace, WorkspaceId},
};

pub struct Hyprland;

#[async_trait::async_trait]
impl Compositor for Hyprland {
    fn is_running(&self) -> bool {
        env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
    }

    async fn start_event_stream(
        &self,
        callback: Sender<CompositorEvent>,
    ) -> Result<(), Box<dyn Error>> {
        let socket = UnixStream::connect(socket_path(".socket2.sock")?).await?;
        let mut reader = BufReader::new(socket);
        let mut line = String::new();

        callback
            .send(CompositorEvent::WorkspacesChanged(
                self.get_workspaces()
                    .await
                    .map_err(|error| std::io::Error::other(error.to_string()))?,
            ))
            .await?;
        callback
            .send(CompositorEvent::WorkspaceFocusChanged())
            .await?;
        callback
            .send(CompositorEvent::FocusedWindowChanged(
                get_active_window()
                    .await
                    .map_err(|error| std::io::Error::other(error.to_string()))?,
            ))
            .await?;

        loop {
            line.clear();
            if reader.read_line(&mut line).await? == 0 {
                return Err("Hyprland event socket closed".into());
            }

            let Ok(event) = HyprlandEvent::from_str(line.trim_end()) else {
                continue;
            };
            if event.handle(&callback).await.is_err() {
                continue;
            }
        }
    }

    async fn get_workspaces(&self) -> Result<Vec<Arc<Workspace>>, CompositorError> {
        let mut workspaces: Vec<Workspace> = serde_json::from_str(&request("j/workspaces").await?)
            .map_err(|error| {
                eprintln!("{error}");
                CompositorError::Unknown
            })?;
        let monitors: Vec<Output> =
            serde_json::from_str(&request("j/monitors").await?).map_err(|error| {
                eprintln!("{error}");
                CompositorError::Unknown
            })?;

        for workspace in &mut workspaces {
            workspace.idx = workspace
                .name
                .as_deref()
                .and_then(|name| name.parse().ok())
                .unwrap_or(workspace.id);
            let monitor = monitors
                .iter()
                .find(|monitor| monitor.name == workspace.output);
            workspace.is_active =
                monitor.is_some_and(|monitor| monitor.active_workspace.id == workspace.id);
            workspace.is_focused =
                monitor.is_some_and(|monitor| monitor.focused) && workspace.is_active;
        }

        Ok(workspaces.into_iter().map(Arc::new).collect())
    }

    async fn go_to_workspace(&self, id: &WorkspaceId) -> Result<(), CompositorError> {
        let _ = request(&format!(
            "j/eval hl.dispatch(hl.dsp.focus({{ workspace = \"{id}\" }}))"
        ))
        .await?;
        Ok(())
    }
}

fn socket_path(socket: &str) -> Result<PathBuf, env::VarError> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")?;
    let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")?;
    Ok(PathBuf::from(runtime_dir)
        .join("hypr")
        .join(signature)
        .join(socket))
}

async fn request(message: &str) -> Result<String, CompositorError> {
    let path = socket_path(".socket.sock").map_err(|_| CompositorError::NotRunning)?;
    let mut socket = UnixStream::connect(path)
        .await
        .map_err(|_| CompositorError::ConnectionError)?;
    socket
        .write_all(message.as_bytes())
        .await
        .map_err(|_| CompositorError::ConnectionError)?;
    socket
        .flush()
        .await
        .map_err(|_| CompositorError::ConnectionError)?;

    let mut response = String::new();
    socket
        .read_to_string(&mut response)
        .await
        .map_err(|_| CompositorError::ConnectionError)?;
    Ok(response)
}

async fn get_active_window() -> Result<Arc<Window>, CompositorError> {
    let mut window: Window = serde_json::from_str(&request("j/activewindow").await?)
        .map_err(|_| CompositorError::Unknown)?;
    window.is_focused = window.id != 0;
    Ok(Arc::new(window))
}

#[enum_dispatch::enum_dispatch]
trait HyprlandEventHandler {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError>;
}

struct WorkspaceV2 {
    id: WorkspaceId,
    name: String,
}

impl HyprlandEventHandler for WorkspaceV2 {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        event_channel
            .send(CompositorEvent::WorkspaceFocusChanged())
            .await
            .map_err(|_| HandlingError::ConnectionError)
    }
}

struct FocusedMonV2 {
    monitor: String,
    workspace_id: WorkspaceId,
}

impl HyprlandEventHandler for FocusedMonV2 {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        event_channel
            .send(CompositorEvent::WorkspaceFocusChanged())
            .await
            .map_err(|_| HandlingError::ConnectionError)
    }
}

async fn send_workspaces_changed(
    event_channel: &Sender<CompositorEvent>,
) -> Result<(), HandlingError> {
    let workspaces = Hyprland {}
        .get_workspaces()
        .await
        .map_err(|_| HandlingError::Unknown)?;
    event_channel
        .send(CompositorEvent::WorkspacesChanged(workspaces))
        .await
        .map_err(|_| HandlingError::ConnectionError)
}

struct CreateWorkspaceV2 {
    id: WorkspaceId,
    name: String,
}

struct DestroyWorkspaceV2 {
    id: WorkspaceId,
    name: String,
}

struct MoveWorkspaceV2 {
    id: WorkspaceId,
    name: String,
    monitor: String,
}

struct RenameWorkspace {
    id: WorkspaceId,
    name: String,
}

impl HyprlandEventHandler for CreateWorkspaceV2 {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        send_workspaces_changed(event_channel).await
    }
}

impl HyprlandEventHandler for DestroyWorkspaceV2 {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        send_workspaces_changed(event_channel).await
    }
}

impl HyprlandEventHandler for MoveWorkspaceV2 {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        send_workspaces_changed(event_channel).await
    }
}

impl HyprlandEventHandler for RenameWorkspace {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        send_workspaces_changed(event_channel).await
    }
}

async fn send_focused_window_changed(
    event_channel: &Sender<CompositorEvent>,
) -> Result<(), HandlingError> {
    let window = get_active_window()
        .await
        .map_err(|_| HandlingError::Unknown)?;
    event_channel
        .send(CompositorEvent::FocusedWindowChanged(window))
        .await
        .map_err(|_| HandlingError::ConnectionError)
}

struct ActiveWindowV2 {
    address: String,
}

struct WindowTitleV2 {
    address: String,
    title: String,
}

impl HyprlandEventHandler for ActiveWindowV2 {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        send_focused_window_changed(event_channel).await
    }
}

impl HyprlandEventHandler for WindowTitleV2 {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        send_focused_window_changed(event_channel).await
    }
}

#[enum_dispatch::enum_dispatch(HyprlandEventHandler)]
enum HyprlandEvent {
    WorkspaceV2(WorkspaceV2),
    FocusedMonV2(FocusedMonV2),
    CreateWorkspaceV2(CreateWorkspaceV2),
    DestroyWorkspaceV2(DestroyWorkspaceV2),
    MoveWorkspaceV2(MoveWorkspaceV2),
    RenameWorkspace(RenameWorkspace),
    ActiveWindowV2(ActiveWindowV2),
    WindowTitleV2(WindowTitleV2),
}

impl FromStr for HyprlandEvent {
    type Err = ();

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let (name, data) = line.split_once(">>").ok_or(())?;
        Ok(match name {
            "workspacev2" => {
                let (id, name) = parse_workspace(data)?;
                Self::WorkspaceV2(WorkspaceV2 { id, name })
            }
            "focusedmonv2" => {
                let (monitor, workspace_id) = data.split_once(',').ok_or(())?;
                Self::FocusedMonV2(FocusedMonV2 {
                    monitor: monitor.to_string(),
                    workspace_id: workspace_id.parse().map_err(|_| ())?,
                })
            }
            "createworkspacev2" => {
                let (id, name) = parse_workspace(data)?;
                Self::CreateWorkspaceV2(CreateWorkspaceV2 { id, name })
            }
            "destroyworkspacev2" => {
                let (id, name) = parse_workspace(data)?;
                Self::DestroyWorkspaceV2(DestroyWorkspaceV2 { id, name })
            }
            "moveworkspacev2" => {
                let mut fields = data.splitn(3, ',');
                Self::MoveWorkspaceV2(MoveWorkspaceV2 {
                    id: fields.next().ok_or(())?.parse().map_err(|_| ())?,
                    name: fields.next().ok_or(())?.to_string(),
                    monitor: fields.next().ok_or(())?.to_string(),
                })
            }
            "renameworkspace" => {
                let (id, name) = parse_workspace(data)?;
                Self::RenameWorkspace(RenameWorkspace { id, name })
            }
            "activewindowv2" => Self::ActiveWindowV2(ActiveWindowV2 {
                address: data.to_string(),
            }),
            "windowtitlev2" => {
                let (address, title) = data.split_once(',').ok_or(())?;
                Self::WindowTitleV2(WindowTitleV2 {
                    address: address.to_string(),
                    title: title.to_string(),
                })
            }
            _ => return Err(()),
        })
    }
}

fn parse_workspace(data: &str) -> Result<(WorkspaceId, String), ()> {
    let (id, name) = data.split_once(',').ok_or(())?;
    Ok((id.parse().map_err(|_| ())?, name.to_string()))
}
