use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::stream::{SelectAll, select_all};
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::item;
use crate::{Error, Result, TrayEvent, TrayHostInner};

const WATCHER_NAMES: [&str; 2] = [
    "org.freedesktop.StatusNotifierWatcher",
    "org.kde.StatusNotifierWatcher",
];
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
const SNI_BUS_PREFIXES: [&str; 3] = [
    "org.kde.StatusNotifierItem-",
    "org.freedesktop.StatusNotifierItem-",
    "org.ayatana.StatusNotifierItem-",
];
const SNI_OBJECT_PATHS: [&str; 2] = [
    "/StatusNotifierItem",
    "/org/ayatana/NotificationItem",
];

pub(crate) async fn register(connection: &Connection) -> Result<()> {
    let mut acquired = Vec::new();
    for name in WATCHER_NAMES {
        let reply = match connection
            .request_name_with_flags(name, RequestNameFlags::DoNotQueue.into())
            .await
        {
            Ok(reply) => reply,
            Err(error) => {
                release_names(connection, &acquired).await;
                return Err(error.into());
            }
        };
        match reply {
            RequestNameReply::PrimaryOwner => acquired.push(name),
            RequestNameReply::AlreadyOwner => {}
            _ => {
                release_names(connection, &acquired).await;
                return Err(Error::WatcherAlreadyRunning);
            }
        }
    }

    let host_name = format!("org.freedesktop.StatusNotifierHost-{}", std::process::id());
    if let Err(error) = connection
        .request_name_with_flags(host_name, RequestNameFlags::DoNotQueue.into())
        .await
    {
        release_names(connection, &acquired).await;
        return Err(error.into());
    }
    Ok(())
}

async fn release_names(connection: &Connection, names: &[&str]) {
    for name in names {
        let _ = connection.release_name(*name).await;
    }
}

pub(crate) async fn shutdown(connection: &Connection) -> Result<()> {
    connection
        .emit_signal(
            None::<&str>,
            WATCHER_PATH,
            WATCHER_INTERFACE,
            "StatusNotifierHostUnregistered",
            &(),
        )
        .await?;
    connection
        .release_name(format!("org.freedesktop.StatusNotifierHost-{}", std::process::id()).as_str())
        .await?;
    for name in WATCHER_NAMES {
        connection.release_name(name).await?;
    }
    Ok(())
}

pub(crate) async fn announce_host(connection: &Connection) -> Result<()> {
    connection
        .emit_signal(
            None::<&str>,
            WATCHER_PATH,
            WATCHER_INTERFACE,
            "StatusNotifierHostRegistered",
            &(),
        )
        .await?;
    Ok(())
}

pub(crate) async fn serve(inner: Arc<TrayHostInner>) -> Result<()> {
    let object_server = inner.connection.object_server();
    object_server
        .at(WATCHER_PATH, Watcher::new(inner.clone()))
        .await?;

    let connection = inner.connection.clone();
    let task_inner = inner.clone();
    inner
        .connection
        .executor()
        .spawn(
            async move {
                let mut stream = match signal_stream(&connection).await {
                    Ok(stream) => stream,
                    Err(error) => {
                        eprintln!("tray signal stream failed: {error}");
                        return;
                    }
                };

                while let Some(message) = stream.next().await {
                    let message = match message {
                        Ok(message) => message,
                        Err(error) => {
                            eprintln!("tray signal error: {error}");
                            continue;
                        }
                    };
                    if let Err(error) = handle_signal(&task_inner, &connection, &message).await {
                        eprintln!("tray signal handling failed: {error}");
                    }
                }
            },
            "tray-watcher-listener",
        )
        .detach();
    Ok(())
}

async fn signal_stream(connection: &Connection) -> zbus::Result<SelectAll<MessageStream>> {
    let rules = [
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .build(),
        MatchRule::builder()
            .msg_type(Type::Signal)
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .arg(0, item::SNI_INTERFACE)?
            .build(),
        MatchRule::builder()
            .msg_type(Type::Signal)
            .interface(item::SNI_INTERFACE)?
            .build(),
        MatchRule::builder()
            .msg_type(Type::Signal)
            .interface("com.canonical.dbusmenu")?
            .build(),
    ];

    let mut streams = Vec::with_capacity(rules.len());
    for rule in rules {
        streams.push(MessageStream::for_match_rule(rule, connection, Some(128)).await?);
    }
    Ok(select_all(streams))
}

