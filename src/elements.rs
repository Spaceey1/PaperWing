use crate::consts::COLUMNS;
use crate::consts::MARGINS;
use crate::consts::MEDIA_IMAGE_SIZE;
use crate::consts::MENU_WIDTH;
use crate::consts::RADIUS;
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
use iced::widget::image;
use iced::widget::image::Handle;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::scrollable::Direction;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{Button, Row, button, column, container, responsive, space::*, text};
use iced::{ContentFit, Element, Length, Theme};
use std::hash::Hash;
use tray_server::TrayItem;

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
            column(state.menu_items.iter().filter_map(|item| {
                if item.visible && !item.label.is_empty() {
                    Some(Element::from(
                        button(
                            text!("{}", item.label)
                                .width(Length::Fill)
                                .align_x(alignment::Horizontal::Left)
                                .wrapping(text::Wrapping::WordOrGlyph),
                        )
                        .padding([5, 8])
                        .style(button_style)
                        .width(Length::Fill)
                        .on_press(Message::MenuEntryPressed(item.id)),
                    ))
                } else {
                    None
                }
            }))
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

pub fn media_box(
    player_name: &str,
    metadata: Option<&media::MprisMetadata>,
    image_data: Option<Vec<u8>>,
) -> Element<'static, Message> {
    let title = helper::truncate(
        metadata
            .and_then(|metadata| metadata.title.as_deref())
            .filter(|title| !title.is_empty())
            .unwrap_or(player_name),
        8,
    );
    let artist = metadata
        .and_then(|metadata| {
            metadata
                .artist
                .as_ref()
                .map(|artists| artists.join(", "))
                .filter(|artist| !artist.is_empty())
                .or_else(|| metadata.album.clone())
        })
        .map(|s| helper::truncate(&s, 8))
        .unwrap_or_else(|| "Unknown artist".to_string());
    let duration = metadata
        .and_then(|metadata| metadata.length)
        .map(format_duration)
        .unwrap_or_default();
    let rewind = Message::MediaControl(player_name.to_string(), crate::state::MediaControl::Rewind);
    let toggle = Message::MediaControl(
        player_name.to_string(),
        crate::state::MediaControl::TogglePlay,
    );
    let skip = Message::MediaControl(player_name.to_string(), crate::state::MediaControl::Skip);

    let mut content_row = row![];
    if let Some(image_data) = image_data {
        content_row = content_row.push(
            container(
                image(Handle::from_bytes(image_data))
                    .content_fit(ContentFit::Cover)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .border_radius(RADIUS),
            )
            .width(MEDIA_IMAGE_SIZE)
            .height(Length::Fill)
            .clip(true),
        );
    }

    container(
        content_row
            .push(
                column![
                    text!("{}", helper::truncate(&title, 24))
                        .size(14)
                        .width(Length::Fill)
                        .wrapping(text::Wrapping::None),
                    row![
                        text!("{}", helper::truncate(&artist, 18))
                            .size(11)
                            .width(Length::Fill)
                            .wrapping(text::Wrapping::None)
                            .style(|theme: &Theme| {
                                let mut style = text::primary(theme);
                                style.color = Some(
                                    theme.extended_palette().primary.weak.text.scale_alpha(0.65),
                                );
                                style
                            }),
                        text!("{}", duration)
                            .size(11)
                            .wrapping(text::Wrapping::None)
                            .style(|theme: &Theme| {
                                let mut style = text::primary(theme);
                                style.color = Some(
                                    theme.extended_palette().primary.weak.text.scale_alpha(0.65),
                                );
                                style
                            }),
                    ]
                    .width(Length::Fill)
                    .spacing(4)
                    .align_y(alignment::Vertical::Center),
                    vertical(),
                    row![
                        button(text!("󰒮").center())
                            .on_press(rewind)
                            .width(Length::Fill)
                            .height(20)
                            .padding([2, 4])
                            .style(button_style),
                        button(text!("󰐎").center())
                            .on_press(toggle)
                            .width(Length::Fill)
                            .height(20)
                            .padding([2, 4])
                            .style(button_style),
                        button(text!("󰒭").center())
                            .on_press(skip)
                            .width(Length::Fill)
                            .height(20)
                            .padding([2, 4])
                            .style(button_style),
                    ]
                    .width(Length::Fill)
                    .spacing(4),
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .spacing(4),
            )
            .padding(6)
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::primary(theme)
            .background(palette.primary.weak.color)
            .border(
                iced::border::color(palette.primary.base.color.scale_alpha(0.35))
                    .width(1)
                    .rounded(RADIUS),
            )
            .color(palette.primary.weak.text)
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn format_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let hours = minutes / 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes % 60, seconds % 60)
    } else {
        format!("{}:{:02}", minutes, seconds % 60)
    }
}

