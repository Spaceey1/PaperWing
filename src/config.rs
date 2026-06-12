use std::{
    fs::File, io::Write, ops::Deref, path::PathBuf, sync::LazyLock
};

use crate::consts::APP_NAME;
use directories::ProjectDirs;
use gpui::accesskit::Uuid;

static CONFIG_DIR: std::sync::LazyLock<ProjectDirs> =
    std::sync::LazyLock::new(|| ProjectDirs::from("", "", APP_NAME).unwrap());
pub static CONFIG: std::sync::LazyLock<std::sync::RwLock<Config>> =
    LazyLock::new(|| std::sync::RwLock::new(load_config()));

fn get_config_path() -> PathBuf {
    CONFIG_DIR.config_dir().join("config.json")
}

fn load_config() -> Config {
    let config_path = get_config_path();
    if !std::fs::exists(&config_path).unwrap() {
        return Config {
            ..Default::default()
        };
    }
    let config_file = File::open(config_path);
    match config_file {
        Ok(config_file) => {
            serde_json::from_reader::<_, Config>(std::io::BufReader::new(config_file))
                .expect("Config is malformed")
        }
        Err(_) => Config { display_id: None },
    }
}

pub fn save_config() -> serde_json::Result<()> {
    let config_path = get_config_path();
    if !std::fs::exists(&config_path).unwrap_or(false) {
        std::fs::create_dir_all(&config_path.parent().unwrap()).ok();
        // If parent directiories exist but the file itself doesn't, it'll be created later, so this
        // is fine
    };
    let mut config_file = File::create(config_path).unwrap();
    let config = CONFIG.read().unwrap();
    let r = serde_json::to_writer(&config_file, config.deref());
    config_file.flush().unwrap();
    return r;
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct Config {
    pub display_id: Option<Uuid>,
}
