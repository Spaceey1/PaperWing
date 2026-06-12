use std::{ops::Deref, rc::Rc};

use gpui::{App, AppContext, DisplayId, PlatformDisplay};

use ui::{
    bar::{init_bar, open_window},
    event::UiEvent,
};

use crate::ui::workspaces::Workspaces;
use crate::ui::{
    event::handle_compositor_events,
    monitor_select::{MonitorSelect, new_monitor_select},
};
mod compositor_ipc;
mod config;
mod consts;
mod ui;

fn main() {
    gpui_platform::application().run(|app: &mut App| {
        let (tx, display_rx) = smol::channel::unbounded::<Rc<dyn PlatformDisplay>>();
        app.spawn(async move |mut cx| {
            let mut monitor_select_windows = Vec::<gpui::WindowHandle<MonitorSelect>>::new();
            // https://github.com/zed-industries/zed/issues/46378
            let mut displays = cx.update(|cx| cx.displays());
            while displays.len() == 0 {
                displays = cx.update(|cx| cx.displays());
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
            let config = config::CONFIG.read().unwrap();
            let display = if let Some(display) = config
                .display_id
                .and_then(|display_id| displays.iter().find(|d| d.uuid().unwrap() == display_id))
            {
                display.clone()
            } else {
                drop(config); // Choosing the displays can take a while, so drop the config lock early
                cx.update(|cx| {
                    let displays_iter = displays.iter();
                    for display in displays_iter {
                        monitor_select_windows.push(new_monitor_select(
                            cx,
                            display.clone(),
                            tx.clone(),
                        ));
                    }
                });
                let display = display_rx
                    .recv()
                    .await
                    .expect("Failed to get selected display");
                {
                    let mut config = config::CONFIG.write().unwrap();
                    config.display_id = Some(display.uuid().unwrap());
                }
                if let Some(e) = config::save_config().err() {
                    eprintln!("{}", e);
                };
                display
            };
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
                handle_compositor_events(rx, cx, weak_bar, workspaces_indicator.downgrade()).await;
            })
            .detach();
            // OPEN THE MAIN WINDOW
            open_window(&mut cx, bar, display.id());

            // CLOSE PICKER WINDOWS IF THEY EXIST
            for window in monitor_select_windows {
                // This has to be after opening main window
                // since otherwise gpui just quits :)
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
