use crate::app::Message::Refresh;
use crate::ipc;
use crate::theme::Rounded;
use crate::tray::{get_tray_host, tray_listiner};
use crate::{
    consts::{APP_NAME, MARGINS, UP_TRAVEL, WINDOW_HEIGHT, WINDOW_WIDTH},
    helper,
    state::AppState,
    theme::{self, default_theme},
};
use battery::units::ratio::percent;
use chrono::Timelike;
use compositor_bridge::{self, CompositorEvent, state::Workspace};
use iced::wgpu::Color;
use iced::widget::{Button, button};
use iced::{Alignment, Length};
use iced::{
    Element, Subscription, Task, Theme,
    futures::{SinkExt, channel::mpsc},
    widget::{
        Row, column, container, grid, row,
        space::{horizontal, vertical},
        text,
    },
};
use iced_layershell::{reexport::Anchor, settings::LayerShellSettings, to_layer_message};
use rustsni::{ItemId, TrayEvent, TrayHost, TrayItem};
use std::sync::Arc;

#[to_layer_message]
#[derive(Debug, Clone)]
pub enum Message {
    CompositorMessage(CompositorEvent),
    TrayMessage(TrayEvent),
    TrayAdded(ItemId, TrayItem),
    ToggleCollapse,
    AnimationUpdate,
    Refresh,
}

/// Iced subscription for ipc with a wayland compositor
fn compositor_subscription(_state: &AppState) -> Subscription<Message> {
    Subscription::run(|| {
        iced::stream::channel::<Message>(100, |mut output: mpsc::Sender<Message>| async move {
            let (tx, rx) = smol::channel::unbounded::<CompositorEvent>();
            smol::spawn(async move { compositor_bridge::start_event_stream(tx).await.unwrap() })
                .detach();
            loop {
                let event = rx.recv().await;
                match event {
                    Ok(event) => {
                        output.send(Message::CompositorMessage(event)).await.ok();
                    }
                    Err(e) => panic!("{}", e),
                }
            }
        })
    })
}

fn clock_subscription() -> Subscription<Message> {
    Subscription::run(|| {
        iced::stream::channel::<Message>(10, |mut output: mpsc::Sender<Message>| async move {
            loop {
                output.send(Message::Refresh).await.unwrap();
                let now = chrono::Local::now();
                let next_update = now.with_second(0).unwrap().with_nanosecond(0).unwrap()
                    + chrono::Duration::minutes(1);
                smol::Timer::after((next_update - now).to_std().unwrap()).await;
            }
        })
    })
}

fn ipc_subscription() -> Subscription<Message> {
    Subscription::run(|| iced::stream::channel(10, ipc::start_listener))
}

fn animation_subscription(state: &AppState) -> Subscription<Message> {
    if state.collapsed.is_animating(state.now) {
        iced::window::frames().map(|_| Message::AnimationUpdate)
    } else {
        Subscription::none()
    }
}

fn tray_subscription() -> Subscription<Message> {
    Subscription::run(|| iced::stream::channel(10, tray_listiner))
}

/// Takes a vector of workspaces and an AppState and assigns it to the state, while also updating
/// the focused workspaces and sorting them by idx
fn update_workspace_state(state: &mut AppState, mut workspaces: Vec<Arc<Workspace>>) {
    workspaces.sort_by(|a, b| a.idx.cmp(&b.idx));
    state.workspaces = workspaces;
    state.focused_workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| workspace.is_active)
        .map(|workspace| workspace.id)
        .collect();
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::CompositorMessage(event) => {
            match event {
                CompositorEvent::WorkspacesChanged(workspaces) => {
                    update_workspace_state(state, workspaces);
                    Task::none()
                }
                CompositorEvent::FocusedWindowChanged(window) => {
                    state.focused_window = Some(window);
                    Task::none()
                }
                CompositorEvent::WorkspaceFocusChanged(_) => {
                    // I don't get information which workspace got unfocused, so have to request full state of all workspaces again
                    Task::future(async { compositor_bridge::get_workspaces().await }).map(
                        |result| match result {
                            Ok(workspaces) => Message::CompositorMessage(
                                CompositorEvent::WorkspacesChanged(workspaces),
                            ),
                            Err(e) => {
                                panic!("{}", e);
                            }
                        },
                    )
                }
            }
        }
        Message::ToggleCollapse => {
            state.now = std::time::Instant::now();
            state.collapsed.go_mut(!state.collapsed.value(), state.now);
            Task::none()
        }
        Message::AnimationUpdate => {
            state.now = std::time::Instant::now();
            Task::done(Message::SizeChange((
                WINDOW_WIDTH,
                state
                    .collapsed
                    .interpolate::<f32>(WINDOW_HEIGHT as f32, UP_TRAVEL as f32, state.now)
                    .round() as u32,
            )))
        }
        Message::TrayMessage(msg) => {
            match msg {
                TrayEvent::ItemAdded(id) | TrayEvent::ItemChanged(id) => {
                    let id2 = id.clone();
                    return Task::future(async move {
                        get_tray_host()
                            .read()
                            .await
                            .items()
                            .get(&id2)
                            .unwrap()
                            .clone()
                    })
                    .map(move |item| Message::TrayAdded(id.clone(), item));
                }
                TrayEvent::ItemRemoved(id) => {
                    state.tray_icons.remove(&id);
                }
                TrayEvent::MenuActivationRequested(_) | TrayEvent::MenuChanged(_) => {}
                TrayEvent::HostShutdown => panic!("Tray got shut down? Dont call shutdown."),
            };
            Task::none()
        }
        Message::TrayAdded(id, item) => {
            state.tray_icons.insert(id, item);
            Task::none()
        }
        _ => Task::none(),
    }
}

