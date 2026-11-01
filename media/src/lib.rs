use futures_util::{Stream, StreamExt, future, stream};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, time::Duration};
use zbus::{
    Connection, Result,
    fdo::DBusProxy,
    proxy,
    zvariant::{self, Error as ZvariantError, OwnedValue},
};

const MEDIA_BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";

pub async fn query_players() -> Result<Vec<String>> {
    let connection = Connection::session().await?;
    let dbus_proxy = DBusProxy::new(&connection).await?;
    let names = dbus_proxy.list_names().await?;
    Ok(names
        .into_iter()
        .filter_map(|name| name.strip_prefix(MEDIA_BUS_PREFIX).map(|s| s.to_string()))
        .collect())
}

pub async fn player_from_name<'a>(name: &str) -> Result<PlayerProxy<'a>> {
    let connection = Connection::session().await?;
    let destination = format!("{}{}", MEDIA_BUS_PREFIX, name);
    PlayerProxy::new(&connection, destination).await
}

pub async fn get_best_player() -> Result<Option<PlayerProxy<'static>>> {
    let name = match query_players().await?.into_iter().next() {
        Some(n) => n,
        None => return Ok(None),
    };
    player_from_name(&name).await.map(|x| Some(x))
}

fn deserialize_microseconds<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let payload = match zvariant::Value::deserialize(deserializer)? {
        zvariant::Value::I64(payload) => u64::try_from(payload).map_err(|_| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Signed(payload),
                &"a non-negative number of microseconds",
            )
        })?,
        zvariant::Value::U64(payload) => payload,
        _ => {
            return Err(serde::de::Error::invalid_type(
                serde::de::Unexpected::Other("non-integer variant"),
                &"an integer number of microseconds",
            ));
        }
    };
    Ok(Some(Duration::from_micros(payload)))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, zvariant::Type, Hash)]
#[zvariant(signature = "a{sv}")]
#[serde(default)]
pub struct MprisMetadata {
    #[serde(
        rename = "xesam:artist",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zbus::zvariant::as_value::optional::deserialize"
    )]
    pub artist: Option<Vec<String>>,

    #[serde(
        rename = "xesam:album",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zbus::zvariant::as_value::optional::deserialize"
    )]
    pub album: Option<String>,

    #[serde(
        rename = "mpris:artUrl",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zbus::zvariant::as_value::optional::deserialize"
    )]
    pub art_url: Option<String>,

    #[serde(
        rename = "xesam:title",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zbus::zvariant::as_value::optional::deserialize"
    )]
    pub title: Option<String>,

    #[serde(
        rename = "mpris:length",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_microseconds"
    )]
    pub length: Option<Duration>,
}

impl TryFrom<OwnedValue> for MprisMetadata {
    type Error = ZvariantError;

    fn try_from(value: OwnedValue) -> std::result::Result<Self, Self::Error> {
        let fields = zvariant::Dict::try_from(value)?;
        let encoded = zvariant::to_bytes_for_signature(
            zvariant::serialized::Context::new_dbus(zvariant::LE, 0),
            "a{sv}",
            &fields,
        )?;
        match encoded.deserialize_for_signature("a{sv}") {
            Ok((metadata, _)) => Ok(metadata),
            Err(e) => Err(e),
        }
    }
}

