use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::{Error, Result};

/// A value in a DBusMenu property dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropValue {
    Str(String),
    Bool(bool),
    Int(i32),
    Bytes(Vec<u8>),
}

/// A node in a DBusMenu layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuNode {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub icon_name: String,
    pub icon_data: Vec<u8>,
    pub toggle_type: String,
    pub toggle_state: i32,
    pub is_submenu: bool,
    pub children: Vec<MenuNode>,
}

pub(crate) async fn layout(
    connection: &zbus::Connection,
    bus_name: &str,
    menu_path: &str,
    parent_id: i32,
) -> Result<Vec<MenuNode>> {
    let proxy = zbus::Proxy::new(connection, bus_name, menu_path, "com.canonical.dbusmenu").await?;
    let reply = match proxy
        .call_method("GetLayout", &(parent_id, -1i32, Vec::<&str>::new()))
        .await
    {
        Ok(reply) => reply,
        Err(zbus::Error::MethodError(_, _, _)) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let (_, root): (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) =
        match reply.body().deserialize() {
            Ok(reply) => reply,
            Err(_) => return Ok(Vec::new()),
        };
    Ok(root.2.iter().filter_map(parse_owned_node).collect())
}

fn parse_owned_node(value: &OwnedValue) -> Option<MenuNode> {
    parse_node(value)
}

fn parse_node(value: &Value<'_>) -> Option<MenuNode> {
    let Value::Structure(fields) = value else {
        return match value {
            Value::Value(value) => parse_node(value),
            _ => None,
        };
    };
    let [
        Value::I32(id),
        Value::Dict(properties),
        Value::Array(children),
    ] = fields.fields()
    else {
        return None;
    };

    let mut props = HashMap::new();
    for (key, value) in properties.iter() {
        let Value::Str(key) = key else {
            continue;
        };
        if let Some(value) = property(value) {
            props.insert(key.as_str().to_owned(), value);
        }
    }

    Some(MenuNode {
        id: *id,
        label: strip_gtk_mnemonics(&string(&props, "label")),
        enabled: bool_prop(&props, "enabled").unwrap_or(true),
        visible: bool_prop(&props, "visible").unwrap_or(true),
        icon_name: string(&props, "icon-name"),
        icon_data: bytes_prop(&props, "icon-data"),
        toggle_type: string(&props, "toggle-type"),
        toggle_state: match props.get("toggle-state") {
            Some(PropValue::Int(value)) => *value,
            _ => -1,
        },
        is_submenu: props.contains_key("children-display") || !children.is_empty(),
        children: children.iter().filter_map(parse_node).collect(),
    })
}

fn property(value: &Value<'_>) -> Option<PropValue> {
    match value {
        Value::Value(value) => property(value),
        Value::Str(value) => Some(PropValue::Str(value.to_string())),
        Value::Bool(value) => Some(PropValue::Bool(*value)),
        Value::I32(value) => Some(PropValue::Int(*value)),
        Value::Array(array) => Some(PropValue::Bytes(
            array
                .inner()
                .iter()
                .map(|byte| match byte {
                    Value::U8(byte) => Some(*byte),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        _ => None,
    }
}

fn string(props: &HashMap<String, PropValue>, key: &str) -> String {
    match props.get(key) {
        Some(PropValue::Str(value)) => value.clone(),
        _ => String::new(),
    }
}

/// GTK labels mark mnemonics with a single underscore. A doubled
/// underscore represents a literal underscore and must be preserved.
fn strip_gtk_mnemonics(label: &str) -> String {
    let mut chars = label.chars().peekable();
    let mut stripped = String::with_capacity(label.len());

    while let Some(ch) = chars.next() {
        if ch == '_' && chars.peek() == Some(&'_') {
            chars.next();
            stripped.push('_');
        } else if ch != '_' {
            stripped.push(ch);
        }
    }

    stripped
}

fn bool_prop(props: &HashMap<String, PropValue>, key: &str) -> Option<bool> {
    match props.get(key) {
        Some(PropValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn bytes_prop(props: &HashMap<String, PropValue>, key: &str) -> Vec<u8> {
    match props.get(key) {
        Some(PropValue::Bytes(value)) => value.clone(),
        _ => Vec::new(),
    }
}

pub(crate) async fn event(
    connection: &zbus::Connection,
    bus_name: &str,
    menu_path: &str,
    menu_id: i32,
    event_id: &str,
) -> Result<()> {
    let proxy = zbus::Proxy::new(connection, bus_name, menu_path, "com.canonical.dbusmenu").await?;
    let data = zbus::zvariant::Value::new(0i32);
    let body = (menu_id, event_id, data, 0u32);
    if let Err(error) = proxy.call::<_, _, ()>("Event", &body).await {
        return match error {
            zbus::Error::MethodError(_, _, _) => Ok(()),
            other => Err(Error::Dbus(other)),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use zbus::zvariant::{Structure, Value};
    use crate::menu::*;

    fn node() -> Value<'static> {
        let mut props = HashMap::new();
        props.insert("label".to_owned(), Value::from("Open"));
        props.insert("visible".to_owned(), Value::from(false));
        props.insert("children-display".to_owned(), Value::from("submenu"));

        Value::from(Structure::from((7i32, props, Vec::<Value<'static>>::new())))
    }

    #[test]
    fn parses_layout_node() {
        let node = parse_node(&node()).unwrap();
        assert_eq!(node.id, 7);
        assert_eq!(node.label, "Open");
        assert!(node.enabled);
        assert!(!node.visible);
        assert!(node.is_submenu);
        assert_eq!(node.toggle_state, -1);
    }

    #[test]
    fn strips_mnemonic_markers() {
        assert_eq!(strip_gtk_mnemonics("_Open"), "Open");
        assert_eq!(strip_gtk_mnemonics("Save _As"), "Save As");
        assert_eq!(strip_gtk_mnemonics("Save __ As"), "Save _ As");
        assert_eq!(strip_gtk_mnemonics("No underscores"), "No underscores");
    }
}
