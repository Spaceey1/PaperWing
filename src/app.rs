use crate::config::CONFIG;
use crate::consts::{DEFAULT_PEEK_TIMEOUT, MENU_WIDTH};
use crate::elements::{self, *};
use crate::ipc::{SOCKET_MADE, get_sock_path};
use crate::state::BarState;
use crate::state::Message;
use crate::theme::BackgroundContainer;
use crate::tray::get_tray_host;
use crate::{
    consts::{APP_NAME, UP_TRAVEL, WINDOW_WIDTH},
    state::AppState,
    theme::{self},
};
use crate::{subscriptions::*, tray};
use async_signal::{Signal, Signals};
use compositor_bridge::CompositorEvent;
use compositor_bridge::state::Workspace;
use iced::Length;
use iced::futures::StreamExt;
use iced::{
    Element, Subscription, Task, alignment,
    widget::{container, row, space::horizontal},
};
use iced_layershell::reexport::core::font;
use iced_layershell::{reexport::Anchor, settings::LayerShellSettings};
use rustsni::TrayEvent;
use smol::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
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

/// Requests a transition of the bar to `target`. If the transition is actually a change,
/// a single compositor resize event is emitted (either immediately or when the internal
/// animation finishes, depending on `resize_now`), while the visual animation is driven
/// internally by [`Message::AnimationUpdate`].
fn request_bar_state(state: &mut AppState, target: BarState, resize_now: bool) -> Task<Message> {
    let current = state.bar_state.value();
    if current == target {
        return Task::none();
    }
    state.now = std::time::Instant::now();
    state.bar_state.go_mut(target, state.now);
    if target == BarState::Collapsed && !resize_now {
        state.resize_pending = true;
    } else if resize_now {
        return Task::done(Message::SizeChange((WINDOW_WIDTH, target.window_height())));
    }
    Task::none()
}

