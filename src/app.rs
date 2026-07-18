use crate::consts::{MENU_WIDTH, RADIUS};
use crate::ipc;
use crate::theme::{BackgoundContainer, Rounded};
use crate::tray::{get_tray_host, tray_listiner};
use crate::{
    consts::{APP_NAME, MARGINS, UP_TRAVEL, WINDOW_HEIGHT, WINDOW_WIDTH},
    helper,
    state::AppState,
    theme::{self},
};
use battery::units::ratio::percent;
use chrono::Timelike;
use compositor_bridge::CompositorEvent;
use compositor_bridge::state::Workspace;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{Button, button, scrollable};
use iced::{
    Element, Subscription, Task, Theme,
    futures::{SinkExt, channel::mpsc},
    widget::{
        Row, column, container, grid, row,
        space::{horizontal, vertical},
        text,
    },
};
use iced::{Length, border};
use iced_layershell::{reexport::Anchor, settings::LayerShellSettings, to_layer_message};
use rustsni::{ItemId, MenuNode, TrayEvent, TrayItem};
use std::sync::Arc;

#[to_layer_message]
#[derive(Debug, Clone)]
pub enum Message {
    CompositorMessage(CompositorEvent),
    TrayMessage(TrayEvent),
    TrayAdded(ItemId, TrayItem),
    TrayPressed(ItemId),
    OpenTrayWith(Vec<MenuNode>, ItemId),
    MenuEntryPressed(i32),
    CloseTray,
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
    if state.collapsed.is_animating(state.now) || state.menu_open.is_animating(state.now) {
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
    // TODO: Make visible groups seperated by output instead of just sorting them
    workspaces.sort_by(|a, b| a.output.cmp(&b.output).then_with(|| a.idx.cmp(&b.idx)));
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
            if state.collapsed.value() && state.menu_open.value() {
                Task::done(Message::CloseTray)
            } else {
                Task::none()
            }
        }
        Message::AnimationUpdate => {
            state.now = std::time::Instant::now();
            if state.collapsed.is_animating(state.now) {
                Task::done(Message::SizeChange((
                    WINDOW_WIDTH,
                    state
                        .collapsed
                        .interpolate::<f32>(WINDOW_HEIGHT as f32, UP_TRAVEL as f32, state.now)
                        .round() as u32,
                )))
            } else {
                Task::none()
            }
        }
        Message::TrayMessage(msg) => {
            match msg {
                TrayEvent::ItemAdded(id) | TrayEvent::ItemChanged(id) => {
                    let id2 = id.clone();
                    return Task::future(async move {
                        rustsni::Result::Ok(
                            get_tray_host()?
                                .read()
                                .await
                                .items()
                                .get(&id2)
                                .unwrap()
                                .clone(),
                        )
                    })
                    .map(move |item| match item {
                        Ok(item) => Message::TrayAdded(id.clone(), item),
                        Err(_) => Message::Refresh,
                    });
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
        Message::TrayPressed(id) => {
            let id2 = id.clone();
            Task::future(async move {
                rustsni::Result::Ok(get_tray_host()?.write().await.get_menu(&id2, 0).unwrap())
            })
            .map(move |items| match items {
                Ok(items) => Message::OpenTrayWith(items, id.clone()),
                Err(_) => Message::Refresh,
            })
            // TODO: fix probably unnecessary clones? I'm not sure why I have to clone here tbh
        }
        Message::OpenTrayWith(items, id) => {
            state.menu_items = items;
            state.menu_id = Some(id);
            state.now = std::time::Instant::now();
            state.menu_open.go_mut(true, state.now);
            Task::none()
        }
        Message::MenuEntryPressed(id) => {
            let menu_id = state.menu_id.clone();
            smol::spawn(async move {
                let Ok(menu) = get_tray_host() else {
                    return;
                };
                let mut menu = menu.write().await;
                let Some(menu_id) = &menu_id else { return };
                let _ = menu.menu_click(menu_id, id);
            })
            .detach();
            Task::done(Message::CloseTray)
        }
        Message::CloseTray => {
            state.now = std::time::Instant::now();
            state.menu_open.go_mut(false, state.now);
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
    state.tray_icons.iter().map(|(_id, t)| {
        button_box(&"A".to_string(), &t.tooltip.title)
            .on_press(Message::TrayPressed(t.id.clone()))
            .into()
    })
}

fn view(state: &AppState) -> Element<'_, Message> {
    const MAIN_WIDTH: u32 = WINDOW_WIDTH - MENU_WIDTH * 2 - MARGINS * 2;
    let main_style = {
        let mut s = container::Style::default().backgound_container(state);
        s.border = s.border.rounded(border::bottom(RADIUS));
        s
    };
    if !state.collapsed.value() {
        let time = chrono::Local::now();
        let menu_progress = state
            .menu_open
            .interpolate(0., MENU_WIDTH as f32, state.now);
        let main = container(
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
                )
                .height(Length::Fill),
                container(workspaces(state))
            ]
            .spacing(MARGINS),
        )
        .width(MAIN_WIDTH)
        .style(move |_| main_style.clone());
        let menu_opening = state.menu_open.is_animating(state.now) || state.menu_open.value();
        let mut main_row = row![
            horizontal(),
            main,
            // TODO: clean up this mess
            if menu_opening {
                let m = container(
                    scrollable(
                        column(
                            std::iter::once(
                                row![
                                    horizontal(),
                                    button(text!("x")).on_press(Message::CloseTray)
                                ]
                                .into(),
                            )
                            .chain(
                                state
                                    .menu_items
                                    .iter()
                                    .map(|item| {
                                        if item.visible && item.label.len() > 0 {
                                            Some(Element::from(
                                                button(text!("{}", item.label))
                                                    .style(|theme, status| {
                                                        button::primary(theme, status).rounded()
                                                    })
                                                    .width(Length::Fill)
                                                    .on_press(Message::MenuEntryPressed(item.id)),
                                            ))
                                        } else {
                                            None
                                        }
                                    })
                                    .flatten(),
                            ),
                        )
                        .spacing(MARGINS),
                    )
                    .spacing(0)
                    .direction(Direction::Vertical(Scrollbar::hidden()))
                    .width(Length::Fill),
                )
                .padding(iced::padding::all(MARGINS))
                .width(menu_progress)
                .height(WINDOW_HEIGHT);
                m.style(|_| {
                    iced::widget::container::Style::default()
                        .backgound_container(state)
                        .rounded()
                })
                .into()
            } else {
                Element::from(horizontal())
            }
        ]
        .spacing(MARGINS);
        if menu_opening {
            main_row = main_row.push(horizontal());
        }
        main_row.into()
    } else {
        let main = container(workspaces(state))
            .width(MAIN_WIDTH)
            .style(move |_| main_style);
        row![horizontal(), main, horizontal()].into()
    }
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
                margin: (0, 0, 0, 0),
                ..Default::default()
            },
            ..Default::default()
        })
        .theme(theme::theme)
        .run()
        .unwrap();
}
