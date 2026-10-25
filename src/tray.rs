use std::path::PathBuf;

use iced::futures::{SinkExt, StreamExt, channel::mpsc::Sender};
use smol::lock::{OnceCell, RwLock};
use tray_server::{TrayHost, TrayItem};

use crate::state::Message;

pub static HOST: OnceCell<RwLock<TrayHost>> = OnceCell::new();

pub async fn get_tray_host() -> tray_server::Result<&'static RwLock<TrayHost>> {
    HOST.get_or_try_init(|| async { TrayHost::new().await.map(RwLock::new) })
        .await
}

pub async fn tray_listener(mut output: Sender<Message>) {
    match get_tray_host().await {
        Ok(host) => {
            let mut events = host.read().await.subscribe();
            while let Some(event) = events.next().await {
                if output.send(Message::TrayMessage(event)).await.is_err() {
                    break;
                }
            }
        }
        Err(e) => match e {
            tray_server::Error::WatcherAlreadyRunning => eprintln!(
                "There is already a taskbar app running. Turn off your taskbar like waybar etc. for the tray functionality to work."
            ),
            e => eprintln!("Tray crashed: {e}"),
        },
    }
}

pub fn get_tray_icon(item: &TrayItem) -> Option<PathBuf> {
    for path in item.icon_search_paths() {
        let Ok(files) = std::fs::read_dir(path) else {
            continue;
        };
        for file in files {
            let Ok(file) = file else {
                continue;
            };
            let file_path = file.path();
            let Some(file_stem) = file_path.file_stem().and_then(|f| f.to_str()) else {
                continue;
            };
            let file_stem = file_stem.to_string();
            if file_stem == item.icon_name {
                return Some(file_path);
            }
        }
    }
    None
}
