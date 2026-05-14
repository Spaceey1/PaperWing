use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt,
    io::{BufRead, BufReader, Write},
    ops::Deref,
    os::unix::net::{self, UnixStream},
    sync::{Arc, Mutex, OnceLock, mpsc},
};

use smol::channel::Sender;

use crate::UiEvent;

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

#[derive(Debug)]
pub enum GetWindowError {
    NotFound,
    Unknown,
}

impl fmt::Display for GetWindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GetWindowError::Unknown => write!(f, "{}", "Something went wrong"),
            GetWindowError::NotFound => write!(f, "{}", "Window not found"),
        }
    }
}

impl std::error::Error for GetWindowError {}

#[enum_dispatch::enum_dispatch]
trait EventHandler {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError>;
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

#[derive(Serialize, Deserialize)]
pub struct WorkspaceActivated {
    pub id: i64,
    pub focused: bool,
}

impl EventHandler for WorkspaceActivated {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        Ok(()) // TODO
    }
}
#[derive(Serialize, Deserialize)]
pub struct WorkspaceActiveWindowChanged {
    pub workspace_id: i64,
    pub active_window_id: i64,
}

impl EventHandler for WorkspaceActiveWindowChanged {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        Ok(()) // TODO
    }
}
#[derive(Serialize, Deserialize)]
pub struct WindowFocusChanged {
    pub id: u64,
}

impl EventHandler for WindowFocusChanged {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        match get_window(&self.id) {
            Ok(window) => {
                event_channel
                    .send_blocking(UiEvent::FocusedWindowChanged(window))
                    .unwrap();
            }
            Err(GetWindowError::NotFound) => {
                // event_channel.send(UiEvent::FocusedWindowChanged("".to_string()));
                println!("Window with id {} not found", self.id);
                return Ok(());
            }
            Err(_) => return Err(HandlingError::Unknown),
        };
        Ok(()) // TODO
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FocusTimestamp {
    pub secs: i64,
    pub nanos: i64,
}

#[derive(Serialize, Deserialize)]
pub struct WindowOpenedOrChanged {
    window: Window,
}
impl EventHandler for WindowOpenedOrChanged {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        let window = track_window(self.window);
        event_channel.send_blocking(UiEvent::FocusedWindowChanged(window)).ok();
        Ok(())
    }
}
static WINDOWS: OnceLock<Mutex<HashMap<u64, Arc<Window>>>> = OnceLock::new();
fn get_windows() -> &'static Mutex<HashMap<u64, Arc<Window>>> {
    WINDOWS.get_or_init(|| Mutex::new(HashMap::new()))
}
#[derive(Serialize, Deserialize)]
pub struct WindowsChanged {
    pub windows: Vec<Window>,
}

impl EventHandler for WindowsChanged {
    fn handle(self, event_channel: Sender<UiEvent>) -> Result<(), HandlingError> {
        for window in self.windows.into_iter() {
            let window = track_window(window);
            if window.is_focused {
                event_channel
                    .send_blocking(UiEvent::FocusedWindowChanged(window.clone()))
                    .ok();
            }
        }
        Ok(())
    }
}

fn track_window(window: Window) -> Arc<Window> {
    let window = Arc::new(window);
    get_windows()
        .lock()
        .unwrap()
        .insert(window.id, window.clone());
    window
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Window {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub pid: u64,
    pub workspace_id: u64,
    pub is_focused: bool,
    pub is_floating: bool,
    pub is_urgent: bool,
    pub layout: Layout,
    pub focus_timestamp: FocusTimestamp,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Layout {
    pub pos_in_scrolling_layout: Vec<f32>,
    pub tile_size: Vec<f32>,
    pub window_size: Vec<f32>,
    pub tile_pos_in_workspace_view: Option<Vec<f32>>,
    pub window_offset_in_tile: Vec<f32>,
}

#[allow(clippy::large_enum_variant)]
#[enum_dispatch::enum_dispatch(EventHandler, event_channel)]
#[derive(Serialize, Deserialize)]
enum Event {
    WindowOpenedOrChanged(WindowOpenedOrChanged),
    WorkspaceActiveWindowChanged(WorkspaceActiveWindowChanged),
    WindowFocusChanged(WindowFocusChanged),
    WorkspaceActivated(WorkspaceActivated),
    WindowsChanged(WindowsChanged),
}

static REQUEST_SOCKET: OnceLock<UnixStream> = OnceLock::new();
fn get_socket() -> &'static UnixStream {
    REQUEST_SOCKET.get_or_init(|| {
        let sock_path = env::var("NIRI_SOCKET").expect("NIRI_SOCKET missing");
        UnixStream::connect(sock_path).expect("Connection failed")
    })
}

pub fn get_window(window_id: &u64) -> Result<Arc<Window>, GetWindowError> {
    let windows = get_windows().lock().map_err(|_| GetWindowError::Unknown)?;
    if windows.contains_key(window_id) {
        return Ok(windows[window_id].clone());
    };
    Err(GetWindowError::NotFound)
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
            Err(e) => {
                println!("{}", e);
                continue;
            }
        };
    }
}
