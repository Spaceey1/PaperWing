use std::collections::HashMap;

use iced::{Element, color, widget::container};
use rustsni::{ItemId, TrayItem};

use crate::{app::Message, tray::get_tray_host};

// https://stackoverflow.com/questions/38461429/how-can-i-truncate-a-string-to-have-at-most-n-characters
pub fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((idx, _)) => (s[..idx].to_string() + "...").to_owned(),
    }
}

#[allow(dead_code)]
pub fn debug_container(content: Element<'_, Message>) -> Element<'_, Message> {
    container(content).style(|_| container::Style::default().background(color!(1, 0, 0))).into()
}

#[macro_export]
macro_rules! package_name {
    () => {
        env!("CARGO_PKG_NAME")
    };
}
