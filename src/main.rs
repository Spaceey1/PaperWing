use std::
    sync::Arc
;

use gpui::{App, AppContext};

use crate::{
    compositor_ipc::niri,
    ui::bar::{init_bar, open_window},
};

mod compositor_ipc;
mod ui;

pub enum UiEvent {
    FocusedWindowChanged(Arc<niri::Window>),
}

fn main() {
    gpui_platform::application().run(|app: &mut App| {
        let async_app = app.to_async();
        let bar = init_bar(app);
        let (tx, rx) = smol::channel::unbounded::<UiEvent>();
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
                            println!("event: {}", window.title);
                            cx.update_entity(&weak_bar.upgrade().unwrap(), |bar, cx| {
                                bar.window_title_text = window.title.clone().into();
                                cx.notify();
                            });
                        }
                    };
                }
            })
            .detach();
        open_window(app, bar);
    });
}
