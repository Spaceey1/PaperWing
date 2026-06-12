use std::{default, sync::Arc, time::Duration};

use gpui::{
    App, AppContext, DisplayId, ParentElement, Render, WindowBounds, WindowOptions, div, px, size,
};

use smol::lock::Mutex;
use ui::{
    bar::{init_bar, open_window},
    event::UiEvent,
};

use crate::ui::monitor_select::{self, MonitorSelect, new_monitor_select};
use crate::ui::workspaces::Workspaces;
mod compositor_ipc;
mod config;
mod consts;
mod ui;

fn main() {
    gpui_platform::application().run(|app: &mut App| {
        let (tx, display_rx) = smol::channel::unbounded::<DisplayId>();
        app.spawn(async move |mut cx| {
            let mut monitor_select_windows = Vec::<gpui::WindowHandle<MonitorSelect>>::new();
            let mut displays = cx.update(|cx| cx.displays());
            while displays.len() == 0 {
                displays = cx.update(|cx| cx.displays());
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50)) // https://github.com/zed-industries/zed/issues/46378
                    .await;
            }
            cx.update(|cx| {
                for display in displays {
                    monitor_select_windows.push(new_monitor_select(cx, display.id(), tx.clone()));
                }
            });
            let display = display_rx
                .recv()
                .await
                .expect("Failed to get selected display");
            let workspaces_indicator = Workspaces::new_entity(&mut cx, "HDMI-A-2".to_string());
            let bar = init_bar(&mut cx, workspaces_indicator.clone());
            let (tx, rx) = smol::channel::unbounded::<UiEvent>();
            // NIRI IPC START
            cx.background_executor()
                .spawn(async {
                    compositor_ipc::niri::connect_event_stream(tx).unwrap();
                })
                .detach();
            // START HANDLING EVENTS FROM NIRI IPC
            let weak_bar = bar.downgrade();
            cx.spawn(async move |cx| {
                loop {
                    let event = rx.recv().await.expect("smol channel disconnected");
                    match event {
                        UiEvent::FocusedWindowChanged(window) => {
                            let Some(bar) = &weak_bar.upgrade() else {
                                eprintln!("Can't get bar reference, it probably no longer exists");
                                break;
                            };
                            cx.update_entity(bar, |bar, cx| {
                                bar.window_title_text = window.title.clone().into();
                                cx.notify();
                            });
                        }
                        UiEvent::WorkspacesChanged(new_workspaces) => {
                            cx.update_entity(
                                &workspaces_indicator,
                                |workspaces: &mut Workspaces, cx| {
                                    workspaces.workspaces = new_workspaces.collect();
                                    cx.notify();
                                },
                            );
                        }
                        UiEvent::WorkspaceFocusChanged(new_focus) => {
                            cx.update_entity(
                                &workspaces_indicator,
                                |workspaces: &mut Workspaces, cx| {
                                    workspaces.focused_workspace = new_focus;
                                    cx.notify();
                                },
                            );
                        }
                    };
                }
            })
            .detach();
            open_window(&mut cx, bar, display);
            for window in monitor_select_windows {
                window
                    .update(cx, |_, window, cx| {
                        window.remove_window();
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    });
}
