use crate::ipc;
use crate::state::AppState;
use crate::state::BarState;
use crate::state::BatteryInfo;
use crate::state::BatteryStatus;
use crate::state::Message;
use crate::tray::tray_listener;
use battery::units::ratio::percent;
use chrono::Timelike;
use compositor_bridge::CompositorEvent;
use iced::futures::StreamExt;
use iced::{Event, event, mouse};
use iced::{
    Subscription,
    futures::{SinkExt, channel::mpsc},
};

pub fn window_hover_subscription() -> Subscription<Message> {
    event::listen_with(|event, _, _| match event {
        // Uncollapsing is handled in view (only when the mouse touches the top of the bar),
        // while leaving the bar collapses it again.
        Event::Mouse(mouse::Event::CursorLeft) => Some(Message::MouseLeave),
        _ => None,
    })
}

/// Iced subscription for ipc with a wayland compositor
pub fn compositor_subscription(_state: &AppState) -> Subscription<Message> {
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

pub fn clock_subscription() -> Subscription<Message> {
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

pub fn ipc_subscription() -> Subscription<Message> {
    Subscription::run(|| iced::stream::channel(10, ipc::start_listener))
}

pub fn animation_subscription(state: &AppState) -> Subscription<Message> {
    let battery_fluid_animating =
        state.bar_state.value() == BarState::Expanded && !state.batteries.is_empty();

    if state.bar_state.is_animating(state.now)
        || state.menu_open.is_animating(state.now)
        || battery_fluid_animating
    {
        iced::window::frames().map(|_| Message::AnimationUpdate)
    } else {
        Subscription::none()
    }
}

/// Polls the battery so the view can read it every frame without talking to
/// upower every frame.
pub fn battery_subscription() -> Subscription<Message> {
    Subscription::run(|| {
        iced::stream::channel(10, |mut output: mpsc::Sender<Message>| async move {
            loop {
                send_message(&mut output, Message::Batteries(poll_batteries())).await;
                smol::Timer::after(std::time::Duration::from_secs(10)).await;
            }
        })
    })
}

/// The manager is not `Send`, so it is built and dropped inside this call
/// instead of being held across an `await`.
fn poll_batteries() -> Vec<BatteryInfo> {
    let Ok(manager) = battery::Manager::new() else {
        return Vec::new();
    };
    let Ok(batteries) = manager.batteries() else {
        return Vec::new();
    };

    batteries
        .flatten()
        .map(|battery| BatteryInfo {
            level: battery.state_of_charge().get::<percent>() / 100.0,
            status: match battery.state() {
                battery::State::Charging => BatteryStatus::Charging,
                battery::State::Discharging => BatteryStatus::Discharging,
                battery::State::Empty => BatteryStatus::Empty,
                battery::State::Full => BatteryStatus::Full,
                _ => BatteryStatus::Unknown,
            },
        })
        .collect()
}

pub fn tray_subscription() -> Subscription<Message> {
    Subscription::run(|| iced::stream::channel(10, tray_listener))
}

pub fn media_subscription() -> Subscription<Message> {
    Subscription::run(|| {
        iced::stream::channel::<Message>(100, |mut output: mpsc::Sender<Message>| async move {
            let mut players = match media::player_stream().await {
                Ok(players) => Box::pin(players),
                Err(e) => {
                    eprintln!("Failed to connect to media player dbus: {e}");
                    return;
                }
            };

            while let Some(event) = players.next().await {
                let message = match event {
                    media::PlayerEvent::Added { player, metadata } => {
                        Message::PlayerAdded(player, metadata)
                    }
                    media::PlayerEvent::Removed(name) => Message::PlayerRemoved(name),
                    media::PlayerEvent::MetadataChanged { name, metadata } => {
                        Message::MediaChanged(name, metadata)
                    }
                };
                send_message(&mut output, message).await;
            }
        })
    })
}

/// Sends a message to the UI, ignoring the error returned when the subscription
/// has already been dropped.
async fn send_message(output: &mut mpsc::Sender<Message>, message: Message) {
    output.send(message).await.ok();
}
