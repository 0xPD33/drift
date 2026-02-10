use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use drift_core::events::{self, Event};
use drift_core::paths;

use crate::daemon::DaemonMsg;

pub fn run_emit_listener(tx: Sender<DaemonMsg>, shutdown: &'static AtomicBool) {
    let sock_path = paths::emit_socket_path();

    if let Some(parent) = sock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&sock_path);

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("failed to bind emit socket at {}: {e}", sock_path.display());
            return;
        }
    };

    listener.set_nonblocking(true).ok();

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    match line {
                        Ok(line) if line.trim().is_empty() => continue,
                        Ok(line) => match serde_json::from_str::<Event>(&line) {
                            Ok(mut event) => {
                                if event.ts.is_empty() {
                                    event.ts = events::iso_now();
                                }
                                if event.level.is_none() {
                                    event.level = Some("info".into());
                                }
                                if tx.send(DaemonMsg::EmitEvent(event)).is_err() {
                                    let _ = std::fs::remove_file(&sock_path);
                                    return;
                                }
                            }
                            Err(e) => eprintln!("invalid event JSON: {e}"),
                        },
                        Err(_) => break,
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                eprintln!("emit accept error: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    let _ = std::fs::remove_file(&sock_path);
}
