use std::{
    os::fd::BorrowedFd,
    path::PathBuf,
    sync::OnceLock,
};

use iced::futures::{SinkExt, channel::mpsc::Sender};
use rustsni::{TrayHost, TrayItem};
use smol::lock::RwLock;

use crate::state::Message;

pub static HOST: OnceLock<RwLock<TrayHost>> = OnceLock::new();

pub fn get_tray_host() -> rustsni::Result<&'static RwLock<TrayHost>> {
    if let Some(lock) = HOST.get() {
        return Ok(lock);
    }
    let host = TrayHost::new()?;
    let lock = RwLock::new(host);
    let _ = HOST.set(lock);
    Ok(HOST.get().unwrap())
}

pub async fn tray_listiner(mut output: Sender<Message>) -> () {
    match get_tray_host() {
        Ok(host) => {
            let borrowed_fd = unsafe { BorrowedFd::borrow_raw(host.read().await.fd()) };
            let fd = smol::Async::new(borrowed_fd).unwrap();
            loop {
                fd.readable().await.unwrap();
                let events = {
                    let mut host = host.write().await;
                    smol::unblock(move || host.poll()).await.unwrap()
                };
                for event in events {
                    output.send(Message::TrayMessage(event)).await.unwrap();
                }
            }
        }
        Err(e) => match e {
            rustsni::Error::WatcherAlreadyRunning => eprintln!(
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
