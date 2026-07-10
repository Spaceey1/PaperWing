mod compositor_event;
mod niri;
pub mod state;
use core::fmt;
use std::{
    env,
    error::Error,
    io::ErrorKind,
    sync::{Arc, LazyLock},
};

pub use compositor_event::CompositorEvent;

use crate::state::Workspace;

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

#[derive(Debug)]
pub enum CompositorError {
    ConnectionError,
    NotRunning,
    Unknown,
}

impl fmt::Display for CompositorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompositorError::ConnectionError => {
                write!(f, "Connection to compositor failed")
            }
            CompositorError::NotRunning => write!(
                f,
                "No compatible wayland compositor was found. Is it running?"
            ),
            CompositorError::Unknown => write!(f, "Something went wrong and I don't know what"),
        }
    }
}

pub enum CompositorMessage {}

#[async_trait::async_trait]
trait Compositor: Sync + Send {
    fn is_running(&self) -> bool;
    async fn send_message(&self, msg: CompositorMessage) -> Result<String, CompositorError>;
    async fn start_event_stream(
        &self,
        callback: smol::channel::Sender<compositor_event::CompositorEvent>,
    ) -> Result<(), Box<dyn Error>>;
    async fn get_workspaces(&self) -> Result<Vec<Arc<Workspace>>, CompositorError>;
}

struct Niri;

#[async_trait::async_trait]
impl Compositor for Niri {
    fn is_running(&self) -> bool {
        env::var("NIRI_SOCKET").is_ok()
    }
    async fn send_message(&self, msg: CompositorMessage) -> Result<String, CompositorError> {
        match msg {}
    }
    async fn start_event_stream(
        &self,
        callback: smol::channel::Sender<compositor_event::CompositorEvent>,
    ) -> Result<(), Box<dyn Error>> {
        niri::connect_event_stream(callback).await
    }
    async fn get_workspaces(&self) -> Result<Vec<Arc<Workspace>>, CompositorError> {
        let response = niri::send_request("Workspaces".into())
            .await
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound => CompositorError::NotRunning,
                ErrorKind::TimedOut
                | ErrorKind::Deadlock
                | ErrorKind::AddrInUse
                | ErrorKind::BrokenPipe => CompositorError::ConnectionError,
                _ => CompositorError::Unknown,
            })
            .unwrap();
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
}
impl std::error::Error for HandlingError {}

static COMPOSITOR: LazyLock<Option<Arc<dyn Compositor>>> = LazyLock::new(|| {
    let mut compositors = Vec::<Arc<dyn Compositor>>::new();
    compositors.push(Arc::new(Niri {}));
    let Some(compositor) = compositors.iter().find(|c| c.is_running()) else {
        return None;
    };
    Some(compositor.clone())
});

macro_rules! get_compositor {
    () => {
        COMPOSITOR
            .clone()
            .ok_or_else(|| CompositorError::NotRunning)?
    };
}

pub async fn start_event_stream(
    callback: smol::channel::Sender<compositor_event::CompositorEvent>,
) -> Result<(), CompositorError> {
    println!("Starting!");
    get_compositor!()
        .start_event_stream(callback)
        .await
        .unwrap();
    Err(CompositorError::Unknown)
}
pub async fn send_message(msg: CompositorMessage) -> Result<String, CompositorError> {
    let comp = get_compositor!();
    comp.send_message(msg).await
}
pub async fn get_workspaces() -> Result<Vec<Arc<Workspace>>, CompositorError> {
    let comp = get_compositor!();
    comp.get_workspaces().await
}
