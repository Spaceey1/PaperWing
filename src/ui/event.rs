use std::sync::Arc;

use gpui::{AsyncApp, WeakEntity};

use crate::ui::bar::Bar;
use crate::ui::state::*;
use crate::ui::workspaces::Workspaces;
use gpui::{App, AppContext};
pub enum UiEvent {
    FocusedWindowChanged(Arc<Window>),
    WorkspacesChanged(Box<dyn Iterator<Item = Arc<Workspace>> + Send>),
    WorkspaceFocusChanged(u64),
}

macro_rules! upgrade_entity_safe {
    ($entity:expr) => {
        match $entity.upgrade() {
            Some(v) => v,
            None => {
                eprintln!("Can't get entity reference, it probably no longer exists");
                continue;
            }
        }
    };
}
pub async fn handle_compositor_events(
    receiving_channel: smol::channel::Receiver<UiEvent>,
    cx: &mut AsyncApp,
    bar_entity: WeakEntity<Bar>,
    workspaces_entity: WeakEntity<Workspaces>,
) {
    loop {
        let event = receiving_channel
            .recv()
            .await
            .expect("smol channel disconnected");
        match event {
            UiEvent::FocusedWindowChanged(window) => {
                let bar = upgrade_entity_safe!(bar_entity);
                cx.update_entity(&bar, |bar, cx| {
                    bar.window_title_text = window.title.clone().into();
                    cx.notify();
                });
            }
            UiEvent::WorkspacesChanged(new_workspaces) => {
                let workspaces = upgrade_entity_safe!(workspaces_entity);
                cx.update_entity(&workspaces, |workspaces, cx| {
                    workspaces.workspaces = new_workspaces.collect();
                    cx.notify();
                });
            }
            UiEvent::WorkspaceFocusChanged(new_focus) => {
                let workspaces = upgrade_entity_safe!(workspaces_entity);
                cx.update_entity(&workspaces, |workspaces, cx| {
                    workspaces.focused_workspace = new_focus;
                    cx.notify();
                });
            }
        };
    }
}
