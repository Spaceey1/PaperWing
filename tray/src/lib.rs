//! Async StatusNotifierWatcher and StatusNotifierHost.

mod icon;
mod item;
mod menu;
mod watcher;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use zbus::Connection;

pub use icon::IconPixmap;
pub use item::{ItemId, ToolTip, TrayItem};
pub use menu::{MenuNode, PropValue};

/// Changes observed by the tray watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    ItemAdded(ItemId),
    ItemChanged(ItemId),
    ItemRemoved(ItemId),
    MenuChanged(ItemId),
    MenuActivationRequested(ItemId),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tray D-Bus error: {0}")]
    Dbus(#[from] zbus::Error),
    #[error("tray D-Bus method error: {0}")]
    Fdo(#[from] zbus::fdo::Error),
    #[error("another StatusNotifierWatcher is already running")]
    WatcherAlreadyRunning,
    #[error("tray item not found: {0}")]
    ItemNotFound(ItemId),
    #[error("invalid tray item: {0}")]
    InvalidItem(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Async StatusNotifierWatcher and StatusNotifierHost.
#[derive(Clone)]
pub struct TrayHost {
    inner: Arc<TrayHostInner>,
}

struct TrayHostInner {
    connection: Connection,
    items: std::sync::Mutex<HashMap<ItemId, TrayItem>>,
    registered: std::sync::Mutex<BTreeSet<String>>,
    events: async_broadcast::Sender<TrayEvent>,
    initial_events: std::sync::Mutex<Option<async_broadcast::Receiver<TrayEvent>>>,
}

impl TrayHost {
    /// Connect to the session bus, register the watcher and host, and start
    /// listening for tray activity.
    pub async fn new() -> Result<Self> {
        let connection = Connection::session().await?;
        let (mut events, receiver) = async_broadcast::broadcast(128);
        events.set_overflow(true);
        let host = Self {
            inner: Arc::new(TrayHostInner {
                connection,
                items: std::sync::Mutex::new(HashMap::new()),
                registered: std::sync::Mutex::new(BTreeSet::new()),
                events,
                initial_events: std::sync::Mutex::new(Some(receiver)),
            }),
        };

        watcher::serve(host.inner.clone()).await?;
        watcher::register(&host.inner.connection).await?;
        watcher::announce_host(&host.inner.connection).await?;

        let discovery_inner = host.inner.clone();
        host.inner
            .connection
            .executor()
            .spawn(
                async move {
                    if let Err(error) = watcher::discover_existing_items(&discovery_inner).await {
                        eprintln!("tray item discovery failed: {error}");
                    }
                },
                "tray-item-discovery",
            )
            .detach();
        Ok(host)
    }

    /// Subscribe to changes produced by the watcher.
    pub fn subscribe(&self) -> async_broadcast::Receiver<TrayEvent> {
        let mut initial = self
            .inner
            .initial_events
            .lock()
            .expect("tray event receiver lock poisoned");
        if let Some(receiver) = initial.take() {
            return receiver;
        }
        drop(initial);

        let mut receiver = self.inner.events.new_receiver();
        receiver.set_overflow(true);
        receiver
    }

    /// Snapshot of all currently known tray items.
    pub fn items(&self) -> HashMap<ItemId, TrayItem> {
        self.inner
            .items
            .lock()
            .expect("tray item cache lock poisoned")
            .clone()
    }

    /// Get a single tray item.
    pub fn item(&self, id: &ItemId) -> Option<TrayItem> {
        self.inner
            .items
            .lock()
            .expect("tray item cache lock poisoned")
            .get(id)
            .cloned()
    }

    /// Fetch a tray item from the bus and add it to the cache.
    pub async fn add_item(&self, bus_name: &str, object_path: &str) -> Result<ItemId> {
        let owner = item::owner_name(&self.inner.connection, bus_name).await?;
        let item = item::load(
            &self.inner.connection,
            bus_name,
            bus_name,
            object_path,
            &owner,
        )
        .await?;
        let id = item.id.clone();
        self.inner
            .items
            .lock()
            .expect("tray item cache lock poisoned")
            .insert(id.clone(), item);
        let _ = self
            .inner
            .events
            .try_broadcast(TrayEvent::ItemAdded(id.clone()));
        Ok(id)
    }

    /// Activate an item.
    pub async fn activate(&self, id: &ItemId, x: i32, y: i32) -> Result<()> {
        self.call_item(id, "Activate", &(x, y)).await
    }

    /// Open an item's context menu.
    pub async fn context_menu(&self, id: &ItemId, x: i32, y: i32) -> Result<()> {
        self.call_item(id, "ContextMenu", &(x, y)).await
    }

    /// Secondary-activate an item.
    pub async fn secondary_activate(&self, id: &ItemId, x: i32, y: i32) -> Result<()> {
        self.call_item(id, "SecondaryActivate", &(x, y)).await
    }

    /// Scroll an item.
    pub async fn scroll(&self, id: &ItemId, delta: i32, orientation: &str) -> Result<()> {
        self.call_item(id, "Scroll", &(delta, orientation)).await
    }

    /// Provide an XDG activation token to an item.
    pub async fn provide_xdg_activation_token(&self, id: &ItemId, token: &str) -> Result<()> {
        self.call_item(id, "ProvideXdgActivationToken", &(token,))
            .await
    }

    /// Announce that the host is going away and release the watcher names.
    pub async fn shutdown(&self) -> Result<()> {
        watcher::shutdown(&self.inner.connection).await
    }

    /// Fetch a menu tree rooted at `parent_id`.
    pub async fn get_menu(&self, id: &ItemId, parent_id: i32) -> Result<Vec<MenuNode>> {
        let Some(item) = self.item(id) else {
            return Ok(Vec::new());
        };
        if !item.has_menu() {
            return Ok(Vec::new());
        }
        menu::layout(
            &self.inner.connection,
            &item.bus_name,
            &item.menu_path,
            parent_id,
        )
        .await
    }

    /// Send a click event to a menu entry.
    pub async fn menu_click(&self, id: &ItemId, menu_id: i32) -> Result<()> {
        let Some(item) = self.item(id) else {
            return Ok(());
        };
        if !item.has_menu() {
            return Ok(());
        }
        menu::event(
            &self.inner.connection,
            &item.bus_name,
            &item.menu_path,
            menu_id,
            "clicked",
        )
        .await
    }

    async fn call_item<B>(&self, id: &ItemId, method: &str, body: &B) -> Result<()>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let item = self
            .item(id)
            .ok_or_else(|| Error::ItemNotFound(id.clone()))?;
        let proxy = zbus::Proxy::new(
            &self.inner.connection,
            item.bus_name.as_str(),
            item.object_path.as_str(),
            item::SNI_INTERFACE,
        )
        .await?;
        proxy.call::<_, _, ()>(method, body).await?;
        Ok(())
    }
}
