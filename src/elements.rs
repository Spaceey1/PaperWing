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
use chrono::Timelike;
use iced::alignment;
use iced::widget::canvas;
use iced::widget::image;
use iced::widget::image::Handle;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::scrollable::Direction;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{Button, Row, button, column, container, responsive, space::*, text};
use iced::{ContentFit, Element, Length, Point, Rectangle, Theme, mouse};
use std::hash::Hash;
use std::sync::LazyLock;
use std::time::Instant;
use tray_server::TrayItem;

use crate::state::{AppState, BatteryInfo, BatteryStatus, Message};

/// Start of the battery fluid animation, used to derive its phase.
static FLUID_STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);

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

#[allow(dead_code)]
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
    let time = state
        .now
        .saturating_duration_since(*FLUID_STARTED)
        .as_secs_f32();

    state
        .batteries
        .iter()
        .map(move |battery| battery_box(*battery, time))
}

pub fn battery_box<'a>(battery: BatteryInfo, time: f32) -> Element<'a, Message> {
    let charging = battery.status == BatteryStatus::Charging;
    let time = (time * if charging { 1.6 } else { 1.0 }) % FLUID_LOOP_LEN;

    container(iced::widget::stack![
        canvas(BatteryFluid {
            level: battery.level,
            time
        })
        .width(Length::Fill)
        .height(Length::Fill),
        container(
            column![
                text!(
                    "{}",
                    (if charging && battery.level != 1. {
                        "⚡"
                    } else {
                        "🔋"
                    })
                )
                .style(|theme: &Theme| {
                    let mut style = text::primary(theme);
                    style.color = Some(text_color(&theme.palette().primary));
                    style
                })
                .center()
                .width(Length::Fill)
                .size(24),
                vertical(),
                text!("{:.0}%", battery.level * 100.0)
                    .center()
                    .width(Length::Fill)
                    .style(|theme: &Theme| {
                        let mut style = text::primary(theme);
                        style.color = Some(text_color(&theme.palette().primary));
                        style
                    }),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .padding(iced::padding::all(MARGINS))
        .width(Length::Fill)
        .height(Length::Fill),
    ])
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::primary(theme)
            .rounded()
            .background(palette.primary.weak.color)
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

const FLUID_LOOP_LEN: f32 = 32.0;
const FLUID_SEGMENTS: usize = 64;

/// Height of the fluid surface at `x`, for a cell of `size` filled to `level`.
fn fluid_surface_y(x: f32, size: iced::Size, level: f32, time: f32) -> f32 {
    let width = size.width.max(1.0);
    let height = size.height.max(1.0);
    let progress = x / width;

    (1.0 - level) * height
        + 0.019 * height * (progress * std::f32::consts::TAU * 1.25 + time * 0.3927).sin()
        + 0.010 * height * (progress * std::f32::consts::TAU * 2.63 - time * 0.5890).sin()
}

struct BatteryFluid {
    level: f32,
    time: f32,
}

impl canvas::Program<Message> for BatteryFluid {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let size = bounds.size();
        let level = self.level.clamp(0.0, 1.0);

        if level <= 0.0 || size.width <= 0.0 || size.height <= 0.0 {
            return Vec::new();
        }

        let accent = theme.extended_palette().primary.base.color;
        let radius = RADIUS as f32;
        let mut frame = canvas::Frame::new(renderer, size);

        if level >= 1.0 {
            frame.fill(
                &canvas::Path::rounded_rectangle(Point::ORIGIN, size, RADIUS.into()),
                accent,
            );

            return vec![frame.into_geometry()];
        }

        let left = fluid_surface_y(0.0, size, level, self.time);
        let right = fluid_surface_y(size.width, size, level, self.time);
        let bottom_left = (size.height - left).clamp(0.0, radius);
        let bottom_right = (size.height - right).clamp(0.0, radius);

        let path = canvas::Path::new(|path| {
            path.move_to(Point::new(0.0, left));
            for segment in 1..=FLUID_SEGMENTS {
                let x = size.width * segment as f32 / FLUID_SEGMENTS as f32;
                path.line_to(Point::new(x, fluid_surface_y(x, size, level, self.time)));
            }
            path.line_to(Point::new(size.width, size.height - bottom_right));
            path.quadratic_curve_to(
                Point::new(size.width, size.height),
                Point::new(size.width - bottom_right, size.height),
            );
            path.line_to(Point::new(bottom_left, size.height));
            path.quadratic_curve_to(
                Point::new(0.0, size.height),
                Point::new(0.0, size.height - bottom_left),
            );
            path.close();
        });

        frame.fill(&path, accent);

        vec![frame.into_geometry()]
    }
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

struct GridCell<'a> {
    element: Element<'a, Message>,
    column_span: usize,
    row_span: usize,
}

impl<'a> GridCell<'a> {
    fn new(element: impl Into<Element<'a, Message>>) -> Self {
        Self {
            element: element.into(),
            column_span: 1,
            row_span: 1,
        }
    }

    fn column_span(mut self, span: usize) -> Self {
        self.column_span = span;
        self
    }

    fn row_span(mut self, span: usize) -> Self {
        self.row_span = span;
        self
    }
}

/// Where a cell ended up on the grid: row, column, column span, row span.
type Placement = (usize, usize, usize, usize);

/// Packs the given `(column_span, row_span)` sizes into a `columns` wide grid in
/// reading order, filling the first free slot that fits each one.
fn place(columns: usize, spans: impl IntoIterator<Item = (usize, usize)>) -> Vec<Placement> {
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let mut placements = Vec::new();

    for (column_span, row_span) in spans {
        let column_span = column_span.clamp(1, columns);
        let row_span = row_span.max(1);

        let (row, column) = first_free_slot(&mut occupied, columns, column_span, row_span);
        for occupied_row in &mut occupied[row..row + row_span] {
            for occupied_cell in &mut occupied_row[column..column + column_span] {
                *occupied_cell = true;
            }
        }

        placements.push((row, column, column_span, row_span));
    }

    placements
}

/// Lays out `cells` on a `columns` wide grid of square `cell_size` cells,
///
/// Cells are packed in order into the first slot that fits their spans, so a
/// cell can cover several rows and the following cells flow past it.
fn grid<'a>(cells: Vec<GridCell<'a>>, cell_size: f32, columns: usize) -> Element<'a, Message> {
    let cell_size = cell_size.max(0.0);
    let gap = MARGINS as f32;
    let columns = columns.max(1);

    let placements = place(
        columns,
        cells.iter().map(|cell| (cell.column_span, cell.row_span)),
    );

    let rows = placements
        .iter()
        .map(|(row, _, _, row_span)| row + row_span)
        .max()
        .unwrap_or(0);
    let height = cell_size * rows as f32 + gap * rows.saturating_sub(1) as f32;

    let layers =
        cells
            .into_iter()
            .zip(placements)
            .map(|(cell, (row, column, column_span, row_span))| {
                let width =
                    cell_size * column_span as f32 + gap * column_span.saturating_sub(1) as f32;
                let cell_height =
                    cell_size * row_span as f32 + gap * row_span.saturating_sub(1) as f32;

                // The grid is absolutely positioned so a cell can span several rows
                // without the rows below it being pushed down. The outer container
                // fills the grid and its padding is what offsets the cell.
                container(container(cell.element).width(width).height(cell_height))
                    .padding(iced::Padding {
                        top: (row as f32) * (cell_size + gap),
                        left: (column as f32) * (cell_size + gap),
                        ..iced::Padding::ZERO
                    })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            });

    iced::widget::stack(layers)
        .width(Length::Fill)
        .height(height)
        .into()
}