fn window_text(state: &AppState) -> Element<'_, Message> {
    let window_text = if state.focused_window.is_some() {
        &state.focused_window.as_ref().unwrap().title
    } else {
        &"".to_string()
    };
    let window_text = helper::truncate(window_text, 35);
    text!("{}", window_text).into()
}

fn workspaces(state: &AppState) -> Row<'_, Message> {
        Row::from_iter(state.workspaces.iter().map(|w| {
            container(horizontal().height(20))
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    container::Style::default()
                        .background(if state.focused_workspaces.contains(&w.id) {
                            palette.primary.base.color
                        } else {
                            palette.background.base.color
                        })
                        .rounded()
                })
                .into()
        }))
        .align_y(iced::alignment::Vertical::Center)
        .spacing(MARGINS)
}

fn box_content<'a>(icon: &String, text: &String) -> Element<'a, Message> {
    column![
        text!("{}", icon).center().size(48).width(Length::Fill),
        vertical(),
        text!("{}", text).center().size(24).width(Length::Fill)
    ]
    .width(Length::Fill)
    .into()
}

fn data_box<'a>(icon: &String, text: &String) -> Element<'a, Message> {
    container(box_content(icon, text))
        .padding(MARGINS as f32)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default()
                .rounded()
                .background(palette.primary.weak.color)
        })
        .into()
}

fn button_box<'a>(icon: &String, text: &String) -> Button<'a, Message> {
    button(box_content(icon, text))
        .padding(MARGINS as f32)
        .style(|theme, status| {
            let palette = theme.extended_palette();
            let mut p = button::Style::default()
                .with_background(match status {
                    button::Status::Hovered => palette.primary.base.color,
                    button::Status::Pressed => palette.primary.strong.color,
                    _ => palette.primary.weak.color,
                })
                .rounded();
            p.text_color = theme.palette().text;
            p
        })
}

fn battery_indicators(state: &AppState) -> impl Iterator<Item = Element<'_, Message>> {
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

fn tray_buttons(state: &AppState) -> impl Iterator<Item = Element<'_, Message>> {
    state
        .tray_icons
        .iter()
        .map(|(_id, t)| data_box(&"A".to_string(), &t.tooltip.title))
}

fn view(state: &AppState) -> Element<'_, Message> {
    let contents = if !state.collapsed.value() {
        let time = chrono::Local::now();
        Element::from(
            column![
                row![
                    window_text(state),
                    horizontal(),
                    Element::from(text!("{:02}:{:02}", time.hour(), time.minute()))
                ],
                iced::widget::scrollable(
                    grid(battery_indicators(state).chain(tray_buttons(state)))
                        .push(
                            button_box(&"T".to_string(), &"toggle".to_string())
                                .on_press(Message::ToggleCollapse),
                        )
                        .spacing(MARGINS),
                ).height(Length::Fill),
                // vertical(),
                container(workspaces(state))
            ]
            .spacing(MARGINS),
        )
    } else {
        workspaces(state).into()
    };
    container(contents)
        .style(|_| {
            let palette = default_theme(state.mode).palette();
            container::Style::default()
                .background(palette.background)
                .rounded()
        })
        .padding(iced::padding::horizontal(MARGINS).vertical(MARGINS / 2))
        .align_bottom(iced::Length::Fill)
        .align_y(Alignment::End)
        .into()
}

pub fn start_app() {
    // smol needs at least 2 threads for iced_layershell to not deadlock :/
    unsafe { std::env::set_var("SMOL_THREADS", "2") };
    iced_layershell::application(AppState::default, || APP_NAME.to_string(), update, view)
        .subscription(|state| {
            Subscription::batch([
                clock_subscription(),
                ipc_subscription(),
                tray_subscription(),
                compositor_subscription(state),
                animation_subscription(state),
            ])
        })
        .settings(iced_layershell::settings::Settings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top,
                layer: iced_layershell::reexport::Layer::Overlay,
                size: Some((WINDOW_WIDTH, UP_TRAVEL)),
                exclusive_zone: 4,
                keyboard_interactivity: iced_layershell::reexport::KeyboardInteractivity::None,
                start_mode: iced_layershell::settings::StartMode::Active,
                margin: (2, 0, 0, 0),
                ..Default::default()
            },
            ..Default::default()
        })
        .theme(theme::theme)
        .run()
        .unwrap();
}
