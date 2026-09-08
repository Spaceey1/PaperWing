use crate::{
    config::CONFIG,
    consts::{APP_NAME, RADIUS},
    helper::set_if_some,
    state::AppState,
};
use iced::{
    Color,
    border::{self, bottom},
    theme::Base,
    widget::{button, container},
};

pub fn default_theme(mode: Option<iced::theme::Mode>) -> iced::Theme {
    native_theme_iced::from_system().map_or(
        match mode {
            Some(iced::theme::Mode::Light) => iced::theme::Theme::Light,
            _ => iced::theme::Theme::Dark,
        },
        |r| r.0,
    )
}

pub fn text_color(background_color: &iced::Color) -> iced::Color {
    if background_color.relative_luminance() < 0.5 {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

fn palette(state: &AppState) -> iced::theme::Palette {
    let theme = default_theme(state.mode);
    let mut palette = theme.palette();
    CONFIG.with_borrow(|config| {
        set_if_some(
            &mut palette.primary,
            config.primary.clone().map(|p| p.into()),
        );
        palette.text = text_color(
            config
                .background
                .as_ref()
                .map(|c| c.into())
                .unwrap_or(&palette.background),
        )
    });
    palette
}

pub fn theme(state: &AppState) -> iced::Theme {
    let mut palette = palette(state);
    palette.background = iced::Color::TRANSPARENT;
    let name = format!("{} - transparent", APP_NAME);
    iced::Theme::custom(name, palette)
}

pub trait BackgroundContainer {
    fn background_container(self, theme: &iced::Theme) -> Self;
}

impl BackgroundContainer for container::Style {
    fn background_container(self, theme: &iced::Theme) -> Self {
        CONFIG.with_borrow(|config| {
            let default = default_theme(Some(theme.mode())).palette();
            let palette = theme.extended_palette();
            let primary = palette.primary.weak.color;
            let bg = config
                .background
                .as_ref()
                .map(|bg| bg.to_owned().into())
                .unwrap_or_else(|| default.background);

            self.background(bg)
                .border(border::color(primary).rounded(bottom(RADIUS)).width(1))
                .color(text_color(&bg))
        })
    }
}

pub trait Rounded {
    fn rounded(self) -> Self;
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

pub fn button_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();
    let bg = match status {
        button::Status::Hovered => palette.primary.weak.color,
        button::Status::Pressed => palette.primary.base.color,
        _ => palette.background.base.color,
    };
    let mut p = button::primary(theme, status).with_background(bg);
    p.border = iced::border::Border {
        color: palette.primary.base.color,
        width: 0.5,
        radius: RADIUS.into(),
    };
    p.text_color = text_color(&bg);
    p
}
