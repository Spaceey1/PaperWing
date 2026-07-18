use std::{os::fd::BorrowedFd, sync::OnceLock};

use iced::futures::{SinkExt, channel::mpsc::Sender};
use rustsni::TrayHost;
use smol::lock::RwLock;

use crate::app::Message;

static HOST: OnceLock<RwLock<TrayHost>> = OnceLock::new();

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