fn collapse(state: &mut AppState) -> Task<Message> {
    state.peek_generation = state.peek_generation.wrapping_add(1);
    let resize = request_bar_state(state, BarState::Collapsed, false);
    let after = Task::done(Message::LayerChange(iced_layershell::reexport::Layer::Top));
    if state.menu_open.value() {
        Task::batch([resize, Task::done(Message::CloseTray), after])
    } else {
        Task::batch([resize, after])
    }
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::CompositorMessage(event) => {
            match event {
                CompositorEvent::WorkspacesChanged(workspaces) => {
                    let mut hasher = DefaultHasher::new();
                    state.focused_workspaces.hash(&mut hasher);
                    let hash1 = hasher.finish();
                    update_workspace_state(state, workspaces);
                    let mut hasher = DefaultHasher::new();
                    state.focused_workspaces.hash(&mut hasher);
                    let hash2 = hasher.finish();
                    if hash1 != hash2 {
                        Task::done(Message::TriggerPeek)
                    } else {
                        Task::none()
                    }
                }
                CompositorEvent::FocusedWindowChanged(window) => {
                    let Some(old) = state.focused_window.clone() else {
                        state.focused_window = Some(window);
                        return Task::none();
                    };
                    state.focused_window = Some(window);
                    if Some(old.id) == state.focused_window.as_ref().map(|f| f.id) {
                        Task::none()
                    } else {
                        Task::done(Message::TriggerPeek)
                    }
                }
                CompositorEvent::WorkspaceFocusChanged() => {
                    // I don't get information which workspace got unfocused, so have to request full state of all workspaces again
                    Task::future(async { compositor_bridge::get_workspaces().await }).map(
                        |result| match result {
                            Ok(workspaces) => Message::CompositorMessage(
                                CompositorEvent::WorkspacesChanged(workspaces),
                            ),
                            Err(e) => {
                                eprintln!("Failed to get workspaces: {e}");
                                Message::Refresh
                            }
                        },
                    )
                }
            }
        }
        Message::ToggleCollapse => Task::done(match state.bar_state.value() {
            BarState::Collapsed => Message::UnCollapse,
            _ => Message::Collapse,
        }),
        Message::UnCollapse => {
            state.peek_generation = state.peek_generation.wrapping_add(1);
            let resize = request_bar_state(state, BarState::Expanded, true);
            let after = Task::done(Message::LayerChange(
                iced_layershell::reexport::Layer::Overlay,
            ));
            Task::batch([resize, after])
        }
        Message::Collapse => collapse(state),
        Message::TriggerPeek => {
            state.peek_generation = state.peek_generation.wrapping_add(1);
            if state.bar_state.value() == BarState::Expanded {
                return Task::none();
            }
            let resize = if state.bar_state.value() == BarState::Collapsed {
                state.resize_pending = false;
                request_bar_state(state, BarState::Peek, true)
            } else {
                Task::none()
            };
            let generation = state.peek_generation;
            let timeout = CONFIG
                .with_borrow(|config| config.peek_timeout.unwrap_or(DEFAULT_PEEK_TIMEOUT))
                .round() as u64
                * 1000;
            let timer = Task::future(async move {
                smol::Timer::after(std::time::Duration::from_millis(timeout)).await;
                Message::PeekTimeout(generation)
            });
            Task::batch([resize, timer])
        }
        Message::PeekTimeout(id) => {
            if id == state.peek_generation && state.bar_state.value() == BarState::Peek {
                state.peek_generation = state.peek_generation.wrapping_add(1);
                state.now = std::time::Instant::now();
                state.bar_state.go_mut(BarState::Collapsed, state.now);
                state.resize_pending = true;
            }
            Task::none()
        }
        Message::MouseLeave => {
            if state.bar_state.value() != BarState::Collapsed {
                collapse(state)
            } else {
                Task::none()
            }
        }
        Message::AnimationUpdate => {
            state.now = std::time::Instant::now();
            if !state.bar_state.is_animating(state.now) && state.resize_pending {
                state.resize_pending = false;
                Task::done(Message::SizeChange((
                    WINDOW_WIDTH,
                    BarState::Collapsed.window_height(),
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
    const MAIN_WIDTH: u32 = WINDOW_WIDTH - MENU_WIDTH;
    let target = state.bar_state.value();
    let animated_height = state
        .bar_state
        .interpolate_with(BarState::visual_height, state.now)
        .round() as u32;
    let content = match target {
        BarState::Collapsed => elements::collapsed_content(state),
        BarState::Peek => elements::peek_content(state),
        BarState::Expanded => elements::expanded_content(state),
    };
    let main = container(container(content).width(Length::Fill).height(Length::Fill))
        .width(MAIN_WIDTH)
        .height(animated_height as f32)
        .align_y(alignment::Vertical::Top)
        .clip(true)
        .style(|theme| container::primary(theme).background_container(theme));
    let mut main_row = row![horizontal(), main,];
    let menu_open = state.menu_open.is_animating(state.now) || state.menu_open.value();
    if menu_open {
        main_row = main_row.push(tray_menu(state));
    }
    main_row.push(horizontal()).into()
}

pub fn start_app() {
    // unwrap here to stop app from launching if we can't stop it
    let mut signal = Signals::new([Signal::Term, Signal::Int, Signal::Quit, Signal::Hup]).unwrap();
    smol::spawn(async move {
        signal.next().await;
        cleanup().await;
    })
    .detach();
    let monitor = CONFIG.with_borrow(|c| c.display.clone());
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
                    layer: iced_layershell::reexport::Layer::Top,
                    // move up 1 pixel offscreen to hide the top border
                    margin: (-1, 0, 0, 0),
                    // expand by 1 pixel to compensate
                    // for the size lost offscreen
                    size: Some((WINDOW_WIDTH, UP_TRAVEL + 1)),
                    exclusive_zone: 0,
                    keyboard_interactivity: iced_layershell::reexport::KeyboardInteractivity::None,
                    start_mode: if let Some(monitor) = monitor {
                        iced_layershell::settings::StartMode::TargetScreen(monitor)
                    } else {
                        iced_layershell::settings::StartMode::Active
                    },
                    events_transparent: false,
                },
                ..Default::default()
            })
            .theme(theme::theme);
    if let Some(font_name) = CONFIG.with_borrow(|config| config.font.clone()) {
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
    if let Some(host) = tray::HOST.get() {
        let mut host = host.write().await;
        if let Err(e) = host.shutdown() {
            eprintln!("{e}");
        };
    }
    process::exit(0);
}
