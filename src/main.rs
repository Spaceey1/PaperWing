use core::task;
use std::iter::once;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::Timelike;
use compositor_bridge;
use compositor_bridge::CompositorEvent;
use compositor_bridge::state::Workspace;
use iced::futures::SinkExt;
use iced::futures::channel::mpsc;
use iced::widget::container::transparent;
use iced::widget::space::{horizontal, vertical};
use iced::widget::{Row, column, container, row, text};
use iced::{Element, Subscription, Task, Theme};
use iced_layershell::reexport::Anchor;
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::to_layer_message;

use crate::consts::{COLLAPSE_TIME, UP_TRAVEL};
use crate::state::AppState;
mod config;
mod consts;
mod helper;
mod state;

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    CompositorMessage(CompositorEvent),
    ToggleCollapse,
    CollapseUpdate,
    Refresh,
}

fn compositor_subscription(_state: &AppState) -> Subscription<Message> {
    Subscription::run(|| {
        iced::stream::channel::<Message>(100, |mut output: mpsc::Sender<Message>| async move {
            let (tx, rx) = smol::channel::unbounded::<CompositorEvent>();
            std::thread::spawn(move || {
                smol::block_on(async { compositor_bridge::start_event_stream(tx).await.unwrap() })
            });
            // using std::thread since the combination of smol::spawn + iced with smol feature +
            // iced_layershell blocks this from running if it's on the same os thread for some
            // fucking reason
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

fn clock_subscription(_state: &AppState) -> Subscription<Message> {
    iced::time::every(Duration::from_secs(1)).map(|_| Message::Refresh)
}

fn update_workspace_state(state: &mut AppState, workspaces: Vec<Arc<Workspace>>) {
    state.workspaces = workspaces;
    state.focused_workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| workspace.is_active)
        .map(|workspace| workspace.id)
        .collect();
}

// fn collapse_update(state: &mut AppState) -> Task<Message> {
//     let diff = chrono::Local::now() - state.collapsed_time;
//     if diff < COLLAPSE_TIME {
//         let margin = 100 - if state.collapsed {
//             diff.subsec_millis() / COLLAPSE_TIME.subsec_millis() * UP_TRAVEL
//         } else {
//             1
//         };
//         println!("1 {}", margin);
//         Task::batch([
//             // 1. Immediately apply this frame's margin change
//             Task::done(Message::SizeChange((margin.try_into().unwrap(), 400 ))),
//             // 2. Wait 16ms, then call TickCollapse again (The "Recursion")
//             Task::future(async {
//                 smol::Timer::after(std::time::Duration::from_millis(16)).await;
//                 // 60fps is 1 update every ~16ms
//                 Message::CollapseUpdate
//             }),
//         ])
//     } else {
//         let margin = 100 - if state.collapsed { UP_TRAVEL } else { 1 };
//         println!("2 {}", margin);
//         Task::done(Message::SizeChange((margin.try_into().unwrap(), 400)))
//     }
// }

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::CompositorMessage(event) => {
            println!("{:?}", event);
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
                            Ok(workspaces) => {
                                println!("{:?}", workspaces);
                                Message::CompositorMessage(CompositorEvent::WorkspacesChanged(
                                    workspaces,
                                ))
                            }
                            Err(e) => {
                                panic!("{}", e);
                            }
                        },
                    )
                    // print!("aaaaaaaaa");
                    // let workspaces = workspaces.collect();
                    // update_workspace_state(state, workspaces);
                }
            }
        }
        Message::ToggleCollapse => {
            state.collapsed = !state.collapsed;
            state.collapsed_time = chrono::Local::now();
            Task::done(Message::CollapseUpdate)
        }
        // Message::CollapseUpdate => collapse_update(state),
        _ => Task::none(),
    }
}

fn view(state: &AppState) -> Element<'_, Message> {
    let window_text = if state.focused_window.is_some() {
        &state.focused_window.as_ref().unwrap().title
    } else {
        &"".to_string()
    };
    let time = chrono::Local::now();
    let window_text = helper::truncate(window_text, 40);
    let workspaces = Element::from(
        Row::from_iter(
            state
                .workspaces
                .iter()
                .map(|w| {
                    container(horizontal().height(20))
                        .style(|theme: &Theme| {
                            let palette = theme.extended_palette();
                            container::Style::default().background(
                                if state.focused_workspaces.contains(&w.id) {
                                    palette.primary.base.color
                                } else {
                                    palette.background.base.color
                                },
                            )
                        })
                        .into()
                })
                .chain(once(Element::from(text!(
                    "{}:{}",
                    time.hour(),
                    time.minute()
                )))),
        )
        .height(30),
    );
    let contents = if !state.collapsed {
        Element::from(column![
            row![text!("{}", window_text), horizontal(),],
            iced::widget::button(Element::from(text!("test"))).on_press(Message::ToggleCollapse),
            vertical(),
            workspaces
        ])
    } else {
        Element::from(container(workspaces).style(transparent))
    };
    container(contents).style(transparent).into()
}

fn main() {
    iced_layershell::application(AppState::default, || "gay".to_string(), update, view)
        .subscription(|state| {
            Subscription::batch(
                ([clock_subscription(state), compositor_subscription(state)]).into_iter(),
            )
        })
        // .subscription(compositor_subscription)
        .settings(iced_layershell::settings::Settings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top,
                layer: iced_layershell::reexport::Layer::Top,
                size: Some((400, 300)),
                exclusive_zone: 20,
                keyboard_interactivity: iced_layershell::reexport::KeyboardInteractivity::None,
                start_mode: iced_layershell::settings::StartMode::Active,
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
        .unwrap();
}