/// Finds the first slot, scanning row by row, where a `column_span` x `row_span`
/// area is free, growing the occupancy map as needed.
fn first_free_slot(
    occupied: &mut Vec<Vec<bool>>,
    columns: usize,
    column_span: usize,
    row_span: usize,
) -> (usize, usize) {
    let mut row = 0;

    loop {
        while occupied.len() < row + row_span {
            occupied.push(vec![false; columns]);
        }

        let free_column = (0..=columns - column_span).find(|&column| {
            occupied[row..row + row_span].iter().all(|occupied_row| {
                occupied_row[column..column + column_span]
                    .iter()
                    .all(|occupied| !occupied)
            })
        });

        if let Some(column) = free_column {
            return (row, column);
        }

        row += 1;
    }
}

/// Horizontal gutter shared by every bar content layout so all states line up with each other
fn content_padding() -> iced::Padding {
    iced::padding::all(MARGINS)
}

fn top_bar<'a>(state: &'a AppState) -> Element<'a, Message> {
    let time = chrono::Local::now();
    iced::widget::row![
        window_text(state),
        horizontal(),
        Element::from(text!("{:02}:{:02}", time.hour(), time.minute()))
    ]
    .into()
}

pub fn expanded_content(state: &AppState) -> Element<'_, Message> {
    container(
        column![
            column![
                top_bar(state),
                vertical().height(MARGINS),
                iced::widget::scrollable(
                    responsive(move |size| {
                        let cell_width = ((size.width
                            - MARGINS as f32 * COLUMNS.saturating_sub(1) as f32)
                            / COLUMNS as f32)
                            .max(0.0);
                        let mut cells = Vec::new();

                        cells.extend(
                            battery_indicators(state)
                                .map(|element| GridCell::new(element).row_span(2)),
                        );
                        for (name, player) in &state.players {
                            let dependency = (name, &player.metadata, &player.image);
                            cells.push(
                                GridCell::new(iced::widget::lazy(
                                    dependency,
                                    |(name, metadata, image)| {
                                        media_box(
                                            name.as_str(),
                                            (*metadata).as_ref(),
                                            (*image).clone(),
                                        )
                                    },
                                ))
                                .column_span(3),
                            );
                        }
                        cells.extend(tray_buttons(state).map(GridCell::new));

                        grid(cells, cell_width, COLUMNS)
                    })
                    .height(Length::Shrink),
                )
                .height(Length::Fill),
            ],
            container(workspaces(state)),
        ]
        .spacing(MARGINS)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .padding(content_padding())
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn peek_content(state: &AppState) -> Element<'_, Message> {
    let content = container(column![top_bar(state), workspaces(state)].spacing(MARGINS))
        .padding(content_padding().vertical(MARGINS / 2))
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
            .style(|theme| container::primary(theme).background_container(theme))
            .padding(content_padding().vertical(MARGINS / 2))
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
