//! `chembuilder-office`: ChemBuilder as ibisOffice's editor for embedded ChemDraw objects.
//!
//! - Started by ibisOffice (`--ibis-embed <socket> --token <token> --mode interactive|headless`):
//!   edits the object in a window, or answers one request without one.
//! - `chembuilder-office --install-ibis-handler [--quiet]`: registers this program with
//!   ibisOffice (a manifest in `%APPDATA%\ibisOffice\embed-handlers\`, or
//!   `$IBIS_SETTINGS_DIR`'s); `--quiet` prints the path instead of showing it in a dialog.
//! - `chembuilder-office [file.cdx]`: edits a CDX file, saved as it changes.
//!
//! Build: `cargo build --release --features office --bin chembuilder-office`.

// No console window: ibisOffice starts this for every picture it needs.
#![windows_subsystem = "windows"]

use chembuider_rs::office::app::{self, Event, OfficeApp, Waker};
use chembuider_rs::office::session::{self, Args, Mode};
use chembuider_rs::office::{headless, instance};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{mpsc, Arc, OnceLock};

/// The manifest ibisOffice reads to know this editor; `{command}` is this program's path.
const MANIFEST: &str = r#"id = "jp.indigocarmine.chembuilder"
command = '{command}'
prog_ids = ["ChemDraw.Document", "ChemDraw_x64.Document"]
new_object = true
import_extensions = ["cdx"]
export_extensions = ["cdx", "cdxml", "mol2"]

[name]
ja = "ChemBuilder（化学構造式）"
en = "ChemBuilder (Chemical Structure)"
"#;

fn install() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let settings = env("IBIS_SETTINGS_DIR").or_else(|| env("APPDATA").map(|d| d.join("ibisOffice"))).ok_or("no %APPDATA%")?;
    let dir = settings.join("embed-handlers");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("chembuilder.toml");
    let command = exe.to_string_lossy().replace('\'', "");
    std::fs::write(&path, MANIFEST.replace("{command}", &command)).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Shown where the user can see it (there is no console).
fn tell(message: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    unsafe { MessageBoxW(0, wide(message).as_ptr(), wide("ChemBuilder").as_ptr(), 0x40) };
}

fn window(events: mpsc::Receiver<Event>, waker: Waker, office: bool, file: Option<PathBuf>) -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 680.0]).with_title("ChemBuilder"),
        ..Default::default()
    };
    let result = eframe::run_native(
        "ChemBuilder Office",
        options,
        Box::new(move |cc| {
            let _ = waker.set(cc.egui_ctx.clone());
            Ok(Box::new(OfficeApp::new(&cc.egui_ctx, events, office, file)))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            app::log(&format!("window: {e}"));
            ExitCode::FAILURE
        }
    }
}

use chembuider_rs::egui;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--install-ibis-handler") {
        let (message, code) = match install() {
            Ok(path) => (format!("ibisOffice に登録しました / Registered with ibisOffice:\n{}", path.display()), ExitCode::SUCCESS),
            Err(e) => (e, ExitCode::FAILURE),
        };
        if argv.iter().any(|a| a == "--quiet") { println!("{message}") } else { tell(&message) }
        return code;
    }

    let Some(args) = Args::parse(argv.clone()) else {
        // Started by hand: a CDX file, or a new drawing.
        let file = argv.into_iter().find(|a| !a.starts_with("--")).map(PathBuf::from);
        let (_tx, rx) = mpsc::channel();
        return window(rx, Arc::new(OnceLock::new()), false, file);
    };

    match args.mode {
        Mode::Headless => match session::connect(&args) {
            Ok((receiver, sender)) => match headless::serve(receiver, sender) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    app::log(&format!("headless: {e}"));
                    ExitCode::FAILURE
                }
            },
            Err(e) => {
                app::log(&format!("connect: {e}"));
                ExitCode::FAILURE
            }
        },
        Mode::Interactive => {
            let handed = match instance::claim(&args) {
                instance::Claim::HandedOver => return ExitCode::SUCCESS,
                instance::Claim::First(handed) => handed,
            };
            let waker: Waker = Arc::new(OnceLock::new());
            let (events, rx) = mpsc::channel();
            app::serve_object(args, 1, events.clone(), waker.clone());
            // Objects of later launches, each on its own connection.
            {
                let (events, waker) = (events.clone(), waker.clone());
                std::thread::spawn(move || {
                    for (object, args) in (2..).zip(handed) {
                        app::serve_object(args, object, events.clone(), waker.clone());
                    }
                });
            }
            window(rx, waker, true, None)
        }
    }
}
