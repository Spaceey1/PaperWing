use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::Result;
use crate::icon::IconPixmap;

pub(crate) const SNI_INTERFACE: &str = "org.kde.StatusNotifierItem";
const SNI_PROPERTIES: [&str; 16] = [
    "Category",
    "Id",
    "Title",
    "Status",
    "WindowId",
    "IconThemePath",
    "IconName",
    "IconPixmap",
    "AttentionIconName",
    "AttentionIconPixmap",
    "AttentionMovieName",
    "OverlayIconName",
    "OverlayIconPixmap",
    "ItemIsMenu",
    "Menu",
    "ToolTip",
];

/// Stable identifier for a tray item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemId(pub String);

impl std::fmt::Display for ItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Tooltip data published by an item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolTip {
    pub icon_name: String,
    pub icon_pixmap: Option<IconPixmap>,
    pub title: String,
    pub text: String,
}

/// Resolved StatusNotifierItem properties.
#[derive(Debug, Clone)]
pub struct TrayItem {
    pub id: ItemId,
    pub bus_name: String,
    pub owner_name: String,
    pub object_path: String,
    pub category: String,
    pub item_id: String,
    pub title: String,
    pub status: String,
    pub window_id: i32,
    pub icon_theme_path: String,
    pub icon_name: String,
    pub icon_pixmaps: Vec<IconPixmap>,
    pub attention_icon_name: String,
    pub attention_icon_pixmaps: Vec<IconPixmap>,
    pub attention_movie_name: String,
    pub overlay_icon_name: String,
    pub overlay_icon_pixmaps: Vec<IconPixmap>,
    pub item_is_menu: bool,
    pub menu_path: String,
    pub tooltip: ToolTip,
}

impl TrayItem {
    pub fn has_menu(&self) -> bool {
        !self.menu_path.is_empty() && self.menu_path != "/"
    }

    pub fn best_icon_pixmap(&self) -> Option<&IconPixmap> {
        self.icon_pixmaps
            .iter()
            .max_by_key(|pixmap| pixmap.width * pixmap.height)
    }

    pub fn best_overlay_icon_pixmap(&self) -> Option<&IconPixmap> {
        self.overlay_icon_pixmaps
            .iter()
            .max_by_key(|pixmap| pixmap.width * pixmap.height)
    }

    pub fn best_attention_icon_pixmap(&self) -> Option<&IconPixmap> {
        self.attention_icon_pixmaps
            .iter()
            .max_by_key(|pixmap| pixmap.width * pixmap.height)
    }

    pub fn icon_search_paths(&self) -> Vec<&str> {
        let mut paths = Vec::new();
        if !self.icon_theme_path.is_empty() {
            paths.push(self.icon_theme_path.as_str());
        }
        paths.push("/usr/share/pixmaps");
        paths
    }
}

pub(crate) async fn owner_name(connection: &zbus::Connection, bus_name: &str) -> Result<String> {
    let proxy = zbus::fdo::DBusProxy::new(connection).await?;
    let bus_name = bus_name
        .try_into()
        .map_err(|error: zbus::names::Error| zbus::Error::from(error))?;
    Ok(proxy.get_name_owner(bus_name).await?.to_string())
}

pub(crate) async fn load(
    connection: &zbus::Connection,
    service_id: &str,
    bus_name: &str,
    object_path: &str,
    owner_name: &str,
) -> Result<TrayItem> {
    let properties = zbus::fdo::PropertiesProxy::builder(connection)
        .destination(bus_name)?
        .path(object_path)?
        .build()
        .await?;
    let values = load_properties(&properties).await?;
    Ok(from_properties(
        service_id,
        bus_name,
        object_path,
        owner_name,
        &values,
    ))
}

async fn load_properties(
    properties: &zbus::fdo::PropertiesProxy<'_>,
) -> Result<HashMap<String, OwnedValue>> {
    match properties
        .get_all(SNI_INTERFACE.try_into().expect("valid SNI interface"))
        .await
    {
        Ok(values) => Ok(values),
        Err(get_all_error) => {
            let mut values = HashMap::new();
            for name in SNI_PROPERTIES {
                if let Ok(value) = properties
                    .get(SNI_INTERFACE.try_into().expect("valid SNI interface"), name)
                    .await
                {
                    values.insert(name.to_owned(), value);
                }
            }
            if values.is_empty() {
                return Err(crate::Error::InvalidItem(format!(
                    "Properties.GetAll failed and no individual SNI properties could be read: {get_all_error}"
                )));
            }
            Ok(values)
        }
    }
}

