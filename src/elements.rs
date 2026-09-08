use crate::consts::COLUMNS;
use crate::consts::MARGINS;
use crate::consts::MENU_WIDTH;
use crate::consts::WINDOW_HEIGHT;
use crate::helper;
use crate::theme::BackgroundContainer;
use crate::theme::Rounded;
use crate::theme::button_style;
use crate::theme::text_color;
use crate::tray::get_tray_icon;
use battery::units::ratio::percent;
use chrono::Timelike;
use iced::alignment;
use iced::widget::image::Handle;
use iced::widget::scrollable;
use iced::widget::scrollable::Direction;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{Button, Row, button, column, container, space::*, text};
use iced::{Element, Length, Theme};
use rustsni::TrayItem;
use std::hash::Hash;

use crate::state::{AppState, Message};

pub fn window_text(state: &AppState) -> Element<'_, Message> {
    let window_text = if let Some(focused_window) = &state.focused_window {
        &focused_window.title
    } else {
        &"".to_string()
    };
    let window_text = helper::truncate(window_text, 30);
    text!("{}", window_text).into()
}

pub fn tray_menu(state: &AppState) -> Element<'_, Message> {
    let menu_progress = state
        .menu_open
        .interpolate(0., MENU_WIDTH as f32, state.now);
    container(
        scrollable(
            column(
                state
                    .menu_items
                    .iter()
                    .filter_map(|item| {
                        if item.visible && !item.label.is_empty() {
                            Some(Element::from(
                                button(text!("{}", item.label).wrapping(text::Wrapping::None))
                                    .style(button_style)
                                    .width(Length::Fill)
                                    .on_press(Message::MenuEntryPressed(item.id)),
                            ))
                        } else {
                            None
                        }
                    }),
            )
            .padding(iced::padding::vertical(MARGINS))
            .spacing(MARGINS),
        )
        .spacing(0)
        .direction(Direction::Vertical(Scrollbar::hidden()))
        .width(Length::Fill),
    )
    .padding(iced::padding::horizontal(MARGINS))
    .width(menu_progress)
    .height(WINDOW_HEIGHT)
    .style(|theme| {
        container::primary(theme)
            .background_container(theme)
            .rounded()
    })
    .into()
}

pub fn workspaces(state: &AppState) -> Row<'_, Message> {
    Row::from_iter(
        state
            .workspaces
            .iter()
            .filter(|w| w.active_window_id.is_some() || w.is_focused)
            .map(|w| {
                let is_focused = state.focused_workspaces.contains(&w.id);
                button(
                    text!("{}", w.name.clone().unwrap_or(w.idx.to_string()))
                        .align_x(alignment::Alignment::Center)
                        .height(20),
                )
                .style(move |theme: &Theme, status| {
                    if is_focused {
                        button::primary(theme, status)
                    } else {
                        let mut b = button::background(theme, status);
                        b.text_color = theme.palette().text;
                        b
                    }
                    .rounded()
                })
                .on_press(Message::FocusWorkspace(w.id))
                .width(Length::Fill)
                .into()
            }),
    )
    .align_y(iced::alignment::Vertical::Center)
    .padding(iced::padding::horizontal(MARGINS).vertical(MARGINS / 2))
    .spacing(MARGINS)
}

pub fn box_content<'a>(icon: &String, text: &String) -> Element<'a, Message> {
    column![
        text!("{}", icon).center().width(Length::Fill).size(24),
        vertical(),
        text!("{}", text)
            .center()
            .width(Length::Fill)
            .style(|theme| {
                let mut style = text::primary(theme);
                style.color = Some(text_color(&theme.palette().primary));
                style
            })
    ]
    .width(Length::Fill)
    .into()
}

pub fn data_box<'a>(icon: &String, text: &String) -> Element<'a, Message> {
    container(box_content(icon, text))
        .padding(iced::padding::all(MARGINS))
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::primary(theme)
                .rounded()
                .background(palette.primary.weak.color)
        })
        .into()
}

pub fn button_box<'a>(icon: &String, text: &String) -> Button<'a, Message> {
    button(box_content(icon, text))
        .padding(iced::padding::all(MARGINS))
        .style(button_style)
}

struct TrayItemWrapper<'a>(pub &'a TrayItem);

impl<'a> Hash for TrayItemWrapper<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.id.to_string().hash(state)
    }
}

pub fn tray_button<'a>(item: &'a TrayItem) -> Button<'a, Message> {
    let image = iced::widget::lazy(TrayItemWrapper(item), |item| {
        let item = item.0;
        match get_tray_icon(item) {
            Some(path) => Element::from(
                iced::widget::Image::new(Handle::from_path(path))
                    .width(Length::Fill)
                    .height(Length::Fill),
            ),
            None => match item.best_icon_pixmap() {
                Some(pixmap) => Element::from(
                    iced::widget::Image::new(Handle::from_rgba(
                        pixmap.width,
                        pixmap.height,
                        pixmap.data.clone(),
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill),
                ),
                None => {
                    let icon = item.title.chars().next().unwrap_or('⊟').to_uppercase();
                    Element::from(button_box(&icon.to_string(), &item.title))
                }
            },
        }
    });
    button(container(image).padding(iced::padding::all(MARGINS))).style(button_style)
}

pub fn battery_indicators(state: &AppState) -> impl Iterator<Item = Element<'_, Message>> {
    let Ok(batteries) = state.battery.batteries() else {
        return either::Left(std::iter::empty());
    };
    either::Right(batteries.flatten().map(|battery| {
        data_box(
            &"🔋".to_string(),
            &format!(
                "{:02.2}%",
                battery.state_of_charge().get::<percent>().to_string()
            ),
        )
    }))
}

pub fn tray_buttons(state: &AppState) -> impl Iterator<Item = Element<'_, Message>> {
    state.tray_icons.values().map(|t| {
        tray_button(t)
            .on_press(Message::TrayPressed(t.id.clone()))
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    })
}

pub fn expanded_content(state: &AppState) -> Element<'_, Message> {
    let time = chrono::Local::now();
    column![
        column![
            iced::widget::row![
                window_text(state),
                horizontal(),
                Element::from(text!("{:02}:{:02}", time.hour(), time.minute()))
            ],
            vertical().height(MARGINS),
            iced::widget::scrollable(
                iced::widget::grid(battery_indicators(state).chain(tray_buttons(state)))
                    .spacing(MARGINS)
                    .columns(COLUMNS),
            )
            .height(Length::Fill)
        ]
        .padding(iced::padding::horizontal(MARGINS)),
        container(workspaces(state))
    ]
    .spacing(MARGINS)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn peek_content(state: &AppState) -> Element<'_, Message> {
    let content = column![
        container(window_text(state)).padding(iced::padding::horizontal(MARGINS)),
        workspaces(state),
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    iced::widget::mouse_area(content)
        .on_enter(Message::UnCollapse)
        .into()
}

pub fn collapsed_content(state: &AppState) -> Element<'_, Message> {
    let activator: Element<Message> =
        iced::widget::mouse_area(iced::widget::space().height(2).width(Length::Fill))
            .on_enter(Message::UnCollapse)
            .into();
    iced::widget::stack![
        activator,
        container(column![vertical(), workspaces(state)])
            .style(|theme| container::primary(theme).background_container(theme)),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
