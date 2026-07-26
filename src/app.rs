use crate::config::CONFIG;
use crate::consts::{COLUMNS, MENU_WIDTH, RADIUS};
use crate::elements::*;
use crate::ipc::{SOCKET_MADE, get_sock_path};
use crate::state::Message;
use crate::subscriptions::*;
use crate::theme::{BackgoundContainer, Rounded, button_style};
use crate::tray::get_tray_host;
use crate::{
    consts::{APP_NAME, MARGINS, UP_TRAVEL, WINDOW_HEIGHT, WINDOW_WIDTH},
    state::AppState,
    theme::{self},
};
use async_signal::{Signal, Signals};
use chrono::Timelike;
use compositor_bridge::CompositorEvent;
use compositor_bridge::state::Workspace;
use iced::futures::StreamExt;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{button, scrollable, space, stack};
use iced::{
    Element, Subscription, Task,
    widget::{
        column, container, grid, row,
        space::{horizontal, vertical},
        text,
    },
};
use iced::{Length, border};
use iced_layershell::reexport::core::font;
use iced_layershell::{reexport::Anchor, settings::LayerShellSettings};
use rustsni::TrayEvent;
use smol::fs;
use std::process;
use std::sync::Arc;

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
        Message::ToggleCollapse => Task::done(if state.collapsed.value() {
            Message::UnCollapse
        } else {
            Message::Collapse
        }),
        Message::UnCollapse => {
            state.now = std::time::Instant::now();
            state.collapsed.go_mut(false, state.now);
            Task::none()
        }
        Message::Collapse => {
            state.now = std::time::Instant::now();
            state.collapsed.go_mut(true, state.now);
            if state.menu_open.value() {
                Task::done(Message::CloseTray)
            } else {
                Task::none()
            }
        }
        Message::AnimationUpdate => {
            state.now = std::time::Instant::now();
            let height = state
                .collapsed
                .interpolate::<f32>(WINDOW_HEIGHT as f32, UP_TRAVEL as f32, state.now)
                .round() as u32;
            Task::done(Message::SizeChange((WINDOW_WIDTH, height)))
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
                        Ok(mut item) => {
                            // iced takes rgba but rustsni gives me argb, so rotating every 4
                            // chunks 1 right converts it to the iced format
                            for pixmap in item.icon_pixmaps.iter_mut() {
                                for c in pixmap.data.chunks_mut(4) {
                                    c.rotate_right(1);
                                }
                            }
                            Message::TrayAdded(id.clone(), item)
                        }
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
            if state.menu_open.value() && state.menu_id == Some(id.clone()) {
                return Task::done(Message::CloseTray);
            }
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
            let Some(menu_id) = state.menu_id.clone() else {
                return Task::done(Message::CloseTray);
            };
            smol::spawn(async move {
                let Ok(menu) = get_tray_host() else {
                    return;
                };
                let mut menu = menu.write().await;
                let _ = menu.menu_click(&menu_id, id);
            })
            .detach();
            Task::done(Message::CloseTray)
        }
        Message::CloseTray => {
            state.now = std::time::Instant::now();
            state.menu_open.go_mut(false, state.now);
            Task::none()
        }
        Message::FocusWorkspace(id) => {
            smol::spawn(async move {
                let _ = compositor_bridge::go_to_workspace(&id).await;
            })
            .detach();
            Task::none()
        }
        _ => Task::none(),
    }
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
                column![
                    row![
                        window_text(state),
                        horizontal(),
                        Element::from(text!("{:02}:{:02}", time.hour(), time.minute()))
                    ],
                    vertical().height(MARGINS),
                    iced::widget::scrollable(
                        grid(battery_indicators(state).chain(tray_buttons(state)))
                            .spacing(MARGINS)
                            .columns(COLUMNS),
                    )
                    .height(Length::Fill)
                ]
                .padding(iced::padding::horizontal(MARGINS)),
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
                container(
                    scrollable(
                        column(
                            state
                                .menu_items
                                .iter()
                                .map(|item| {
                                    if item.visible && item.label.len() > 0 {
                                        Some(Element::from(
                                            button(
                                                text!("{}", item.label)
                                                    .wrapping(text::Wrapping::None),
                                            )
                                            .style(button_style)
                                            .width(Length::Fill)
                                            .on_press(Message::MenuEntryPressed(item.id)),
                                        ))
                                    } else {
                                        None
                                    }
                                })
                                .flatten(),
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
                    iced::widget::container::primary(theme)
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
        let activator: Element<Message> =
            iced::widget::mouse_area(space().height(2).width(Length::Fill))
                .on_enter(Message::UnCollapse)
                .into();
        let main = stack![
            activator,
            container(column![vertical(), workspaces(state),]).style(move |_| main_style)
        ]
        .width(MAIN_WIDTH)
        .height(Length::Fill);
        row![horizontal(), main, horizontal()].into()
    }
}

pub fn start_app() {
    // unwrap here to stop app from launching if we can't stop it
    let mut signal = Signals::new([Signal::Term, Signal::Int, Signal::Quit, Signal::Hup]).unwrap();
    smol::spawn(async move {
        signal.next().await;
        cleanup().await;
    })
    .detach();
    let mut app =
        iced_layershell::application(AppState::default, || APP_NAME.to_string(), update, view)
            .subscription(|state| {
                Subscription::batch([
                    clock_subscription(),
                    ipc_subscription(),
                    tray_subscription(),
                    window_hover_subscription(),
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
            .theme(theme::theme);
    if let Some(font_name) =
        CONFIG.with_borrow(|config| config.as_ref().and_then(|config| config.font.clone()))
    {
        let family = font_kit::family_name::FamilyName::Title(font_name.clone());
        let properties = font_kit::properties::Properties::default();
        let loaded_font_data = font_kit::sources::fontconfig::FontconfigSource::new()
            .select_best_match(&[family], &properties)
            .ok()
            .and_then(|handle| handle.load().ok())
            .and_then(|font| font.copy_font_data());
        if let Some(font_data) = loaded_font_data {
            app = app.font((*font_data).clone()).default_font(iced::Font {
                family: font::Family::Name(font_name.clone().leak()),
                ..Default::default()
            });
        }
    }
    app.run().unwrap();
}

async fn cleanup() {
    if SOCKET_MADE.get().is_some_and(|a| *a) {
        let path = get_sock_path();
        let result = fs::remove_file(&path).await;
        let path_str = path.to_string_lossy().to_string();
        let _ = result.inspect_err(|e| {
            eprintln!(
                "Error while deleting ipc socket: {e}
                Make sure to delete {path_str} so the app can function properly on next relaunch.",
            )
        });
    }
    process::exit(0);
}