fn from_properties(
    service_id: &str,
    bus_name: &str,
    object_path: &str,
    owner_name: &str,
    values: &HashMap<String, OwnedValue>,
) -> TrayItem {
    TrayItem {
        id: ItemId(service_id.to_owned()),
        bus_name: bus_name.to_owned(),
        owner_name: owner_name.to_owned(),
        object_path: object_path.to_owned(),
        category: string(values, "Category"),
        item_id: string(values, "Id"),
        title: string(values, "Title"),
        status: string(values, "Status"),
        window_id: i32_value(values, "WindowId"),
        icon_theme_path: string(values, "IconThemePath"),
        icon_name: string(values, "IconName"),
        icon_pixmaps: pixmaps(values, "IconPixmap"),
        attention_icon_name: string(values, "AttentionIconName"),
        attention_icon_pixmaps: pixmaps(values, "AttentionIconPixmap"),
        attention_movie_name: string(values, "AttentionMovieName"),
        overlay_icon_name: string(values, "OverlayIconName"),
        overlay_icon_pixmaps: pixmaps(values, "OverlayIconPixmap"),
        item_is_menu: bool_value(values, "ItemIsMenu"),
        menu_path: string(values, "Menu"),
        tooltip: tooltip(values, "ToolTip"),
    }
}

fn inner<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
    match value {
        Value::Value(value) => value,
        other => other,
    }
}

fn string(values: &HashMap<String, OwnedValue>, key: &str) -> String {
    match values.get(key).map(|value| inner(value)) {
        Some(Value::Str(value)) => value.to_string(),
        Some(Value::ObjectPath(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn bool_value(values: &HashMap<String, OwnedValue>, key: &str) -> bool {
    matches!(
        values.get(key).map(|value| inner(value)),
        Some(Value::Bool(true))
    )
}

fn i32_value(values: &HashMap<String, OwnedValue>, key: &str) -> i32 {
    match values.get(key).map(|value| inner(value)) {
        Some(Value::I32(value)) => *value,
        _ => 0,
    }
}

fn bytes(value: &Value<'_>) -> Option<Vec<u8>> {
    let Value::Array(array) = value else {
        return None;
    };
    array
        .inner()
        .iter()
        .map(|byte| match byte {
            Value::U8(byte) => Some(*byte),
            _ => None,
        })
        .collect()
}

fn parse_pixmap(value: &Value<'_>) -> Option<IconPixmap> {
    let Value::Structure(fields) = value else {
        return None;
    };
    let [Value::I32(width), Value::I32(height), data] = fields.fields() else {
        return None;
    };
    IconPixmap::from_be_bytes(*width, *height, &bytes(data)?)
}

fn pixmaps(values: &HashMap<String, OwnedValue>, key: &str) -> Vec<IconPixmap> {
    let Some(Value::Array(array)) = values.get(key).map(|value| inner(value)) else {
        return Vec::new();
    };
    array.inner().iter().filter_map(parse_pixmap).collect()
}

fn tooltip(values: &HashMap<String, OwnedValue>, key: &str) -> ToolTip {
    let Some(Value::Structure(fields)) = values.get(key).map(|value| inner(value)) else {
        return ToolTip::default();
    };
    let fields = fields.fields();
    if fields.len() < 4 {
        return ToolTip::default();
    }
    let icon_pixmap = match &fields[1] {
        Value::Array(array) => array.inner().first().and_then(parse_pixmap),
        _ => None,
    };
    ToolTip {
        icon_name: match &fields[0] {
            Value::Str(value) => value.to_string(),
            _ => String::new(),
        },
        icon_pixmap,
        title: match &fields[2] {
            Value::Str(value) => value.to_string(),
            _ => String::new(),
        },
        text: match &fields[3] {
            Value::Str(value) => value.to_string(),
            _ => String::new(),
        },
    }
}
