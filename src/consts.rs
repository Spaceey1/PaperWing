use crate::package_name;
pub const APP_NAME: &str = package_name!();
pub const COLLAPSE_TIME: chrono::TimeDelta = chrono::TimeDelta::milliseconds(100);
pub const UP_TRAVEL: u32 = 270;
pub const WINDOW_WIDTH: u32 = 400;
pub const WINDOW_HEIGHT: u32 = 300;
pub const RADIUS: u32 = 8;
pub const MARGINS: u32 = 10;
