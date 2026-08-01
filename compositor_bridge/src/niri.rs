use crate::compositor_event::CompositorEvent;
use crate::state::*;
use crate::{Compositor, CompositorError, HandlingError, compositor_event, io_err_to_comp};
use serde::{Deserialize, Serialize};
use smol::channel::Sender;
use std::ops::Deref;
use std::os::unix::net::UnixStream;
use std::sync::LazyLock;
use std::{
    env,
    error::Error,
    io::{BufRead, BufReader, Write},
    sync::Arc,
};

pub struct Niri;

#[async_trait::async_trait]
impl Compositor for Niri {
    fn is_running(&self) -> bool {
        env::var("NIRI_SOCKET").is_ok()
    }
    async fn start_event_stream(
        &self,
        callback: smol::channel::Sender<compositor_event::CompositorEvent>,
    ) -> Result<(), Box<dyn Error>> {
        connect_event_stream(callback).await
    }
    async fn get_workspaces(&self) -> Result<Vec<Arc<Workspace>>, CompositorError> {
        let response = io_err_to_comp(send_request(&"Workspaces".into()).await)?;
        let workspaces: Vec<Arc<Workspace>> = serde_json::from_str::<serde_json::Value>(&response)
            .map_err(|e| {
                eprintln!("JSON syntax error: {}", e);
                CompositorError::Unknown
            })
            .and_then(|value| {
                if let Some(ok_val) = value.get("Ok").and_then(|ok| ok.get("Workspaces")) {
                    serde_json::from_value::<Vec<Workspace>>(ok_val.clone()).map_err(|e| {
                        eprintln!("Failed to parse ok body: {}", e);
                        CompositorError::Unknown
                    })
                } else {
                    let err_detail = value.get("Err").unwrap_or(&value);
                    eprintln!("API returned error: {}", err_detail);
                    Err(CompositorError::Unknown)
                }
            })?
            .into_iter()
            .map(Arc::new)
            .collect();
        Ok(workspaces)
    }
    async fn go_to_workspace(&self, id: &WorkspaceId) -> Result<(), CompositorError> {
        io_err_to_comp(
            send_action(&format!(
                "\"FocusWorkspace\": {{\"reference\": {{\"Id\": {}}}}}",
                id
            ))
            .await,
        )?;
        Ok(())
    }
}

#[enum_dispatch::enum_dispatch]
trait EventHandler {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError>;
}

