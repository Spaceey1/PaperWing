use std::{
    io::Write,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

use iced::futures::{AsyncBufReadExt, SinkExt, StreamExt, channel::mpsc, io::BufReader};
use smol::net::unix::UnixListener;

use crate::{app::Message, consts::APP_NAME};

fn get_sock_path() -> PathBuf {
    let path = std::env::var("XDG_RUNTIME_DIR").unwrap();
    PathBuf::from(path).join(APP_NAME)
}

pub async fn start_listener(mut output: mpsc::Sender<Message>) {
    let path = get_sock_path();
    if Path::new(&path).exists() {
        panic!(
            "App is already running. If that's not the case delete {}",
            path.to_str().or(Some("")).unwrap()
        );
    }

    println!("{}", path.to_str().or(Some("")).unwrap());
            
    let listener = UnixListener::bind(path).unwrap();
    loop {
        let Some(Ok(msg)) = listener.incoming().next().await else {
            panic!("Ipc stream broken")
        };
        let mut reader = BufReader::new(msg);
        let mut line: String = String::new();
        if reader.read_line(&mut line).await.is_err() {
            eprint!("Error reading ipc event");
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

pub fn send_message(mut msg: String) {
    // trim first so the default if "--" is somehow missing is the
    // trimmed message
    msg = msg.trim().to_string(); 
    msg = msg.strip_prefix(&"--".to_string()).unwrap_or(&msg).to_string();
    let path = get_sock_path();
    let mut stream = UnixStream::connect(path).unwrap();
    stream.write_all(&msg.as_bytes()).unwrap();
}
