use crate::{
    config::CONFIG, consts::{APP_NAME, RADIUS}, helper::set_if_some, state::AppState
};
use iced::{theme::Base, widget::button};

pub fn default_theme(mode: Option<iced::theme::Mode>) -> iced::Theme {
    native_theme_iced::from_system().map_or(
        match mode {
            Some(iced::theme::Mode::Light) => iced::theme::Theme::Light,
            _ => iced::theme::Theme::Dark,
        },
        |r| r.0,
    )
}

pub fn theme(state: &AppState) -> iced::Theme {
    let theme = default_theme(state.mode);
    let mut palette = theme.palette();
    let name = format!("{} - {}", theme.name(), APP_NAME);
    palette.background = iced::Color::TRANSPARENT;
    CONFIG.with_borrow(|config| {
        let Some(config) = config.as_ref() else {
            return iced::Theme::custom(name, palette);
        };
        set_if_some(
            &mut palette.primary,
            config.primary.clone().map(|p| p.into()),
        );
        iced::Theme::custom(name, palette)
    })
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
        let mut palette = default_theme(state.mode).palette();
        CONFIG.with_borrow(|config| {
            if let Some(config) = config.as_ref() {
                set_if_some(
                    &mut palette.background,
                    config.background.clone().map(|b| b.into()),
                );
            }
            iced::widget::container::Style::default().background(palette.background)
        })
    }
}

pub fn button_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();
    let mut p = button::primary(theme, status).with_background(match status {
        button::Status::Hovered => palette.primary.weak.color,
        button::Status::Pressed => palette.primary.base.color,
        _ => palette.background.base.color,
    });
    p.border = iced::border::Border {
        color: palette.primary.base.color,
        width: 0.5,
        radius: RADIUS.into(),
    };
    p.text_color = palette.background.base.text;
    p
}
