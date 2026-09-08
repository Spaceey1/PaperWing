use std::{
    io::Write,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process,
    sync::OnceLock,
};

use iced::futures::{AsyncBufReadExt, SinkExt, StreamExt, channel::mpsc, io::BufReader};
use smol::net::unix::UnixListener;

pub static SOCKET_MADE: OnceLock<bool> = OnceLock::new();
use crate::{consts::APP_NAME, state::Message};

pub fn get_sock_path() -> PathBuf {
    let path = std::env::var("XDG_RUNTIME_DIR").unwrap();
    PathBuf::from(path).join(APP_NAME)
}

/// Opens a socket in `XDG_RUNTIME_DIR` and listens for events from [`send_message`]
pub async fn start_listener(mut output: mpsc::Sender<Message>) {
    let path = get_sock_path();
    if Path::new(&path).exists() {
        eprintln!(
            "App is already running. If that's not the case delete {}",
            path.to_str().unwrap_or("")
        );
        process::exit(1);
    }
    let Ok(listener) = UnixListener::bind(&path) else {
        eprintln!("Failed to create socket");
        return;
    };
    SOCKET_MADE.set(true).unwrap();
    loop {
        let Some(Ok(msg)) = listener.incoming().next().await else {
            eprintln!("Ipc stream broken");
            break;
        };
        let mut reader = BufReader::new(msg);
        let mut line: String = String::new();
        if reader
            .read_line(&mut line)
            .await
            .inspect_err(|e| {
                eprint!("Error reading ipc event: {e}");
            })
            .is_err()
        {
            continue;
        };
        let line = line.trim();
        match line {
            "toggle-collapse" => {
                output.send(Message::ToggleCollapse).await.unwrap();
            }
            e => {
                eprint!("Received an invalid command: {}", e);
            }
        }
    }
}

/// Sends a message to main instance of the app to be received in [`start_listener`]
pub fn send_message(mut msg: String) {
    // trim first so the default if "--" is somehow missing is the
    // trimmed message
    msg = msg.trim().to_string();
    msg = msg
        .strip_prefix(&"--".to_string())
        .unwrap_or(&msg)
        .to_string();
    let path = get_sock_path();
    let mut stream = UnixStream::connect(path).unwrap();
    stream.write_all(msg.as_bytes()).unwrap();
}
