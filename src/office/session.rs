//! One connection to ibisOffice: the launch arguments it gave, the socket, the handshake.

use super::message::{EditorMsg, HostMsg, PROTOCOL};
use super::wire;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericFilePath, GenericNamespaced, Name, RecvHalf, SendHalf, Stream};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// A window the user edits in.
    Interactive,
    /// One request answered without a window.
    Headless,
}

/// How ibisOffice started this process: `--ibis-embed <socket> --token <token> --mode <mode>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Args {
    pub socket: String,
    pub token: String,
    pub mode: Mode,
}

impl Args {
    /// The arguments ibisOffice passes (as `--flag value` or `--flag=value`, among any others),
    /// or `None` when it did not start this process.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Option<Args> {
        let mut socket = None;
        let mut token = None;
        let mut mode = Mode::Interactive;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
                _ => (arg, None),
            };
            let mut value = || inline.clone().or_else(|| args.next());
            match flag.as_str() {
                "--ibis-embed" => socket = value(),
                "--token" => token = value(),
                "--mode" if value().as_deref() == Some("headless") => mode = Mode::Headless,
                _ => {}
            }
        }
        Some(Args { socket: socket?, token: token.unwrap_or_default(), mode })
    }
}

/// A socket name as ibisOffice makes them: a path when it has a separator, else a name in the
/// system's namespace (a named pipe on Windows).
pub fn socket_name(name: &str) -> io::Result<Name<'_>> {
    if name.contains(['/', '\\']) { name.to_fs_name::<GenericFilePath>() } else { name.to_ns_name::<GenericNamespaced>() }
}

/// The receiving half of a connection.
pub struct Receiver(RecvHalf);

/// The sending half of a connection.
pub struct Sender(SendHalf);

impl Receiver {
    /// The next message ibisOffice sends, skipping ones this version does not know.
    /// `UnexpectedEof` when ibisOffice has gone.
    pub fn recv(&mut self) -> io::Result<HostMsg> {
        loop {
            let frame = wire::read_frame(&mut self.0)?;
            if let Some(msg) = HostMsg::from_json(&frame).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))? {
                return Ok(msg);
            }
        }
    }
}

impl Sender {
    pub fn send(&mut self, msg: &EditorMsg) -> io::Result<()> {
        let json = serde_json::to_vec(msg).map_err(io::Error::other)?;
        wire::write_frame(&mut self.0, &json)
    }
}

/// Connect to the socket in `args` and say hello (the token proves ibisOffice started us).
pub fn connect(args: &Args) -> io::Result<(Receiver, Sender)> {
    let stream = Stream::connect(socket_name(&args.socket)?)?;
    let (recv, send) = stream.split();
    let mut sender = Sender(send);
    sender.send(&EditorMsg::Hello { token: args.token.clone(), protocol: PROTOCOL })?;
    Ok((Receiver(recv), sender))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Option<Args> {
        Args::parse(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn reads_the_launch_arguments_in_both_spellings() {
        let a = args(&["--ibis-embed", "ibisoffice-1-2", "--token", "abc", "--mode", "headless"]).unwrap();
        assert_eq!(a, Args { socket: "ibisoffice-1-2".into(), token: "abc".into(), mode: Mode::Headless });
        let b = args(&["--extra", "--ibis-embed=C:\\tmp\\x.sock", "--token=t"]).unwrap();
        assert_eq!(b.socket, "C:\\tmp\\x.sock");
        assert_eq!(b.mode, Mode::Interactive, "interactive unless told otherwise");
        assert!(args(&["file.cdx"]).is_none(), "started by the user");
    }
}