#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
pub trait Player {
    fn next(&self) -> Result<()>;
    fn previous(&self) -> Result<()>;
    fn pause(&self) -> Result<()>;
    fn play_pause(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn play(&self) -> Result<()>;
    fn seek(&self, offset: i64) -> Result<()>;
    fn set_position(&self, track_id: zbus::zvariant::ObjectPath<'_>, position: i64) -> Result<()>;
    fn open_uri(&self, uri: &str) -> Result<()>;

    #[zbus(signal)]
    fn seeked(&self, position: i64) -> Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> Result<String>;
    #[zbus(property)]
    fn loop_status(&self) -> Result<String>;
    #[zbus(property)]
    fn set_loop_status(&self, value: &str) -> Result<()>;
    #[zbus(property)]
    fn rate(&self) -> Result<f64>;
    #[zbus(property)]
    fn set_rate(&self, value: f64) -> Result<()>;
    #[zbus(property)]
    fn shuffle(&self) -> Result<bool>;
    #[zbus(property)]
    fn set_shuffle(&self, value: bool) -> Result<()>;
    #[zbus(property, name = "Metadata")]
    fn metadata_stream(&self) -> Result<MprisMetadata>;
    #[zbus(property)]
    fn volume(&self) -> Result<f64>;
    #[zbus(property)]
    fn set_volume(&self, value: f64) -> Result<()>;
    #[zbus(property)]
    fn position(&self) -> Result<i64>;
    #[zbus(property)]
    fn minimum_rate(&self) -> Result<f64>;
    #[zbus(property)]
    fn maximum_rate(&self) -> Result<f64>;
    #[zbus(property)]
    fn can_go_next(&self) -> Result<bool>;
    #[zbus(property)]
    fn can_go_previous(&self) -> Result<bool>;
    #[zbus(property)]
    fn can_play(&self) -> Result<bool>;
    #[zbus(property)]
    fn can_pause(&self) -> Result<bool>;
    #[zbus(property)]
    fn can_seek(&self) -> Result<bool>;
    #[zbus(property)]
    fn can_control(&self) -> Result<bool>;
    #[zbus(property)]
    fn metadata(&self) -> Result<MprisMetadata>;
}

#[derive(Clone, Debug)]
pub struct Player {
    pub proxy: PlayerProxy<'static>,
    #[allow(dead_code)]
    name: String,
}

/// Message indicating whether a player bus name was acquired (added) or lost (removed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerMessage {
    Added(String),
    Removed(String),
}

async fn player_events() -> Result<impl Stream<Item = PlayerMessage>> {
    let connection = Connection::session().await?;
    let dbus_proxy = DBusProxy::new(&connection).await?;

    Ok(dbus_proxy
        .receive_name_owner_changed()
        .await?
        .filter_map(|signal| async move {
            let (name, old_owner, new_owner): (String, String, String) =
                signal.message().body().deserialize().ok()?;
            let name = name.strip_prefix(MEDIA_BUS_PREFIX)?.to_string();
            match (old_owner.is_empty(), new_owner.is_empty()) {
                (_, false) => Some(PlayerMessage::Added(name)),
                (false, true) => Some(PlayerMessage::Removed(name)),
                _ => None,
            }
        }))
}

#[derive(Debug)]
struct PlaybackMessage {
    name: String,
    status: String,
}

enum StreamEvent {
    Player(PlayerMessage),
    Playback(PlaybackMessage),
}

type PlaybackStream = stream::BoxStream<'static, PlaybackMessage>;

async fn playback_stream(name: String, proxy: PlayerProxy<'static>) -> PlaybackStream {
    let statuses = proxy.receive_playback_status_changed().await;
    statuses
        .filter_map(move |changed| {
            let name = name.clone();
            async move {
                let status = changed.get().await.ok()?;
                Some(PlaybackMessage { name, status })
            }
        })
        .boxed()
}

async fn add_player(players: &mut BTreeMap<String, PlayerProxy<'static>>, name: String) -> bool {
    let Ok(proxy) = player_from_name(&name).await else {
        return false;
    };
    players.insert(name, proxy);
    true
}

async fn subscribe_playback(
    players: &BTreeMap<String, PlayerProxy<'static>>,
) -> stream::SelectAll<PlaybackStream> {
    let mut streams = stream::SelectAll::new();
    for (name, proxy) in players {
        streams.push(playback_stream(name.clone(), proxy.clone()).await);
    }
    streams
}

async fn select_player(players: &BTreeMap<String, PlayerProxy<'static>>) -> Option<String> {
    let mut fallback = None;
    for (name, proxy) in players {
        fallback.get_or_insert_with(|| name.clone());
        if matches!(proxy.playback_status().await, Ok(status) if status == "Playing") {
            return Some(name.clone());
        }
    }
    fallback
}

pub async fn player_stream() -> Result<impl Stream<Item = Player>> {
    let events = player_events().await?.boxed();

    let connection = Connection::session().await?;
    let dbus_proxy = DBusProxy::new(&connection).await?;
    let names = dbus_proxy.list_names().await?;
    let mut players = BTreeMap::new();

    for name in names {
        if let Some(name) = name.strip_prefix(MEDIA_BUS_PREFIX) {
            add_player(&mut players, name.to_string()).await;
        }
    }
    let playback_streams = subscribe_playback(&players).await;

    let state = (events, players, playback_streams, None);
    Ok(stream::unfold(
        state,
        |(mut events, mut players, mut playback_streams, mut current)| async move {
            loop {
                if current.is_none() {
                    if let Some(name) = select_player(&players).await {
                        current = Some(name.clone());
                        return Some((
                            Player {
                                proxy: players[&name].clone(),
                                name,
                            },
                            (events, players, playback_streams, current),
                        ));
                    }
                }

                let event = if playback_streams.is_empty() {
                    events.next().await.map(StreamEvent::Player)
                } else {
                    let player_event = events.next();
                    let playback_event = playback_streams.next();
                    futures_util::pin_mut!(player_event, playback_event);
                    match future::select(player_event, playback_event).await {
                        future::Either::Left((event, _)) => event.map(StreamEvent::Player),
                        future::Either::Right((event, _)) => event.map(StreamEvent::Playback),
                    }
                };

                match event {
                    Some(StreamEvent::Player(PlayerMessage::Added(name))) => {
                        let replaces_current = current.as_deref() == Some(name.as_str());
                        if add_player(&mut players, name).await {
                            playback_streams = subscribe_playback(&players).await;
                            if replaces_current {
                                current = None;
                            }
                        }
                    }
                    Some(StreamEvent::Player(PlayerMessage::Removed(name))) => {
                        players.remove(&name);
                        playback_streams = subscribe_playback(&players).await;
                        if current.as_deref() == Some(name.as_str()) {
                            current = None;
                        }
                    }
                    Some(StreamEvent::Playback(PlaybackMessage { name, status })) => {
                        let Some(proxy) = players.get(&name) else {
                            continue;
                        };
                        if status != "Playing" {
                            continue;
                        }
                        if current.as_deref() != Some(name.as_str()) {
                            current = Some(name.clone());
                            return Some((
                                Player {
                                    proxy: proxy.clone(),
                                    name,
                                },
                                (events, players, playback_streams, current),
                            ));
                        }
                    }
                    None => return None,
                }
            }
        },
    ))
}
