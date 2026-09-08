#![allow(dead_code)]

use crate::{consts::APP_NAME, helper::parse_hex_color};
use directories::ProjectDirs;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::error::Category::{self};
use std::{cell::RefCell, fmt::Display, fs::File, io::Write, path::PathBuf, sync::LazyLock};

#[derive(PartialEq, Clone, Debug)]
pub enum ConfigRWError {
    WrongSyntax(String),
    Missing,
    Io,
}

impl Display for ConfigRWError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Io => "Io Error".into(),
                Self::WrongSyntax(s) => format!("Syntax error in config: {s}"),
                Self::Missing => "Config file is missing.".into(),
            }
        )
    }
}

static CONFIG_DIR: std::sync::LazyLock<ProjectDirs> =
    std::sync::LazyLock::new(|| ProjectDirs::from("", "", APP_NAME).unwrap());

thread_local! {
    pub static CONFIG: RefCell<LazyLock<Config>> = RefCell::new(LazyLock::new(|| {
        let config = load_config().inspect_err(|e| eprintln!("{e}"));
        config.unwrap_or_else(|_|Config::default())
    }));
}

fn get_config_path() -> PathBuf {
    CONFIG_DIR.config_dir().join("config.json")
}

fn load_config() -> Result<Config, ConfigRWError> {
    let config_path = get_config_path();
    if !std::fs::exists(&config_path).unwrap() {
        return Ok(Config {
            ..Default::default()
        });
    }
    let config_file = File::open(config_path);
    match config_file {
        Ok(config_file) => {
            let config = serde_json::from_reader::<_, Config>(std::io::BufReader::new(config_file))
                .map_err(|e| match e.classify() {
                    Category::Syntax | Category::Data | Category::Eof => {
                        ConfigRWError::WrongSyntax(e.to_string())
                    }
                    Category::Io => ConfigRWError::Io,
                });
            if config.as_ref().is_err_and(|e| *e == ConfigRWError::Io) {
                return Err(ConfigRWError::Missing);
            }
            config
        }
        Err(_) => Err(ConfigRWError::Missing),
    }
}

pub fn refresh_config() -> Result<(), ConfigRWError> {
    CONFIG.with_borrow_mut(|config| -> Result<(), ConfigRWError> {
        *config = load_config()?.into();
        Ok(())
    })
}

pub fn save_config() -> serde_json::Result<()> {
    let config_path = get_config_path();
    if !std::fs::exists(&config_path).unwrap_or(false) {
        std::fs::create_dir_all(config_path.parent().unwrap()).ok();
        // If parent directiories exist but the file itself doesn't, it'll be created later, so this
        // is fine
    };
    let mut config_file = File::create(config_path).unwrap();
    CONFIG.with_borrow(|config| {
        let r = serde_json::to_writer(&config_file, &**config);
        config_file.flush().unwrap();
        r
    })
}

#[derive(Clone, Default, Debug)]
pub struct Color(iced::Color);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        // Deserializing from a tuple r, g, b or r, g, b, a
        if let Some(arr) = value.as_array() {
            for x in arr {
                if let Some(x) = x.as_f64()
                    && (!(0. ..=1.).contains(&x)) {
                        return Err(de::Error::custom(
                            "invalid color format: color values must be between 0 and 1 inclusive",
                        ));
                    }
            }
            match arr.as_slice() {
                [r, g, b] => {
                    if let (Some(r), Some(g), Some(b)) = (r.as_f64(), g.as_f64(), b.as_f64()) {
                        return Ok(Color(iced::Color::from_rgb(r as f32, g as f32, b as f32)));
                    }
                }
                [r, g, b, a] => {
                    if let (Some(r), Some(g), Some(b), Some(a)) =
                        (r.as_f64(), g.as_f64(), b.as_f64(), a.as_f64())
                    {
                        return Ok(Color(iced::Color::from_rgba(
                            r as f32, g as f32, b as f32, a as f32,
                        )));
                    }
                }
                _ => {}
            }
        }

        // Deserializing from Hex Strings "#RRGGBB", "0xRRGGBBAA", etc.
        if let Some(s) = value.as_str() {
            let hex = s.trim_start_matches("#").trim_start_matches("0x");
            if let Ok(color) = parse_hex_color(hex) {
                return Ok(Color(color));
            }
        }

        Err(de::Error::custom(
            "invalid color format: expected [r,g,b], [r,g,b,a], or hex string in RGB, RGBA, RRGGBB or RRGGBBAA format",
        ))
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (self.0.r, self.0.g, self.0.b, self.0.a).serialize(serializer)
    }
}

impl From<Color> for iced::Color {
    fn from(val: Color) -> Self {
        val.0
    }
}

impl<'a> From<&'a Color> for &'a iced::Color {
    fn from(val: &'a Color) -> Self {
        &val.0
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct Config {
    pub background: Option<Color>,
    pub primary: Option<Color>,
    pub font: Option<String>,
    pub display: Option<String>,
    pub peek_timeout: Option<f32>,
}
