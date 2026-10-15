use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{Arc, LazyLock, RwLock},
};

use serde::{Deserialize, Serialize};

pub type WindowId = usize;

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Window {
    #[serde(alias = "address", deserialize_with = "deserialize_id")]
    pub id: WindowId,
    pub title: String,
    #[serde(alias = "class")]
    pub app_id: String,
    pub pid: u32,
    #[serde(alias = "workspace", deserialize_with = "deserialize_workspace_id")]
    pub workspace_id: usize,
    pub is_focused: bool,
    #[serde(alias = "floating")]
    pub is_floating: bool,
    pub is_urgent: bool,
    pub layout: Layout,
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Layout {
    pub pos_in_scrolling_layout: Vec<f32>,
    pub tile_size: Vec<f32>,
    pub window_size: Vec<f32>,
    pub tile_pos_in_workspace_view: Option<Vec<f32>>,
    pub window_offset_in_tile: Vec<f32>,
}

pub type WorkspaceId = usize;

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Workspace {
    pub id: WorkspaceId,
    /// idx is the "number" of the workspace; for example when user "goes to workspace 1" they go to
    /// workspace with idx: 1. This is not necessarely the actual id of the workspace it and doesn't
    /// have to be unique.
    pub idx: usize,
    /// Similar to idx
    pub name: Option<String>,
    /// The physical output; a monitor, display, screen, vr headset, or whatever the compositor deems suitable
    /// to show windows on
    #[serde(alias = "monitor")]
    pub output: String,
    pub is_urgent: bool,
    pub is_active: bool,
    pub is_focused: bool,
    #[serde(alias = "lastwindow", deserialize_with = "deserialize_optional_id")]
    pub active_window_id: Option<WindowId>,
}

fn deserialize_id<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct IdVisitor;

    impl<'de> serde::de::Visitor<'de> for IdVisitor {
        type Value = usize;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an integer or hexadecimal address")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            usize::try_from(value).map_err(E::custom)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            usize::try_from(value).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            usize::from_str_radix(value.trim_start_matches("0x"), 16).map_err(E::custom)
        }
    }

    deserializer.deserialize_any(IdVisitor)
}

fn deserialize_optional_id<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct OptionalIdVisitor;

    impl<'de> serde::de::Visitor<'de> for OptionalIdVisitor {
        type Value = Option<usize>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an optional integer or hexadecimal address")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            usize::try_from(value).map(Some).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value.is_empty() || value == "0x0" {
                Ok(None)
            } else {
                usize::from_str_radix(value.trim_start_matches("0x"), 16)
                    .map(Some)
                    .map_err(E::custom)
            }
        }
    }

    deserializer.deserialize_any(OptionalIdVisitor)
}

fn deserialize_workspace_id<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct WorkspaceIdVisitor;

    impl<'de> serde::de::Visitor<'de> for WorkspaceIdVisitor {
        type Value = usize;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a workspace id or an object containing one")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            usize::try_from(value).map_err(E::custom)
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut id = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "id" {
                    id = Some(map.next_value()?);
                } else {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
            id.ok_or_else(|| serde::de::Error::missing_field("id"))
        }
    }

    deserializer.deserialize_any(WorkspaceIdVisitor)
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Output {
    pub name: String,
    pub make: String,
    pub modes: Vec<Mode>,
    pub current_mode: usize,
    pub vrr_supported: bool,
    pub vrr_enabled: bool,
    pub logical: Logical,
    pub focused: bool,
    #[serde(rename = "activeWorkspace")]
    pub active_workspace: Workspace,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Logical {
    /// Logical x position
    pub x: usize,
    /// Logical y position
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub scale: usize,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Mode {
    pub width: usize,
    pub height: usize,
    pub refresh_rate: usize,
    pub is_preferred: bool,
}

#[derive(Debug)]
pub enum GetResourceError {
    NotFound,
    Unknown,
}

impl fmt::Display for GetResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GetResourceError::Unknown => write!(f, "Something went wrong"),
            GetResourceError::NotFound => write!(f, "Window not found"),
        }
    }
}

impl std::error::Error for GetResourceError {}

static WINDOWS: RwLock<LazyLock<HashMap<WindowId , Arc<Window>>>> =
    RwLock::new(LazyLock::new(HashMap::new));
pub fn get_window(window_id: &WindowId) -> Result<Arc<Window>, GetResourceError> {
    let windows = WINDOWS.read().map_err(|_| GetResourceError::Unknown)?;
    if windows.contains_key(window_id) {
        return Ok(windows[window_id].clone());
    };
    Err(GetResourceError::NotFound)
}
pub fn track_window(window: Window) -> Result<Arc<Window>, Box<dyn Error>> {
    let mut windows = WINDOWS.write()?;
    let window = Arc::new(window);
    windows.insert(window.id, window.clone());
    Ok(window)
}
pub fn remove_window(id: &usize) -> Result<Option<Arc<Window>>, Box<dyn Error>> {
    let mut windows = WINDOWS.write()?;
    Ok(windows.remove(id))
}

static WORKSPACES: RwLock<LazyLock<HashMap<WorkspaceId, Arc<Workspace>>>> =
    RwLock::new(LazyLock::new(HashMap::new));
pub fn get_workspace(workspace_id: &WorkspaceId) -> Result<Arc<Workspace>, GetResourceError> {
    let workspaces = WORKSPACES.read().map_err(|_| GetResourceError::Unknown)?;
    if workspaces.contains_key(workspace_id) {
        return Ok(workspaces[workspace_id].clone());
    };
    Err(GetResourceError::NotFound)
}
pub fn set_workspaces<I>(new_workspaces: I) -> Result<(), Box<dyn Error>>
where
    I: IntoIterator<Item = Arc<Workspace>>,
{
    let mut workspaces = WORKSPACES.write()?;
    workspaces.clear();
    for new_workspace in new_workspaces {
        workspaces.insert(new_workspace.id, new_workspace);
    }
    Ok(())
}
