use std::{collections::HashMap, sync::Arc};

use compositor_bridge::{
    CompositorEvent,
    state::{Window, Workspace, WorkspaceId},
};
use iced::{Animation, theme::Mode};
use iced_layershell::to_layer_message;
use rustsni::{ItemId, MenuNode, TrayEvent, TrayItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarState {
    #[default]
    Collapsed,
    Peek,
    Expanded,
}

impl BarState {
    pub fn visual_height(self) -> f32 {
        match self {
            BarState::Collapsed => crate::consts::UP_TRAVEL as f32,
            BarState::Peek => crate::consts::PEEK_HEIGHT as f32,
            BarState::Expanded => crate::consts::WINDOW_HEIGHT as f32,
        }
    }

    pub fn window_height(self) -> u32 {
        match self {
            BarState::Collapsed => crate::consts::UP_TRAVEL + 1,
            BarState::Peek => crate::consts::PEEK_HEIGHT + 1,
            BarState::Expanded => crate::consts::WINDOW_HEIGHT,
        }
    }
}

impl iced::animation::Float for BarState {
    fn float_value(&self) -> f32 {
        self.visual_height()
    }
}

pub struct AppState {
    pub workspaces: Vec<Arc<Workspace>>,
    pub focused_workspaces: Vec<usize>,
    pub focused_window: Option<Arc<Window>>,
    pub bar_state: Animation<BarState>,
    pub menu_open: Animation<bool>,
    pub menu_items: Vec<MenuNode>,
    pub menu_id: Option<ItemId>,
    pub now: std::time::Instant,
    pub mode: Option<Mode>,
    pub battery: battery::Manager,
    pub tray_icons: HashMap<ItemId, TrayItem>,
    pub resize_pending: bool,
    pub peek_generation: u64,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            workspaces: vec![],
            focused_window: None,
            focused_workspaces: vec![],
            menu_items: vec![],
            tray_icons: HashMap::new(),
            bar_state: Animation::new(BarState::Collapsed)
                .quick()
                .easing(iced::animation::Easing::EaseOutQuad),
            menu_open: Animation::new(false)
                .quick()
                .easing(iced::animation::Easing::EaseInOut),
            menu_id: None,
            now: std::time::Instant::now(),
            mode: None,
            battery: battery::Manager::new().unwrap(),
            resize_pending: false,
            peek_generation: 0,
        }
    }
}

#[to_layer_message]
#[derive(Debug, Clone)]
pub enum Message {
    #[allow(clippy::enum_variant_names)]
    CompositorMessage(CompositorEvent),
    #[allow(clippy::enum_variant_names)]
    TrayMessage(TrayEvent),
    TrayAdded(ItemId, TrayItem),
    TrayPressed(ItemId),
    OpenTrayWith(Vec<MenuNode>, ItemId),
    MenuEntryPressed(i32),
    CloseTray,
    ToggleCollapse,
    AnimationUpdate,
    UnCollapse,
    Collapse,
    TriggerPeek,
    PeekTimeout(u64),
    MouseLeave,
    Refresh,
    FocusWorkspace(WorkspaceId),
}
