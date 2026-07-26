use iced::{Element, color, widget::container};

use crate::state::Message;

// https://stackoverflow.com/questions/38461429/how-can-i-truncate-a-string-to-have-at-most-n-characters
pub fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((idx, _)) => (s[..idx].to_string() + "...").to_owned(),
    }
}

#[allow(dead_code)]
pub fn debug_container(content: Element<'_, Message>) -> Element<'_, Message> {
    container(content)
        .style(|_| container::Style::default().background(color!(1, 0, 0)))
        .into()
}

#[macro_export]
macro_rules! package_name {
    () => {
        env!("CARGO_PKG_NAME")
    };
}

pub fn set_if_some<T>(into: &mut T, from: Option<T>) {
    if let Some(from) = from {
        *into = from;
    }
}

pub fn parse_hex_color(hex: &str) -> Result<iced::Color, ()> {
    if !hex.is_ascii() {
        return Err(());
    }
    let hex = hex.to_uppercase();
    match hex.len() {
        // RGB
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).map_err(|_| ())?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).map_err(|_| ())?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).map_err(|_| ())?;
            Ok(iced::Color::from_rgb8(r, g, b))
        }
        // RGBA
        4 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).map_err(|_| ())?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).map_err(|_| ())?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).map_err(|_| ())?;
            let a = u8::from_str_radix(&hex[3..4].repeat(2), 16).map_err(|_| ())?;
            Ok(iced::Color::from_rgba8(r, g, b, a as f32 / 255.0))
        }
        // RRGGBB
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| ())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| ())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| ())?;
            Ok(iced::Color::from_rgb8(r, g, b))
        }
        // RRGGBBAA
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| ())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| ())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| ())?;
            let a = u8::from_str_radix(&hex[6..8], 16).map_err(|_| ())?;
            Ok(iced::Color::from_rgba8(r, g, b, a as f32 / 255.0))
        }
        _ => Err(()),
    }
}