async fn handle_signal(
    inner: &Arc<TrayHostInner>,
    connection: &Connection,
    message: &zbus::Message,
) -> Result<()> {
    let header = message.header();
    let interface = header.interface().map(|value| value.as_str()).unwrap_or("");
    let member = header.member().map(|value| value.as_str()).unwrap_or("");
    let sender = header.sender().map(|value| value.as_str()).unwrap_or("");

    if interface == "org.freedesktop.DBus" && member == "NameOwnerChanged" {
        let (name, _, new_owner): (String, String, String) = message.body().deserialize()?;
        if new_owner.is_empty() {
            remove_items(inner, connection, &name).await?;
        } else if is_status_notifier_item_name(&name) {
            discover_bus_name(inner, connection, &name).await;
        }
        return Ok(());
    }

    if interface == "org.freedesktop.DBus.Properties" && member == "PropertiesChanged" {
        let (changed_interface, _, _): (
            String,
            std::collections::HashMap<String, zbus::zvariant::Value<'_>>,
            Vec<String>,
        ) = match message.body().deserialize() {
            Ok(value) => value,
            Err(_) => return Ok(()),
        };
        if changed_interface == item::SNI_INTERFACE {
            refresh_items(inner, connection, sender).await;
        }
        return Ok(());
    }

    if interface == "org.kde.StatusNotifierItem" {
        match member {
            "NewIcon" | "NewAttentionIcon" | "NewOverlayIcon" | "NewIconThemePath" | "NewTitle"
            | "NewStatus" | "NewToolTip" => refresh_items(inner, connection, sender).await,
            _ => {}
        }
        return Ok(());
    }

    if interface == "com.canonical.dbusmenu" && sender.is_empty() {
        return Ok(());
    }
    if interface == "com.canonical.dbusmenu" {
        let ids = ids_for_bus_name(inner, sender);
        match member {
            "LayoutUpdated" | "ItemsPropertiesUpdated" => {
                for id in ids {
                    let _ = inner.events.try_broadcast(TrayEvent::MenuChanged(id));
                }
            }
            "ItemActivationRequested" => {
                for id in ids {
                    let _ = inner
                        .events
                        .try_broadcast(TrayEvent::MenuActivationRequested(id));
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn ids_for_bus_name(inner: &Arc<TrayHostInner>, sender: &str) -> Vec<crate::ItemId> {
    inner
        .items
        .lock()
        .expect("tray item cache lock poisoned")
        .values()
        .filter(|item| item.bus_name == sender || item.owner_name == sender)
        .map(|item| item.id.clone())
        .collect()
}

pub(crate) async fn discover_existing_items(inner: &Arc<TrayHostInner>) -> Result<()> {
    let proxy = zbus::fdo::DBusProxy::new(&inner.connection).await?;
    let names = proxy.list_names().await?;
    for name in names {
        if is_status_notifier_item_name(name.as_str()) {
            discover_bus_name(inner, &inner.connection, name.as_str()).await;
        }
    }
    Ok(())
}

async fn discover_bus_name(inner: &Arc<TrayHostInner>, connection: &Connection, bus_name: &str) {
    for object_path in SNI_OBJECT_PATHS {
        if add_item(inner, connection, bus_name, object_path).await.is_ok() {
            break;
        }
    }
}

async fn add_item(
    inner: &Arc<TrayHostInner>,
    connection: &Connection,
    bus_name: &str,
    object_path: &str,
) -> Result<crate::ItemId> {
    let owner = item::owner_name(connection, bus_name).await?;
    let service_id = format!("{bus_name}{object_path}");
    let item = item::load(connection, &service_id, bus_name, object_path, &owner).await?;
    let id = item.id.clone();
    inner
        .registered
        .lock()
        .expect("tray registration lock poisoned")
        .insert(service_id.clone());
    insert_item(inner, item);
    connection
        .emit_signal(
            None::<&str>,
            WATCHER_PATH,
            WATCHER_INTERFACE,
            "StatusNotifierItemRegistered",
            &(service_id.as_str(),),
        )
        .await?;
    Ok(id)
}

fn insert_item(inner: &Arc<TrayHostInner>, item: crate::TrayItem) {
    let id = item.id.clone();
    let bus_name = item.bus_name.clone();
    let owner_name = item.owner_name.clone();
    let object_path = item.object_path.clone();
    let (stale_ids, existed) = {
        let mut items = inner
            .items
            .lock()
            .expect("tray item cache lock poisoned");
        let stale_ids: Vec<_> = items
            .iter()
            .filter(|(existing_id, existing)| {
                **existing_id != id
                    && (existing.bus_name == bus_name
                        || (existing.owner_name == owner_name
                            && existing.object_path == object_path))
            })
            .map(|(existing_id, _)| existing_id.clone())
            .collect();
        let existed = items.contains_key(&id);
        for stale_id in &stale_ids {
            items.remove(stale_id);
        }
        items.insert(id.clone(), item);
        (stale_ids, existed)
    };
    for stale_id in stale_ids {
        let _ = inner
            .events
            .try_broadcast(TrayEvent::ItemRemoved(stale_id));
    }
    let _ = inner.events.try_broadcast(if existed {
        TrayEvent::ItemChanged(id)
    } else {
        TrayEvent::ItemAdded(id)
    });
}

async fn refresh_items(inner: &Arc<TrayHostInner>, connection: &Connection, sender: &str) {
    let items: Vec<_> = inner
        .items
        .lock()
        .expect("tray item cache lock poisoned")
        .values()
        .filter(|item| item.owner_name == sender || item.bus_name == sender)
        .cloned()
        .collect();

    for old in items {
        match item::load(
            connection,
            &old.id.0,
            &old.bus_name,
            &old.object_path,
            &old.owner_name,
        )
        .await
        {
            Ok(new) => {
                inner
                    .items
                    .lock()
                    .expect("tray item cache lock poisoned")
                    .insert(new.id.clone(), new.clone());
                let _ = inner.events.try_broadcast(TrayEvent::ItemChanged(new.id));
            }
            Err(_) => {}
        }
    }
}

async fn remove_items(
    inner: &Arc<TrayHostInner>,
    connection: &Connection,
    bus_name: &str,
) -> Result<()> {
    {
        let mut registered = inner
            .registered
            .lock()
            .expect("tray registration lock poisoned");
        let prefix = format!("{bus_name}/");
        registered.retain(|service| service != bus_name && !service.starts_with(&prefix));
    }
    let ids: Vec<_> = inner
        .items
        .lock()
        .expect("tray item cache lock poisoned")
        .values()
        .filter(|item| item.bus_name == bus_name || item.owner_name == bus_name)
        .map(|item| item.id.clone())
        .collect();

    for id in ids {
        inner
            .items
            .lock()
            .expect("tray item cache lock poisoned")
            .remove(&id);
        let _ = inner
            .events
            .try_broadcast(TrayEvent::ItemRemoved(id.clone()));
        connection
            .emit_signal(
                None::<&str>,
                WATCHER_PATH,
                WATCHER_INTERFACE,
                "StatusNotifierItemUnregistered",
                &(id.0.as_str(),),
            )
            .await?;
    }
    Ok(())
}

pub(crate) struct Watcher {
    inner: Arc<TrayHostInner>,
}

impl Watcher {
    pub(crate) fn new(inner: Arc<TrayHostInner>) -> Self {
        Self { inner }
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    #[zbus(name = "RegisterStatusNotifierItem")]
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let (bus_name, object_path) = parse_service(&sender, &service);
        let service_id = format!("{bus_name}{object_path}");
        let is_new = self
            .inner
            .registered
            .lock()
            .expect("tray registration lock poisoned")
            .insert(service_id.clone());
        if !is_new {
            return Ok(());
        }
        let _ = self
            .inner
            .connection
            .emit_signal(
                None::<&str>,
                WATCHER_PATH,
                WATCHER_INTERFACE,
                "StatusNotifierItemRegistered",
                &(service_id.as_str(),),
            )
            .await;
        let inner = self.inner.clone();
        let connection = self.inner.connection.clone();
        self.inner
            .connection
            .executor()
            .spawn(
                async move {
                    let result = async {
                        let owner = item::owner_name(&connection, &bus_name).await?;
                        let item = item::load(
                            &connection,
                            &service_id,
                            &bus_name,
                            &object_path,
                            &owner,
                        )
                        .await?;
                        insert_item(&inner, item);
                        Ok::<_, crate::Error>(())
                    }
                    .await;
                    if let Err(error) = result {
                        eprintln!("tray item registration failed for {service}: {error}");
                    }
                },
                "tray-item-registration",
            )
            .detach();
        Ok(())
    }

    #[zbus(name = "RegisterStatusNotifierHost")]
    fn register_status_notifier_host(&self, _service: String) {}

    #[zbus(name = "GetHostServiceName")]
    fn get_host_service_name(&self) -> String {
        format!("org.freedesktop.StatusNotifierHost-{}", std::process::id())
    }

    #[zbus(property, name = "RegisteredStatusNotifierItems")]
    fn registered_items(&self) -> Vec<String> {
        self.inner
            .registered
            .lock()
            .expect("tray registration lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    #[zbus(property, name = "IsStatusNotifierHostRegistered")]
    fn is_host_registered(&self) -> bool {
        true
    }

    #[zbus(property, name = "ProtocolVersion")]
    fn protocol_version(&self) -> i32 {
        0
    }
}

fn parse_service(sender: &str, service: &str) -> (String, String) {
    if service.starts_with('/') {
        (sender.to_owned(), service.to_owned())
    } else if let Some(index) = service.find('/') {
        (service[..index].to_owned(), service[index..].to_owned())
    } else {
        (
            normalize_item_name(service),
            "/StatusNotifierItem".to_owned(),
        )
    }
}

fn normalize_item_name(name: &str) -> String {
    if name.starts_with("org.freedesktop.StatusNotifierItem")
        || name.starts_with("org.kde.StatusNotifierItem")
        || name.starts_with("org.ayatana.StatusNotifierItem")
        || name.starts_with(':')
    {
        name.to_owned()
    } else {
        format!("org.freedesktop.StatusNotifierItem-{name}")
    }
}

fn is_status_notifier_item_name(name: &str) -> bool {
    SNI_BUS_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_registration_names() {
        assert_eq!(
            normalize_item_name("4077-1"),
            "org.freedesktop.StatusNotifierItem-4077-1"
        );
        assert_eq!(
            normalize_item_name("org.kde.StatusNotifierItem-1"),
            "org.kde.StatusNotifierItem-1"
        );
        assert_eq!(
            normalize_item_name("org.ayatana.StatusNotifierItem-1"),
            "org.ayatana.StatusNotifierItem-1"
        );
        assert_eq!(normalize_item_name(":1.42"), ":1.42");
    }

    #[test]
    fn recognizes_status_notifier_item_names() {
        assert!(is_status_notifier_item_name(
            "org.kde.StatusNotifierItem-4077-1"
        ));
        assert!(is_status_notifier_item_name(
            "org.freedesktop.StatusNotifierItem-4077-1"
        ));
        assert!(is_status_notifier_item_name(
            "org.ayatana.StatusNotifierItem-4077-1"
        ));
        assert!(!is_status_notifier_item_name(":1.42"));
        assert!(!is_status_notifier_item_name(
            "org.kde.StatusNotifierWatcher"
        ));
    }

    #[test]
    fn parses_registration_forms() {
        assert_eq!(
            parse_service(":1.42", "/StatusNotifierItem"),
            (":1.42".to_owned(), "/StatusNotifierItem".to_owned())
        );
        assert_eq!(
            parse_service(":1.42", "com.example.App/SomePath"),
            ("com.example.App".to_owned(), "/SomePath".to_owned())
        );
        assert_eq!(
            parse_service(":1.42", "org.freedesktop.StatusNotifierItem-4077-1"),
            (
                "org.freedesktop.StatusNotifierItem-4077-1".to_owned(),
                "/StatusNotifierItem".to_owned()
            )
        );
    }
}
