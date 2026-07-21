use std::env::args;
mod app;
mod config;
mod consts;
mod helper;
mod ipc;
mod state;
mod theme;
mod tray;
mod subscriptions;
mod elements;

fn main() {
    match args().nth(1) {
        None => app::start_app(),
        Some(msg) => ipc::send_message(msg),
    }
}
