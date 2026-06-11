use std::{default, time::Duration};

use gpui::{
    App, AppContext, DisplayId, ParentElement, Render, WindowBounds, WindowOptions, div, px, size,
};

use ui::{
    bar::{init_bar, open_window},
    event::UiEvent,
};

use crate::ui::monitor_select::new_monitor_select;
use crate::ui::workspaces::Workspaces;
mod compositor_ipc;
mod config;
mod consts;
mod ui;

struct Dummy {}
impl Render for Dummy {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        div().child("hello world")
    }
}

fn main() {
    gpui_platform::application().run(|app: &mut App| {
        let mut async_app = app.to_async();
        let (tx, display_rx) = smol::channel::unbounded::<DisplayId>();
        app.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500)) // https://github.com/zed-industries/zed/issues/46378
                .await;
            let displays = cx.update(|cx| cx.displays());
            println!("{:?}", displays);
            cx.update(move |cx| {
                for display in displays {
                    new_monitor_select(cx, display.id(), tx.clone());
                    println!("{}", display.uuid().ok().expect("shit").to_string());
                }
            });
        })
        .detach();
        app.to_async().spawn(async move |cx| {
            let workspaces_indicator =
                Workspaces::new_entity(&mut async_app, "HDMI-A-1".to_string());
            let bar = init_bar(&mut async_app, workspaces_indicator.clone());
            let (tx, rx) = smol::channel::unbounded::<UiEvent>();
            let display = display_rx.recv().await.expect("Failed to get selected display");
            // let display = display.expect("a");
            // NIRI IPC START
            async_app
                .background_executor()
                .spawn(async {
                    compositor_ipc::niri::connect_event_stream(tx).unwrap();
                })
                .detach();
            // START HANDLING EVENTS FROM NIRI IPC
            let weak_bar = bar.downgrade();
            async_app
                .spawn(async move |cx| {
                    loop {
                        let event = rx.recv().await.expect("smol channel disconnected");
                        match event {
                            UiEvent::FocusedWindowChanged(window) => {
                                let Some(bar) = &weak_bar.upgrade() else {
                                    eprintln!(
                                        "Can't get bar reference, it probably no longer exists"
                                    );
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
                        };
                    }
                })
                .detach();
            open_window(&mut async_app, bar, display);
        }).detach();
    });
}
