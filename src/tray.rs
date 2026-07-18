use std::{os::fd::BorrowedFd, sync::OnceLock};

use iced::futures::{SinkExt, channel::mpsc::Sender};
use rustsni::TrayHost;
use smol::lock::RwLock;

use crate::app::Message;

static HOST: OnceLock<RwLock<TrayHost>> = OnceLock::new();

pub fn get_tray_host() -> &'static RwLock<TrayHost> {
    HOST.get_or_init(|| RwLock::new(TrayHost::new().unwrap()))
}
// static ITEMS: OnceLock<&HashMap<ItemId, TrayItem>> = OnceLock::new();
// static HOST: RwLock<TrayHost> = RwLock::new(TrayHost::new().unwrap());

pub async fn tray_listiner(mut output: Sender<Message>) -> () {
    let host = get_tray_host();
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
