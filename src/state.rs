use std::{collections::HashMap, sync::Arc};

use compositor_bridge::state::{Window, Workspace};
use iced::{Animation, theme::Mode};
use rustsni::{ItemId, MenuNode, TrayItem};

pub struct AppState {
    pub workspaces: Vec<Arc<Workspace>>,
    pub focused_workspaces: Vec<usize>,
    pub focused_window: Option<Arc<Window>>,
    pub collapsed: Animation<bool>,
    pub menu_open: Animation<bool>,
    pub menu_items: Vec<MenuNode>,
    pub menu_id: Option<ItemId>,
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
            menu_items: vec![],
            tray_icons: HashMap::new(),
            collapsed: Animation::new(true)
                .quick()
                .easing(iced::animation::Easing::EaseInOut),
            menu_open: Animation::new(false)
                .quick()
                .easing(iced::animation::Easing::EaseInOut),
            menu_id: None,
            now: std::time::Instant::now(),
            mode: None,
            battery: battery::Manager::new().unwrap(),
        }
    }
}
