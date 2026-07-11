mod config;
mod consts;
mod helper;
mod state;
mod theme;
use crate::{
    consts::{APP_NAME, COLLAPSE_TIME, MARGINS, UP_TRAVEL, WINDOW_HEIGHT, WINDOW_WIDTH},
    state::AppState,
    theme::default_theme,
};
use chrono::Timelike;
use compositor_bridge::{self, CompositorEvent, state::Workspace};
use iced::{
    Element, Subscription, Task, Theme,
    futures::{SinkExt, channel::mpsc},
    widget::{
        Row, column, container, row,
        space::{horizontal, vertical},
        text,
    },
};
use iced_layershell::{reexport::Anchor, settings::LayerShellSettings, to_layer_message};
use std::sync::Arc;
use theme::Rounded;

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    CompositorMessage(CompositorEvent),
    ToggleCollapse,
    CollapseUpdate,
    Refresh,
}

/// Iced subscription for ipc with a wayland compositor
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
    Subscription::run(|| {
        iced::stream::channel::<Message>(10, |mut output: mpsc::Sender<Message>| async move {
            loop {
                output.send(Message::Refresh).await.unwrap();
                let now = chrono::Local::now();
                let next_update = now
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
                    + chrono::Duration::minutes(1);
                smol::Timer::after((next_update - now).to_std().unwrap()).await;
            }
        })
    })
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

// most readable rust code competition
//
/// This is basically a recursive "animation" of the window size, taking how long it's been since
/// the collapse event, and lerping the size based on that. multiplied by -1 if it's being
/// uncollapsed instread of collapsed
///
/// TODO: use the actual iced animation type. I'm too lazy to figure this out rn and I found out
/// about it's existance only after I wrote this mess.
fn collapse_update(state: &mut AppState) -> Task<Message> {
    let diff = std::time::Instant::now() - state.collapsed_time;
    if diff < COLLAPSE_TIME {
        let margin = WINDOW_HEIGHT as i32
            - ((diff.as_millis()) as f32 / (COLLAPSE_TIME.as_millis()) as f32 * UP_TRAVEL as f32)
                as i32
                * if state.collapsed { 1 } else { -1 };
        Task::batch([
            Task::done(Message::SizeChange((
                WINDOW_WIDTH,
                margin.try_into().unwrap(),
            ))),
            Task::future(async {
                // 60fps is 1 update every ~16ms
                smol::Timer::after(std::time::Duration::from_millis(16)).await;
                Message::CollapseUpdate
            }),
        ])
    } else {
        let margin = WINDOW_HEIGHT - if state.collapsed { UP_TRAVEL } else { 1 };
        Task::done(Message::SizeChange((
            WINDOW_WIDTH,
            margin.try_into().unwrap(),
        )))
    }
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
            state.collapsed = !state.collapsed;
            state.collapsed_time = std::time::Instant::now();
            Task::done(Message::CollapseUpdate)
        }
        Message::CollapseUpdate => collapse_update(state),
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
        row![
            Element::from(Row::from_iter(state.workspaces.iter().map(|w| {
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
            }))),
            Element::from(text!("{:0>2}:{:0>2}", time.hour(), time.minute()))
        ]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(MARGINS),
    );
    let contents = if !state.collapsed {
        Element::from(column![
            row![text!("{}", window_text), horizontal(),],
            iced::widget::button(Element::from(text!("test"))).on_press(Message::ToggleCollapse),
            vertical(),
            workspaces
        ])
    } else {
        workspaces
    };
    container(contents)
        .align_y(iced::alignment::Vertical::Center)
        .style(|_| {
            let palette = default_theme(state.mode).palette();
            container::Style::default()
                .background(palette.background)
                .rounded()
        })
        .height(iced::Length::Fill)
        .padding(iced::padding::horizontal(MARGINS).vertical(MARGINS / 2))
        .into()
}

fn main() {
    iced_layershell::application(AppState::default, || APP_NAME.to_string(), update, view)
        .subscription(|state| {
            Subscription::batch(
                ([clock_subscription(state), compositor_subscription(state)]).into_iter(),
            )
        })
        .settings(iced_layershell::settings::Settings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top,
                layer: iced_layershell::reexport::Layer::Top,
                size: Some((WINDOW_WIDTH, WINDOW_HEIGHT - UP_TRAVEL)),
                exclusive_zone: (WINDOW_HEIGHT - UP_TRAVEL) as i32,
                keyboard_interactivity: iced_layershell::reexport::KeyboardInteractivity::None,
                start_mode: iced_layershell::settings::StartMode::Active,
                margin: (3, 0, 0, 0),
                ..Default::default()
            },
            ..Default::default()
        })
        .theme(theme::theme)
        .run()
        .unwrap();
}
