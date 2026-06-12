use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt,
    io::{BufRead, BufReader, Write},
    sync::{Arc, OnceLock},
};

use crate::ui::event::*;
use crate::ui::state::*;
use smol::{channel::Sender, future::FutureExt, lock::Mutex};
use std::os::unix::net::UnixStream;

#[derive(Debug)]
pub enum HandlingError {
    ConnectionError,
    BadData,
    Unknown,
}

impl fmt::Display for HandlingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HandlingError::ConnectionError => {
                write!(f, "Connection with niri failed while handling event")
            }
            HandlingError::BadData => write!(f, "Event data was bad"),
            HandlingError::Unknown => write!(f, "Something went wrong"),
        }
    }
}

impl std::error::Error for HandlingError {}

#[enum_dispatch::enum_dispatch]
trait EventHandler {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError>;
}

#[derive(Serialize, Deserialize)]
pub struct Output {
    name: String,
    make: String,
    model: String,
    serial: String,
    physical_size: Vec<i64>,
    modes: Vec<Mode>,
    current_mode: i64,
    is_custom_mode: bool,
    vrr_supported: bool,
    vrr_enabled: bool,
    logical: Logical,
}

#[derive(Serialize, Deserialize)]
pub struct Logical {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    scale: i64,
    transform: String,
}

#[derive(Serialize, Deserialize)]
pub struct Mode {
    width: i64,
    height: i64,
    refresh_rate: i64,
    is_preferred: bool,
}

#[derive(Serialize, Deserialize)]
pub struct WorkspaceActivated {
    pub id: u64,
    pub focused: bool,
}
impl EventHandler for WorkspaceActivated {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        if !self.focused {
            return Ok(());
        };
        event_channel
            .send_blocking(UiEvent::WorkspaceFocusChanged(self.id))
            .map_err(|_| HandlingError::ConnectionError)
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowFocusChanged {
    pub id: Option<u64>,
}
impl EventHandler for WindowFocusChanged {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        fn send_empty(event_channel: Sender<UiEvent>) {
            event_channel
                .send_blocking(UiEvent::FocusedWindowChanged(Arc::new(Window {
                    title: "".to_string(),
                    ..Default::default()
                })))
                .ok();
        }
        match self.id {
            Some(id) => {
                match get_window(&id) {
                    Ok(window) => {
                        event_channel
                            .send_blocking(UiEvent::FocusedWindowChanged(window))
                            .unwrap();
                    }
                    Err(GetResourceError::NotFound) => {
                        send_empty(event_channel);
                        println!("Window with id {} not found", id);
                        return Ok(());
                    }
                    Err(_) => return Err(HandlingError::Unknown),
                };
            }
            None => {
                send_empty(event_channel);
            }
        }
        Ok(()) // TODO
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowOpenedOrChanged {
    window: Window,
}
impl EventHandler for WindowOpenedOrChanged {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        let window = track_window(self.window).map_err(|_| HandlingError::Unknown)?;
        if window.is_focused {
            event_channel
                .send_blocking(UiEvent::FocusedWindowChanged(window))
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
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        for window in self.windows.into_iter() {
            let window = track_window(window).map_err(|_| HandlingError::Unknown)?;
            if window.is_focused {
                event_channel
                    .send_blocking(UiEvent::FocusedWindowChanged(window.clone()))
                    .ok();
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowClosed {
    pub id: u64,
}
impl EventHandler for WindowClosed {
    fn handle(self, _event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
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
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        let workspaces: Vec<Arc<Workspace>> = self.workspaces.into_iter().map(Arc::new).collect();
        let mut iter = workspaces.into_iter();

        set_workspaces(iter.clone()).map_err(|_| HandlingError::Unknown)?;
        event_channel
            .send_blocking(UiEvent::WorkspacesChanged(Box::new(iter.clone())))
            .map_err(|_| HandlingError::ConnectionError)?;
        let Some(focused_workspace) = iter.find(|x| x.is_focused) else {
            return Err(HandlingError::BadData);
        };
        event_channel
            .send_blocking(UiEvent::WorkspaceFocusChanged(focused_workspace.id))
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

static REQUEST_SOCKET: OnceLock<UnixStream> = OnceLock::new();
fn get_socket() -> &'static UnixStream {
    REQUEST_SOCKET.get_or_init(|| {
        let sock_path = env::var("NIRI_SOCKET").expect("NIRI_SOCKET missing");
        UnixStream::connect(sock_path).expect("niri connection failed")
    })
}

pub fn send_request(msg: &[u8]) -> std::io::Result<String> {
    let mut sock = get_socket();
    sock.write_all(msg)?;
    sock.flush()?;
    let mut reader = BufReader::new(sock);
    let mut result = String::new();
    reader.read_line(&mut result)?;
    Ok(result)
}

pub fn connect_event_stream(
    event_channel: smol::channel::Sender<UiEvent>,
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
                if event.handle(event_channel.clone()).is_err() {
                    continue;
                }
            }
            Err(ref e) if e.is_data() => {}
            Err(e) => eprintln!("{}", e),
        };
    }
}
