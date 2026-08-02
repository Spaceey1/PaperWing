// WIP!

use std::{
    collections::HashMap,
    env::{self},
    error::Error,
    sync::{Arc, LazyLock},
};

use smol::net::unix::UnixStream;
use smol::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    lock::RwLock,
};

use crate::state::Window;
use crate::{Compositor, CompositorError, WorkspaceId, compositor_event, state::Workspace};

static MANGO_SOCK_PATH: &str = "MANGO_INSTANCE_SIGNATURE";

static WORKSPACES: LazyLock<RwLock<HashMap<WorkspaceId, Arc<Workspace>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub struct Mango;

#[async_trait::async_trait]
impl Compositor for Mango {
    fn is_running(&self) -> bool {
        !std::env::var(MANGO_SOCK_PATH).is_err()
    }
    async fn start_event_stream(
        &self,
        callback: smol::channel::Sender<compositor_event::CompositorEvent>,
    ) -> Result<(), Box<dyn Error>> {
        let callback2 = callback.clone();
        smol::spawn(async move { start_window_stream(callback2).await.unwrap() }).detach();
        start_workspace_stream(callback).await.unwrap();
        Ok(())
    }
    async fn get_workspaces(&self) -> Result<Vec<Arc<Workspace>>, CompositorError> {
        let r = send_message(b"get all-tags\n").await;
        get_workspaces_from_value(r)
            .await
            .ok_or_else(|| CompositorError::ConnectionError)
    }
    async fn go_to_workspace(&self, id: &WorkspaceId) -> Result<(), CompositorError> {
        let workspaces = WORKSPACES.read().await;
        let Some(workspace) = workspaces.get(id) else {
            return Err(CompositorError::Unknown);
        };
        send_message(
            &format!(
                "dispatch viewcrossmon,{},{}\n",
                workspace.idx, workspace.output
            )
            .into_bytes(),
        )
        .await;
        Ok(())
    }
}
async fn send_message(message: &[u8]) -> serde_json::Value {
    let sock_path = env::var(MANGO_SOCK_PATH).unwrap();
    let mut socket = smol::net::unix::UnixStream::connect(sock_path)
        .await
        .unwrap();
    let socket2 = socket.clone();
    let mut reader = smol::io::BufReader::new(socket2);
    socket.write_all(message).await.unwrap();
    socket.flush().await.unwrap();
    let mut buf = String::new();
    reader.read_line(&mut buf).await.unwrap();
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&buf) {
        return data;
    } else {
        serde_json::Value::Null
    }
}

async fn get_stream(message: &[u8]) -> Result<BufReader<UnixStream>, Box<dyn Error>> {
    let sock_path = env::var(MANGO_SOCK_PATH)?;
    let mut socket = smol::net::unix::UnixStream::connect(sock_path).await?;
    let socket2 = socket.clone();
    let reader = smol::io::BufReader::new(socket2);
    socket.write_all(message).await?;
    socket.flush().await?;
    return Ok(reader);
}

async fn start_window_stream(
    callback: smol::channel::Sender<compositor_event::CompositorEvent>,
) -> Result<(), Box<dyn Error>> {
    let mut reader = get_stream(b"watch all-clients\n").await?;
    let mut buf = String::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf).await.is_err() {
            continue;
        };
        let Some(data) = (|| -> Option<Vec<Window>> {
            Some(
                serde_json::from_str::<serde_json::Value>(&buf)
                    .ok()?
                    .get("clients")?
                    .as_array()?
                    .into_iter()
                    .flat_map(|v| -> Option<Window> {
                        // mango doesn't mark windows as not focused when they're not visible.
                        let is_visible = v.get("is_visible")?.as_bool()?;
                        let mut w = serde_json::from_value::<Window>(v.clone()).ok()?;
                        w.is_focused = w.is_focused && is_visible;
                        return Some(w);
                    })
                    .collect(),
            )
        })() else {
            continue;
        };
        if let Some(new_focused) = data.into_iter().find(|w| w.is_focused) {
            let _ = callback
                .send(crate::CompositorEvent::FocusedWindowChanged(Arc::new(
                    new_focused,
                )))
                .await;
        };
    }
}

async fn start_workspace_stream(
    callback: smol::channel::Sender<compositor_event::CompositorEvent>,
) -> Result<(), Box<dyn Error>> {
    let mut reader = get_stream(b"watch all-tags\n").await?;
    let mut buf = String::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf).await.is_err() {
            continue;
        };
        let Some(data) = get_workspaces_from_value(serde_json::from_str(&buf)?).await else {
            continue;
        };

        callback
            .send(crate::CompositorEvent::WorkspacesChanged(data))
            .await
            .unwrap();
        callback
            .send(crate::CompositorEvent::WorkspaceFocusChanged())
            .await
            .unwrap();
    }
}

fn get_workspace_id(tag: usize, monitor_index: &usize) -> WorkspaceId {
    // mango doesnt provide unique ids, but index is unique per monitor so combining the 2 *should* be unique
    let half_bits = (usize::BITS / 2) as usize;
    (tag << half_bits) | monitor_index
}

async fn get_workspaces_from_value(data: serde_json::Value) -> Option<Vec<Arc<Workspace>>> {
    let mut workspaces = WORKSPACES.write().await;
    Some(
        data.get("all_tags")?
            .as_array()?
            .into_iter()
            .enumerate()
            .flat_map(|(i, m)| -> Option<_> {
                let monitor = m.get("monitor")?.as_str()?;
                Some(
                    m.get("tags")?
                        .as_array()?
                        .into_iter()
                        .filter_map(move |tag| {
                            let index = tag.get("index")?.as_u64()? as usize;
                            let id = get_workspace_id(index, &i);
                            let any_windows_present = tag.get("client_count")?.as_u64()? > 0;
                            let new = Some(Arc::new(Workspace {
                                idx: index,
                                id,
                                is_active: tag.get("is_active")?.as_bool()?,
                                is_focused: tag.get("is_active")?.as_bool()?,
                                // mango doesn't provide which window is active
                                active_window_id: if any_windows_present { Some(0) } else { None },
                                output: monitor.to_string(),
                                ..Default::default()
                            }));
                            new
                        }),
                )
            })
            .flatten()
            .map(|w| {
                workspaces.insert(w.id, w.clone());
                w
            })
            .collect(),
    )
}
