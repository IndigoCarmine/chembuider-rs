//! One editor window for every object: the first interactive launch listens on a socket of its
//! own, and each later launch hands its arguments over and exits (with status 0, which tells
//! ibisOffice to keep waiting for the connection the running window makes).

use super::session::{socket_name, Args};
use super::wire;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ListenerOptions, Stream};
use std::io::Write;
use std::sync::mpsc;

pub enum Claim {
    /// This is the window: arguments of later launches arrive here.
    First(mpsc::Receiver<Args>),
    /// A window was already open and has the object now.
    HandedOver,
}

/// The socket of the running window, one per user.
fn instance_name() -> String {
    let user: String = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    format!("chembuilder-office-instance-{user}")
}

fn hand_over(args: &Args) -> std::io::Result<()> {
    let mut stream = Stream::connect(socket_name(&instance_name())?)?;
    wire::write_frame(&mut stream, &serde_json::to_vec(args).map_err(std::io::Error::other)?)?;
    let answer = wire::read_frame(&mut stream)?;
    if answer == b"true" { Ok(()) } else { Err(std::io::Error::other("refused")) }
}

/// Become the window, or hand `args` to the one already open.
pub fn claim(args: &Args) -> Claim {
    for _ in 0..3 {
        if hand_over(args).is_ok() {
            return Claim::HandedOver;
        }
        let instance = instance_name();
        let Ok(name) = socket_name(&instance) else { break };
        if let Ok(listener) = ListenerOptions::new().name(name).create_sync() {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                for mut stream in listener.incoming().filter_map(Result::ok) {
                    let Ok(frame) = wire::read_frame(&mut stream) else { continue };
                    let Ok(args) = serde_json::from_slice::<Args>(&frame) else { continue };
                    // Acknowledged once the window has the object, so the launch can exit.
                    let taken = tx.send(args).is_ok();
                    let _ = wire::write_frame(&mut stream, if taken { b"true" } else { b"false" });
                    let _ = stream.flush();
                }
            });
            return Claim::First(rx);
        }
        // Another launch became the window between the two attempts: hand over to it.
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // No window to hand over to and none could be made: be a window of our own.
    Claim::First(mpsc::channel().1)
}
