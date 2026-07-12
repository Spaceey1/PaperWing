use std::sync::Arc;

use compositor_bridge::state::{Window, Workspace};
use iced::{Animation, theme::Mode};

pub struct AppState {
    pub workspaces: Vec<Arc<Workspace>>,
    pub focused_window: Option<Arc<Window>>,
    pub focused_workspaces: Vec<usize>,
    pub collapsed: Animation<bool>,
    pub now: std::time::Instant,
    pub mode: Option<Mode>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            workspaces: vec![],
            focused_window: None,
            focused_workspaces: vec![],
            collapsed: Animation::new(true)
                .quick()
                .easing(iced::animation::Easing::EaseInOut),
            now: std::time::Instant::now(),
            mode: None,
        }
    }
}
