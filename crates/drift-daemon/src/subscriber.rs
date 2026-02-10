use std::collections::VecDeque;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, thread};

use drift_core::events::Event;
use drift_core::paths;

pub fn run_subscriber_manager(
    rx: mpsc::Receiver<Event>,
    shutdown: &'static AtomicBool,
    replay_count: usize,
) {
    let sock_path = paths::subscribe_socket_path();
    if let Some(parent) = sock_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::remove_file(&sock_path);

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("failed to bind subscribe socket: {e}");
            return;
        }
    };
    listener.set_nonblocking(true).expect("set nonblocking");

    let mut subscribers: Vec<UnixStream> = Vec::new();
    let mut replay_buffer: VecDeque<Event> = VecDeque::with_capacity(replay_count + 1);

    while !shutdown.load(Ordering::Relaxed) {
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    let mut alive = true;
                    for event in &replay_buffer {
                        if let Ok(json) = serde_json::to_string(event) {
                            if writeln!(&stream, "{json}").is_err() {
                                alive = false;
                                break;
                            }
                        }
                    }
                    if alive {
                        subscribers.push(stream);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        loop {
            match rx.try_recv() {
                Ok(event) => {
                    replay_buffer.push_back(event.clone());
                    if replay_buffer.len() > replay_count {
                        replay_buffer.pop_front();
                    }

                    if let Ok(json) = serde_json::to_string(&event) {
                        subscribers.retain(|stream| writeln!(&*stream, "{json}").is_ok());
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = fs::remove_file(&sock_path);
                    return;
                }
            }
        }

        thread::sleep(Duration::from_millis(50));
    }

    let _ = fs::remove_file(&sock_path);
}
