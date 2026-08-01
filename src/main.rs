use std::{env::args, fmt::Display};
mod app;
mod config;
mod consts;
mod elements;
mod helper;
mod ipc;
mod state;
mod subscriptions;
mod theme;
mod tray;

fn set_env_if_not_present<T>(name: &str, value: T)
where
    T: Display,
{
    if std::env::var(name).is_err_and(|e| e == std::env::VarError::NotPresent) {
        unsafe { std::env::set_var(name, format!("{value}")) };
    } else {
        eprintln!("{name} is set, not overriding.");
    };
}

fn main() {
    match args().nth(1) {
        None => {
            // smol needs at least 2 threads for iced_layershell to not deadlock :/
            set_env_if_not_present("SMOL_THREADS", 8);
            app::start_app();
        }
        Some(msg) => ipc::send_message(msg),
    }
}
