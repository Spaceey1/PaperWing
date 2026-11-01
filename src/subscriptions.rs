use crate::ipc;
use crate::state::AppState;
use crate::state::Message;
use crate::tray::tray_listener;
use chrono::Timelike;
use compositor_bridge::CompositorEvent;
use iced::futures::select;
use iced::futures::{FutureExt, StreamExt, stream::Stream};
use iced::{Event, event, mouse};
use iced::{
    Subscription,
    futures::{SinkExt, channel::mpsc},
};
use std::pin::Pin;

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

            // A player is handed over here when another one takes over while the
            // current one is being watched, so it does not have to be awaited again.
            let mut pending_player = None;

            loop {
                let player = match pending_player.take() {
                    Some(player) => player,
                    None => match players.next().await {
                        Some(player) => player,
                        None => break,
                    },
                };

                send_message(&mut output, Message::PlayerChanged(Some(player.clone()))).await;
                send_message(&mut output, Message::MediaChanged(None)).await;

                match watch_player(player, &mut players, &mut output).await {
                    PlayerWatchEnd::SwitchTo(player) => pending_player = Some(player),
                    PlayerWatchEnd::PlayerLost => clear_now_playing(&mut output).await,
                    PlayerWatchEnd::NextPlayer => {}
                    PlayerWatchEnd::Exhausted => break,
                }
            }

            clear_now_playing(&mut output).await;
        })
    })
}

/// Sends a message to the UI, ignoring the error returned when the subscription
/// has already been dropped.
async fn send_message(output: &mut mpsc::Sender<Message>, message: Message) {
    output.send(message).await.ok();
}

async fn clear_now_playing(output: &mut mpsc::Sender<Message>) {
    send_message(output, Message::PlayerChanged(None)).await;
    send_message(output, Message::MediaChanged(None)).await;
}

/// Why we stopped forwarding the events of a player.
enum PlayerWatchEnd {
    /// Another player became the one to display.
    SwitchTo(media::Player),
    /// The watched player left the bus.
    PlayerLost,
    /// This player can no longer be watched; pick the next one.
    NextPlayer,
    /// No players are left at all.
    Exhausted,
}

/// Forwards the metadata changes of `player` until it leaves the bus or another
/// player takes over.
async fn watch_player<S>(
    player: media::Player,
    players: &mut Pin<Box<S>>,
    output: &mut mpsc::Sender<Message>,
) -> PlayerWatchEnd
where
    S: Stream<Item = media::Player>,
{
    let mut metadata = player.proxy.receive_metadata_changed().await;
    let mut owner_changed = match player.proxy.inner().receive_owner_changed().await {
        Ok(owner_changed) => owner_changed,
        Err(e) => {
            eprintln!("Error watching media player owner: {e}");
            return PlayerWatchEnd::NextPlayer;
        }
    };

    loop {
        let mut player_event = players.next().fuse();
        let mut metadata_event = metadata.next().fuse();
        let mut owner_event = owner_changed.next().fuse();

        select! {
            new_player = player_event => {
                return match new_player {
                    Some(player) => PlayerWatchEnd::SwitchTo(player),
                    None => PlayerWatchEnd::Exhausted,
                };
            },
            changed = metadata_event => match changed {
                Some(changed) => match changed.get().await {
                    Ok(metadata) => send_message(output, Message::MediaChanged(Some(metadata))).await,
                    Err(e) => eprintln!("Error getting new metadata: {e}"),
                },
                None => return PlayerWatchEnd::NextPlayer,
            },
            owner = owner_event => {
                return match owner {
                    Some(_) => PlayerWatchEnd::PlayerLost,
                    None => PlayerWatchEnd::NextPlayer,
                };
            },
        }
    }
}
