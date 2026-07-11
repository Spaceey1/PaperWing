use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{Arc, LazyLock, RwLock},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Window {
    pub id: usize,
    pub title: String,
    pub app_id: String,
    pub pid: u32,
    pub workspace_id: usize,
    pub is_focused: bool,
    pub is_floating: bool,
    pub is_urgent: bool,
    pub layout: Layout,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Layout {
    pub pos_in_scrolling_layout: Vec<f32>,
    pub tile_size: Vec<f32>,
    pub window_size: Vec<f32>,
    pub tile_pos_in_workspace_view: Option<Vec<f32>>,
    pub window_offset_in_tile: Vec<f32>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Workspace {
    pub id: usize,
    pub idx: usize,
    pub name: Option<String>,
    pub output: String,
    pub is_urgent: bool,
    pub is_active: bool,
    pub is_focused: bool,
    pub active_window_id: Option<usize>,
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

static WINDOWS: RwLock<LazyLock<HashMap<usize, Arc<Window>>>> =
    RwLock::new(LazyLock::new(HashMap::new));
pub fn get_window(window_id: &usize) -> Result<Arc<Window>, GetResourceError> {
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

static WORKSPACES: RwLock<LazyLock<HashMap<usize, Arc<Workspace>>>> =
    RwLock::new(LazyLock::new(HashMap::new));
pub fn get_workspace(workspace_id: &usize) -> Result<Arc<Workspace>, GetResourceError> {
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