#[derive(Serialize, Deserialize)]
pub struct WorkspaceActivated {
    pub id: usize,
    pub focused: bool,
}
impl EventHandler for WorkspaceActivated {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        if !self.focused {
            return Ok(());
        };
        event_channel
            .send(CompositorEvent::WorkspaceFocusChanged())
            .await
            .map_err(|_| HandlingError::ConnectionError)
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowFocusChanged {
    pub id: Option<usize>,
}
impl EventHandler for WindowFocusChanged {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        async fn send_empty(event_channel: &Sender<CompositorEvent>) {
            event_channel
                .send(CompositorEvent::FocusedWindowChanged(Arc::new(Window {
                    title: "".to_string(),
                    ..Default::default()
                })))
                .await
                .unwrap()
        }
        match self.id {
            Some(id) => {
                match get_window(&id) {
                    Ok(window) => {
                        event_channel
                            .send(CompositorEvent::FocusedWindowChanged(window))
                            .await
                            .unwrap();
                    }
                    Err(GetResourceError::NotFound) => {
                        send_empty(event_channel).await;
                        eprintln!("Window with id {} not found", id);
                        return Ok(());
                    }
                    Err(_) => return Err(HandlingError::Unknown),
                };
            }
            None => {
                send_empty(event_channel).await;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowOpenedOrChanged {
    window: Window,
}
impl EventHandler for WindowOpenedOrChanged {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        let window = track_window(self.window).map_err(|_| HandlingError::Unknown)?;
        if window.is_focused {
            event_channel
                .send(CompositorEvent::FocusedWindowChanged(window))
                .await
                .ok();
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowsChanged {
    pub windows: Vec<Window>,
}
impl EventHandler for WindowsChanged {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        for window in self.windows.into_iter() {
            let window = track_window(window).map_err(|_| HandlingError::Unknown)?;
            if window.is_focused {
                event_channel
                    .send(CompositorEvent::FocusedWindowChanged(window.clone()))
                    .await
                    .ok();
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowClosed {
    pub id: usize,
}
impl EventHandler for WindowClosed {
    async fn handle(self, _event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        match remove_window(&self.id) {
            Ok(removed_window) => match removed_window {
                Some(_) => Ok(()),
                None => Err(HandlingError::BadData),
            },
            Err(_) => Err(HandlingError::Unknown),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WorkspacesChanged {
    pub workspaces: Vec<Workspace>,
}

impl EventHandler for WorkspacesChanged {
    async fn handle(self, event_channel: &Sender<CompositorEvent>) -> Result<(), HandlingError> {
        let workspaces: Vec<Arc<Workspace>> = self.workspaces.into_iter().map(Arc::new).collect();
        let iter = workspaces.clone().into_iter(); // Cloning vec of arcs is fine

        set_workspaces(iter.clone()).map_err(|_| HandlingError::Unknown)?;
        event_channel
            .send(CompositorEvent::WorkspacesChanged(workspaces))
            .await
            .map_err(|_| HandlingError::ConnectionError)?;
        event_channel
            .send(CompositorEvent::WorkspaceFocusChanged())
            .await
            .map_err(|_| HandlingError::ConnectionError)
    }
}

#[allow(clippy::large_enum_variant)]
#[enum_dispatch::enum_dispatch(EventHandler, event_channel)]
#[derive(Serialize, Deserialize)]
enum Event {
    WindowOpenedOrChanged(WindowOpenedOrChanged),
    WindowFocusChanged(WindowFocusChanged),
    WorkspaceActivated(WorkspaceActivated),
    WindowsChanged(WindowsChanged),
    WindowClosed(WindowClosed),
    WorkspacesChanged(WorkspacesChanged),
}
impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string(&self).unwrap_or("Invalid event".to_string())
        )
    }
}

static REQUEST_SOCKET: LazyLock<UnixStream> = LazyLock::new(|| {
    let sock_path = env::var("NIRI_SOCKET").expect("NIRI_SOCKET missing");
    UnixStream::connect(sock_path).expect("niri connection failed")
});

pub async fn send_request(msg: &String) -> std::io::Result<String> {
    send_raw_request(format!("\"{}\"\n", msg)).await
}

async fn send_raw_request(msg: String) -> std::io::Result<String> {
    let mut sock = REQUEST_SOCKET.deref();
    sock.write_all(&msg.into_bytes())?;
    sock.flush()?;
    let mut reader = BufReader::new(sock);
    let mut result = String::new();
    reader.read_line(&mut result)?;
    Ok(result)
}

pub async fn send_action(msg: &String) -> std::io::Result<String> {
    let r = format!("{{\"Action\": {{{}}}}}\n", msg); // I'm simply too lazy to make a struct just for this
    send_raw_request(r).await
}

pub async fn connect_event_stream(
    event_channel: smol::channel::Sender<CompositorEvent>,
) -> Result<(), Box<dyn Error>> {
    let sock_path = env::var("NIRI_SOCKET")?;
    let mut socket = std::os::unix::net::UnixStream::connect(sock_path)?;
    let socket2 = socket.try_clone()?;
    let mut reader = BufReader::new(socket2);
    socket.write_all(b"\"EventStream\"\n")?;
    socket.flush()?;
    loop {
        let mut buf = String::new();
        if reader.read_line(&mut buf).is_err() {
            continue;
        };
        match serde_json::from_str::<Event>(&buf) {
            Ok(event) => {
                if event.handle(&event_channel).await.is_err() {
                    continue;
                }
            }
            Err(ref e) if e.is_data() => {}
            Err(e) => eprintln!("{}", e),
        };
    }
}
