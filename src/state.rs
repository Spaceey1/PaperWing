use std::{collections::HashMap, sync::Arc};

use compositor_bridge::state::{Window, Workspace};
use iced::{Animation, theme::Mode};
use rustsni::{ItemId, TrayHost, TrayItem};

pub struct AppState {
    pub workspaces: Vec<Arc<Workspace>>,
    pub focused_workspaces: Vec<usize>,
    pub focused_window: Option<Arc<Window>>,
    pub collapsed: Animation<bool>,
    pub now: std::time::Instant,
    pub mode: Option<Mode>,
    pub battery: battery::Manager,
    pub tray_icons: HashMap<ItemId, TrayItem>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            workspaces: vec![],
            focused_window: None,
            focused_workspaces: vec![],
            tray_icons: HashMap::new(),
            collapsed: Animation::new(true)
                .quick()
                .easing(iced::animation::Easing::EaseInOut),
            now: std::time::Instant::now(),
            mode: None,
            battery: battery::Manager::new().unwrap(),
        }
    }
}