const TRAY_ICON_SIZE: u32 = 32;

struct TrayItemWrapper<'a>(pub &'a TrayItem);

impl<'a> Hash for TrayItemWrapper<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.id.to_string().hash(state)
    }
}

pub fn tray_button<'a>(item: &'a TrayItem) -> Button<'a, Message> {
    let image = iced::widget::lazy(TrayItemWrapper(item), |item| {
        let item = item.0;
        let image = match get_tray_icon(item) {
            Some(path) => Some(Element::from(
                iced::widget::Image::new(Handle::from_path(path))
                    .content_fit(ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )),
            None => item.best_icon_pixmap().map(|pixmap| {
                Element::from(
                    iced::widget::Image::new(Handle::from_rgba(
                        pixmap.width,
                        pixmap.height,
                        pixmap.data.clone(),
                    ))
                    .content_fit(ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fill),
                )
            }),
        };
        match image {
            Some(image) => container(container(image).center(TRAY_ICON_SIZE).clip(true))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
            None => {
                let icon = item.title.chars().next().unwrap_or('^').to_uppercase();
                Element::from(button_box(&icon.to_string(), &item.title))
            }
        }
    });
    button(
        container(image)
            .padding(iced::padding::all(MARGINS))
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .style(button_style)
}

pub fn battery_indicators(state: &AppState) -> impl Iterator<Item = Element<'_, Message>> {
    let Ok(batteries) = state.battery.batteries() else {
        return either::Left(std::iter::empty());
    };
    either::Right(batteries.flatten().map(|battery| {
        data_box(
            &"🔋".to_string(),
            &format!(
                "{:.0}%",
                battery.state_of_charge().get::<percent>()
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

fn grid_row<'a>(cells: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    Row::with_children(cells)
        .spacing(MARGINS)
        .width(Length::Fill)
        .into()
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
                responsive(move |size| {
                    let cell_width = ((size.width
                        - MARGINS as f32 * COLUMNS.saturating_sub(1) as f32)
                        / COLUMNS as f32)
                        .max(0.0);
                    let mut cells = Vec::new();

                    cells.extend(battery_indicators(state).map(|element| (element, 1)));
                    for (name, player) in &state.players {
                        let dependency = (name, &player.metadata, &player.image);
                        cells.push((
                            Element::from(iced::widget::lazy(
                                dependency,
                                |(name, metadata, image)| {
                                    media_box(name.as_str(), (*metadata).as_ref(), (*image).clone())
                                },
                            )),
                            2,
                        ));
                    }
                    cells.extend(tray_buttons(state).map(|element| (element, 1)));

                    let mut rows = Vec::new();
                    let mut row_cells = Vec::new();
                    let mut used_columns = 0;

                    for (element, span) in cells {
                        if used_columns > 0 && used_columns + span > COLUMNS {
                            rows.push(grid_row(std::mem::take(&mut row_cells)));
                            used_columns = 0;
                        }

                        let width = cell_width * span as f32
                            + MARGINS as f32 * span.saturating_sub(1) as f32;
                        row_cells.push(container(element).width(width).height(cell_width).into());
                        used_columns += span;

                        if used_columns == COLUMNS {
                            rows.push(grid_row(std::mem::take(&mut row_cells)));
                            used_columns = 0;
                        }
                    }

                    if !row_cells.is_empty() {
                        rows.push(grid_row(row_cells));
                    }

                    column(rows).spacing(MARGINS).width(Length::Fill).into()
                })
                .height(Length::Shrink)
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
