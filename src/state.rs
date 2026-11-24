use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use compositor_bridge::{
    CompositorEvent,
    state::{Window, Workspace, WorkspaceId},
};
use iced::{Animation, theme::Mode, widget::image::Handle};
use iced_layershell::to_layer_message;
use media::{MprisMetadata, Player};
use tray_server::{ItemId, MenuNode, TrayEvent, TrayItem};

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
    pub batteries: Vec<BatteryInfo>,
    pub tray_icons: HashMap<ItemId, TrayItem>,
    pub resize_pending: bool,
    pub peek_generation: u64,
    pub persistent_peek: bool,
    pub players: BTreeMap<String, PlayerState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryStatus {
    Unknown,
    Charging,
    Discharging,
    Empty,
    Full,
}

#[derive(Debug, Clone, Copy)]
pub struct BatteryInfo {
    /// Charge level, from `0.0` to `1.0`.
    pub level: f32,
    pub status: BatteryStatus,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub player: Player,
    pub metadata: Option<MprisMetadata>,
    pub volume: Option<f32>,
    /// Artwork handle. Keeping the handle (instead of the raw bytes) means
    /// rebuilding the view reuses the renderer's texture instead of decoding again.
    pub image: Option<Handle>,
    /// URL the current `image` belongs to, used to avoid redundant downloads.
    pub image_url: Option<String>,
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
            batteries: vec![],
            resize_pending: false,
            peek_generation: 0,
            persistent_peek: false,
            players: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum MediaControl {
    TogglePlay,
    Skip,
    Rewind,
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
    Batteries(Vec<BatteryInfo>),
    UnCollapse,
    Collapse,
    Peek,
    TimedPeek,
    PeekTimeout(u64),
    MouseLeave,
    Refresh,
    FocusWorkspace(WorkspaceId),
    PlayerAdded(PlayerState),
    PlayerRemoved(String),
    MediaChanged(PlayerState),
    VolumeChanged(String, Option<f32>),
    NewImage(String, String, Option<Vec<u8>>),
    MediaControl(String, MediaControl),
}
