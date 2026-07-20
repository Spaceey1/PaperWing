use iced::theme::Base;

use crate::{
    consts::{APP_NAME, RADIUS},
    state::AppState,
};

pub fn default_theme(mode: Option<iced::theme::Mode>) -> iced::Theme {
    match mode {
        Some(iced::theme::Mode::Light) => iced::theme::Theme::Light,
        _ => {
            let theme = iced::theme::Theme::Dark;
            let mut pal = theme.palette();
            pal.background = iced::color!(0x020202);
            pal.primary = iced::color!(0xa64dff);
            iced::Theme::custom(format!("Default - {}", theme.name()), pal)
        }
    }
}

pub fn theme(state: &AppState) -> iced::Theme {
    let theme = default_theme(state.mode);
    let mut palette = theme.palette();
    palette.background = iced::color!(0, 0, 0, 0.);
    let name = format!("{} - {}", theme.name(), APP_NAME);
    return iced::Theme::custom(name, palette);
}

pub trait Rounded {
    fn rounded(self) -> Self;
}

pub trait BackgoundContainer {
    fn backgound_container(self, state: &AppState) -> Self;
}

impl Rounded for iced::widget::container::Style {
    fn rounded(self) -> Self {
        let mut border = self.border;
        border.radius = RADIUS.into();
        self.border(border)
    }
}
impl Rounded for iced::widget::button::Style {
    fn rounded(mut self) -> Self {
        self.border.radius = RADIUS.into();
        self
    }
}
impl BackgoundContainer for iced::widget::container::Style {
    fn backgound_container(self, state: &AppState) -> Self {
        let palette = default_theme(state.mode).palette();
        iced::widget::container::Style::default().background(palette.background)
    }
}
