use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use niri_ipc::socket::Socket;
use niri_ipc::{Request, Response};

use crate::daemon::DaemonMsg;

pub fn run_event_stream(tx: Sender<DaemonMsg>, shutdown: &'static AtomicBool) {
    while !shutdown.load(Ordering::Relaxed) {
        match connect_and_stream(&tx, shutdown) {
            Ok(()) => break,
            Err(e) => {
                eprintln!("event stream error: {e}, reconnecting in 5s");
                for _ in 0..50 {
                    if shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
}

fn connect_and_stream(
    tx: &Sender<DaemonMsg>,
    shutdown: &'static AtomicBool,
) -> anyhow::Result<()> {
    let mut socket = Socket::connect()?;

    let reply = socket.send(Request::EventStream)?;
    match reply {
        Ok(Response::Handled) => {}
        Ok(other) => anyhow::bail!("unexpected response: {other:?}"),
        Err(msg) => anyhow::bail!("niri error: {msg}"),
    }

    let mut read_event = socket.read_events();

    loop {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(());
        }

        match read_event() {
            Ok(event) => {
                if tx.send(DaemonMsg::NiriEvent(event)).is_err() {
                    return Ok(());
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}
