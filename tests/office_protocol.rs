//! `chembuilder-office` as ibisOffice sees it: a host on a real local socket starts the binary,
//! checks the handshake, and makes the requests ibisOffice makes.
//!
//!   cargo test --features office --test office_protocol
#![cfg(all(windows, feature = "office"))]

use chembuider_rs::molecule::cdx;
use chembuider_rs::office::content;
use chembuider_rs::office::message::{EditorMsg, HostMsg};
use chembuider_rs::office::session::socket_name;
use chembuider_rs::office::wire;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ListenerNonblockingMode, ListenerOptions, Stream};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const SAMPLE: &[u8] = include_bytes!("fixtures/chemdraw26_benzene.cdx");
const TOKEN: &str = "0123456789abcdef0123456789abcdef";

struct Host {
    stream: Stream,
    child: Child,
}

impl Host {
    /// Start the editor in `mode` and take its connection, as ibisOffice does.
    fn start(mode: &str, env: &[(&str, &str)]) -> Host {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let name = format!("chembuilder-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let listener = ListenerOptions::new()
            .name(socket_name(&name).unwrap())
            .nonblocking(ListenerNonblockingMode::Accept)
            .create_sync()
            .unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_chembuilder-office"))
            .args(["--ibis-embed", &name, "--token", TOKEN, "--mode", mode])
            .envs(env.iter().copied())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let stream = loop {
            match listener.accept() {
                Ok(s) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => panic!("no connection: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        let mut host = Host { stream, child };
        let hello: serde_json::Value = serde_json::from_slice(&wire::read_frame(&mut host.stream).unwrap()).unwrap();
        assert_eq!(hello, serde_json::json!({"type": "Hello", "token": TOKEN, "protocol": 1}));
        host
    }

    fn send(&mut self, msg: &HostMsg) {
        wire::write_frame(&mut self.stream, &serde_json::to_vec(msg).unwrap()).unwrap();
    }

    fn recv(&mut self) -> EditorMsg {
        serde_json::from_slice(&wire::read_frame(&mut self.stream).unwrap()).unwrap()
    }

    fn finish(mut self) {
        let status = self.child.wait().unwrap();
        assert!(status.success(), "exits cleanly: {status}");
    }
}

#[test]
fn headless_requests_are_answered() {
    let mut host = Host::start("headless", &[]);
    host.send(&HostMsg::Render { content: content::new_content(SAMPLE.to_vec()) });
    let EditorMsg::Rendered { preview, size_emu } = host.recv() else { panic!("Rendered") };
    assert_eq!(&preview.png.unwrap()[1..4], b"PNG");
    assert!(size_emu.is_some());
    host.finish();

    let mut host = Host::start("headless", &[]);
    host.send(&HostMsg::New);
    let EditorMsg::Created { content, .. } = host.recv() else { panic!("Created") };
    assert_eq!(content.prog_id, "ChemDraw_x64.Document.6.0");
    host.finish();

    let mut host = Host::start("headless", &[]);
    host.send(&HostMsg::Import { extension: "cdx".into(), data: SAMPLE.to_vec() });
    let EditorMsg::Created { content, .. } = host.recv() else { panic!("Created") };
    assert_eq!(content.stream("CONTENTS"), Some(SAMPLE));
    host.finish();
}

#[test]
fn an_edit_goes_back_as_chemdraw_content() {
    let mut host = Host::start("interactive", &[("CHEMBUILDER_OFFICE_TEST_SCRIPT", "add-atom")]);
    let mut original = content::new_content(SAMPLE.to_vec());
    original.prog_id = "ChemDraw.Document.6.0".into();
    host.send(&HostMsg::Open { content: original, title: "sample.pptx".into(), locale: "ja".into() });
    let EditorMsg::Update { content, preview, size_emu } = host.recv() else { panic!("Update") };
    assert_eq!(content.prog_id, "ChemDraw.Document.6.0", "the object's class is kept");
    let mol = cdx::cdx_bytes_to_molecule(content.stream("CONTENTS").unwrap()).unwrap();
    assert_eq!(mol.atoms.len(), 7, "benzene and the added carbon");
    assert!(preview.png.is_some() && size_emu.is_some());
    assert_eq!(host.recv(), EditorMsg::Closed);
    host.finish();
}
