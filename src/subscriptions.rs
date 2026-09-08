use crate::ipc;
use crate::state::AppState;
use crate::state::Message;
use crate::tray::tray_listiner;
use chrono::Timelike;
use compositor_bridge::CompositorEvent;
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
    if state.bar_state.is_animating(state.now) || state.menu_open.is_animating(state.now) {
        iced::window::frames().map(|_| Message::AnimationUpdate)
    } else {
        Subscription::none()
    }
}

pub fn tray_subscription() -> Subscription<Message> {
    Subscription::run(|| iced::stream::channel(10, tray_listiner))
}
