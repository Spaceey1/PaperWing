use std::sync::Arc;

use compositor_bridge::state::{Workspace, Window};

#[derive(Default)]
pub struct AppState {
    pub workspaces: Vec<Arc<Workspace>>,
    pub focused_window: Option<Arc<Window>>,
    pub focused_workspaces: Vec<usize>,
    pub collapsed: bool,
    pub collapsed_time: chrono::DateTime<chrono::Local>
}
