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
    name: String,
}

impl Player {
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug)]
pub enum PlayerEvent {
    Added {
        player: Player,
        metadata: Option<MprisMetadata>,
    },
    Removed(String),
    MetadataChanged {
        name: String,
        metadata: Option<MprisMetadata>,
    },
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

type MetadataStream = stream::BoxStream<'static, (String, MprisMetadata)>;

enum StreamEvent {
    Player(PlayerMessage),
    Metadata((String, MprisMetadata)),
}

async fn metadata_stream(name: String, proxy: PlayerProxy<'static>) -> MetadataStream {
    proxy
        .receive_metadata_changed()
        .await
        .filter_map(move |changed| {
            let name = name.clone();
            async move { changed.get().await.ok().map(|metadata| (name, metadata)) }
        })
        .boxed()
}

async fn subscribe_metadata(
    players: &BTreeMap<String, Player>,
) -> stream::SelectAll<MetadataStream> {
    let mut streams = stream::SelectAll::new();
    for (name, player) in players {
        streams.push(metadata_stream(name.clone(), player.proxy.clone()).await);
    }
    streams
}

pub async fn player_stream() -> Result<impl Stream<Item = PlayerEvent>> {
    let events = player_events().await?.boxed();

    let connection = Connection::session().await?;
    let dbus_proxy = DBusProxy::new(&connection).await?;
    let names = dbus_proxy.list_names().await?;
    let mut players = BTreeMap::new();
    let mut initial_events = Vec::new();

    for name in names {
        if let Some(name) = name.strip_prefix(MEDIA_BUS_PREFIX) {
            let Ok(proxy) = player_from_name(name).await else {
                continue;
            };
            let metadata = proxy.metadata().await.ok();
            let player = Player {
                proxy,
                name: name.to_string(),
            };
            players.insert(name.to_string(), player.clone());
            initial_events.push(PlayerEvent::Added { player, metadata });
        }
    }

    let metadata_streams = subscribe_metadata(&players).await;

    let state = (
        events,
        players,
        metadata_streams,
        initial_events.into_iter(),
    );
    Ok(stream::unfold(
        state,
        |(mut events, mut players, mut metadata_streams, mut initial_events)| async move {
            loop {
                if let Some(event) = initial_events.next() {
                    return Some((event, (events, players, metadata_streams, initial_events)));
                }

                let event = if metadata_streams.is_empty() {
                    events.next().await.map(StreamEvent::Player)
                } else {
                    let player_event = events.next();
                    let metadata_event = metadata_streams.next();
                    futures_util::pin_mut!(player_event, metadata_event);
                    match future::select(player_event, metadata_event).await {
                        future::Either::Left((event, _)) => event.map(StreamEvent::Player),
                        future::Either::Right((event, _)) => event.map(StreamEvent::Metadata),
                    }
                };

                match event {
                    Some(StreamEvent::Player(PlayerMessage::Added(name))) => {
                        let Ok(proxy) = player_from_name(&name).await else {
                            continue;
                        };
                        let metadata = proxy.metadata().await.ok();
                        let player = Player {
                            proxy,
                            name: name.clone(),
                        };
                        players.insert(name, player.clone());
                        metadata_streams = subscribe_metadata(&players).await;
                        return Some((
                            PlayerEvent::Added { player, metadata },
                            (events, players, metadata_streams, initial_events),
                        ));
                    }
                    Some(StreamEvent::Player(PlayerMessage::Removed(name))) => {
                        players.remove(&name);
                        metadata_streams = subscribe_metadata(&players).await;
                        return Some((
                            PlayerEvent::Removed(name),
                            (events, players, metadata_streams, initial_events),
                        ));
                    }
                    Some(StreamEvent::Metadata((name, metadata))) => {
                        return Some((
                            PlayerEvent::MetadataChanged {
                                name,
                                metadata: Some(metadata),
                            },
                            (events, players, metadata_streams, initial_events),
                        ));
                    }
                    None => return None,
                }
            }
        },
    ))
}
