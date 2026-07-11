use std::sync::Arc;

use compositor_bridge::state::{Window, Workspace};
use iced::theme::Mode;

pub struct AppState {
    pub workspaces: Vec<Arc<Workspace>>,
    pub focused_window: Option<Arc<Window>>,
    pub focused_workspaces: Vec<usize>,
    pub collapsed: bool,
    pub collapsed_time: std::time::Instant,
    pub mode: Option<Mode>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            workspaces: vec![],
            focused_window: None,
            focused_workspaces: vec![],
            collapsed: true,
            collapsed_time: std::time::Instant::now(),
            mode: None,
        }
    }
}
