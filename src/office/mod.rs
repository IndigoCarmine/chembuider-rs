//! ChemBuilder as ibisOffice's editor for embedded ChemDraw objects (feature `office`, the
//! `chembuilder-office` binary).
//!
//! ibisOffice starts the editor with `--ibis-embed <socket> --token <token> --mode …` and talks
//! to it over a local socket (a named pipe on Windows) in length-prefixed JSON frames. An object
//! is ChemDraw's: its `CONTENTS` stream is a CDX document, so what is saved here opens in
//! Office with ChemDraw, and the other way round. Written from the protocol's published
//! description (ibisOffice's `ibis-embed-protocol/README.md`); it shares no code with it.
//!
//! - [`message`], [`wire`], [`session`]: the messages, their frames, one connection.
//! - [`instance`]: one window for every object.
//! - [`headless`]: requests without a window (new object, import, export, picture).
//! - [`app`]: the window.

pub mod app;
pub mod content;
pub mod headless;
pub mod instance;
pub mod message;
pub mod session;
pub mod wire;
