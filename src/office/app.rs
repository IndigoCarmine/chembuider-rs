//! The editor window: one tab per object ibisOffice opened (or a CDX file opened by hand), each
//! edit sent back as the object's new content and picture a moment after the user lets go.

use super::content;
use super::message::{EditorMsg, HostMsg, OleContent};
use super::session::{self, Args, Sender};
use crate::molecule::cdx;
use crate::widget::SCALE_FACTOR;
use crate::{BondStereo, ChemStructEditor, Config, Molecule, Tool};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};

/// How long after the last change (with no button held) the object is sent back: one undo
/// step in the document per gesture, not per frame.
const SETTLE: Duration = Duration::from_millis(400);

/// Where a drawing started without a file is saved.
const UNTITLED: &str = "untitled.cdx";

/// What the connection threads tell the window.
pub enum Event {
    Opened { object: u64, content: OleContent, title: String, locale: String, sender: Sender },
    /// ibisOffice asked for the object again (a double-click on one already open).
    Focus(u64),
    /// ibisOffice closed the object, or went away.
    Gone(u64),
    Failed(String),
}

/// Where an open drawing came from and goes back to.
enum Source {
    Office { sender: Sender, original: OleContent },
    /// `original` is the file's CDX as it was opened (`None` for a new file): each save merges
    /// the drawing into it, as for ibisOffice, so what the editor cannot show stays in the file.
    /// `on_disk` is what the file held when it was last read or written here (`None`: not
    /// there), to tell whether another program changed it since.
    File { path: PathBuf, original: Option<Vec<u8>>, on_disk: Option<Vec<u8>> },
}

struct Tab {
    object: u64,
    title: String,
    source: Source,
    editor: ChemStructEditor,
    /// The drawing as ibisOffice (or the file) last got it.
    sent: Molecule,
    /// When the drawing last changed since then.
    changed: Option<Instant>,
    /// Not a drawing the editor can read: shown, never sent back.
    unreadable: Option<String>,
    note: Option<String>,
    /// The file was changed by another program since it was opened or last saved here: it is
    /// no longer written, so that change is not lost.
    changed_outside: bool,
    /// The window pans to the drawing on its first frame.
    fit: bool,
}

impl Tab {
    fn new(object: u64, title: String, source: Source, cdx_bytes: Option<&[u8]>, ja: bool) -> Tab {
        let mut editor = ChemStructEditor::default();
        editor.config = Config::load();
        let (mut unreadable, mut note) = (None, None);
        match cdx_bytes.map(cdx::read_cdx) {
            Some(Some(import)) => {
                editor.molecule = import.molecule;
                if import.other_objects > 0 {
                    note = Some(if ja {
                        format!("構造式以外の {} 個のオブジェクト（矢印・文字など）はそのまま保持されます", import.other_objects)
                    } else {
                        format!("{} other objects (arrows, text, …) are kept as they are", import.other_objects)
                    });
                }
            }
            Some(None) => unreadable = Some(if ja { "ChemDraw (CDX) の図として読めません".to_string() } else { "Not a ChemDraw (CDX) drawing".to_string() }),
            None => {}
        }
        let sent = editor.molecule.clone();
        Tab { object, title, source, editor, sent, changed: None, unreadable, note, changed_outside: false, fit: true }
    }

    /// A CDX file opened by hand.
    fn open_file(path: PathBuf, ja: bool) -> Tab {
        let title = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read(&path) {
            Ok(bytes) => Tab::new(0, title, Source::File { path, original: Some(bytes.clone()), on_disk: Some(bytes.clone()) }, Some(&bytes), ja),
            // Nothing there yet: a new drawing, written on the first edit.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Tab::new(0, title, Source::File { path, original: None, on_disk: None }, None, ja),
            // There but not readable (locked, no access, …): shown as such and never written
            // over, or the first edit would replace the file with a drawing that is not it.
            Err(e) => {
                let mut tab = Tab::new(0, title, Source::File { path, original: None, on_disk: None }, None, ja);
                tab.unreadable = Some(if ja { format!("ファイルを読めません: {e}") } else { format!("Cannot read the file: {e}") });
                tab
            }
        }
    }

    /// The drawing started without a file: `untitled.cdx` in `dir`. One left by an earlier
    /// session is opened, so its drawing is shown and edited rather than silently replaced.
    fn untitled(dir: &Path, ja: bool) -> Tab {
        Tab::open_file(dir.join(UNTITLED), ja)
    }

    fn original_cdx(&self) -> Option<&[u8]> {
        match &self.source {
            Source::Office { original, .. } => original.stream(content::CONTENTS),
            Source::File { original, .. } => original.as_deref(),
        }
    }

    /// What becomes of an edit, for the status bar.
    fn edits_status(&self, ja: bool) -> &'static str {
        edits_status(matches!(self.source, Source::Office { .. }), self.unreadable.is_some() || self.changed_outside, ja)
    }

    /// The drawing as it is now, sent back (to ibisOffice, or written to its file). A file
    /// another program changed since it was read or written here is left as it is, and no
    /// longer written: its content is compared, as a modification time may not change.
    fn flush(&mut self) -> Result<(), String> {
        self.changed = None;
        if self.unreadable.is_some() || self.changed_outside || self.editor.molecule == self.sent {
            return Ok(());
        }
        let mol = self.editor.molecule.clone();
        let cdx_bytes = match self.original_cdx() {
            Some(original) => cdx::merge_into_cdx(original, &mol),
            None => cdx::molecule_to_cdx_document(&mol),
        };
        match &mut self.source {
            Source::Office { sender, original } => {
                let (preview, size) = content::preview(&mol);
                let content = content::with_cdx(original, cdx_bytes);
                sender.send(&EditorMsg::Update { content, preview, size_emu: Some(size) }).map_err(|e| e.to_string())?;
            }
            Source::File { path, on_disk, .. } => {
                let now = match std::fs::read(&path) {
                    Ok(bytes) => Some(bytes),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.to_string()),
                };
                if now != *on_disk {
                    self.changed_outside = true;
                    return Ok(());
                }
                std::fs::write(&path, &cdx_bytes).map_err(|e| e.to_string())?;
                *on_disk = Some(cdx_bytes);
            }
        }
        self.sent = mol;
        Ok(())
    }

    /// Done with the object: what is left sent, and ibisOffice told.
    fn close(mut self) {
        if let Err(e) = self.flush() {
            log(&format!("{}: {e}", self.title));
        }
        if let Source::Office { sender, .. } = &mut self.source {
            let _ = sender.send(&EditorMsg::Closed);
        }
    }
}

pub struct OfficeApp {
    tabs: Vec<Tab>,
    active: usize,
    events: mpsc::Receiver<Event>,
    /// Japanese UI (the locale ibisOffice asked for).
    ja: bool,
    status: String,
    /// The window closes when its last object does (not when it was opened by hand).
    office: bool,
    /// A scripted edit for the interop tests (`CHEMBUILDER_OFFICE_TEST_SCRIPT=add-atom`).
    script: Option<String>,
}

/// The egui context, once the window has one: connection threads wake the window with it.
pub type Waker = Arc<OnceLock<egui::Context>>;

fn wake(waker: &Waker) {
    if let Some(ctx) = waker.get() {
        ctx.request_repaint();
    }
}

/// Serve one object ibisOffice asked us to open, on a thread of its own.
pub fn serve_object(args: Args, object: u64, events: mpsc::Sender<Event>, waker: Waker) {
    std::thread::spawn(move || {
        let (mut receiver, sender) = match session::connect(&args) {
            Ok(c) => c,
            Err(e) => {
                let _ = events.send(Event::Failed(format!("ibisOffice: {e}")));
                return wake(&waker);
            }
        };
        let mut sender = Some(sender);
        loop {
            let event = match receiver.recv() {
                Ok(HostMsg::Open { content, title, locale }) => match sender.take() {
                    Some(sender) => Event::Opened { object, content, title, locale, sender },
                    None => continue,
                },
                Ok(HostMsg::Ping) => Event::Focus(object),
                Ok(HostMsg::Close) | Err(_) => {
                    let _ = events.send(Event::Gone(object));
                    return wake(&waker);
                }
                Ok(other) => {
                    log(&format!("unexpected in a window: {other:?}"));
                    continue;
                }
            };
            if events.send(event).is_err() {
                return;
            }
            wake(&waker);
        }
    });
}

/// A font with Japanese in it, from the system, behind egui's own (titles are file names).
fn add_japanese_font(ctx: &egui::Context) {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let candidates = ["YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"];
    let Some(data) = candidates.iter().find_map(|f| std::fs::read(PathBuf::from(&windir).join("Fonts").join(f)).ok()) else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("japanese".into(), Arc::new(egui::FontData::from_owned(data)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("japanese".into());
    }
    ctx.set_fonts(fonts);
}

impl OfficeApp {
    pub fn new(ctx: &egui::Context, events: mpsc::Receiver<Event>, office: bool, file: Option<PathBuf>) -> OfficeApp {
        add_japanese_font(ctx);
        let ja = std::env::var("LANG").map(|l| l.starts_with("ja")).unwrap_or(false);
        let mut app = OfficeApp { tabs: Vec::new(), active: 0, events, ja, status: String::new(), office, script: std::env::var("CHEMBUILDER_OFFICE_TEST_SCRIPT").ok() };
        if let Some(path) = file {
            app.tabs.push(Tab::open_file(path, app.ja));
        } else if !office {
            // In the current directory, as `untitled.cdx` always was.
            app.tabs.push(Tab::untitled(Path::new(""), app.ja));
        }
        app
    }

    fn t(&self, ja: &'static str, en: &'static str) -> &'static str {
        if self.ja { ja } else { en }
    }

    fn take_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Opened { object, content, title, locale, sender } => {
                    self.ja = locale == "ja";
                    let bytes = content.stream(content::CONTENTS).map(<[u8]>::to_vec);
                    let mut tab = Tab::new(object, title, Source::Office { sender, original: content }, Some(bytes.as_deref().unwrap_or(b"")), self.ja);
                    if let Some(script) = &self.script {
                        run_script(script, &mut tab);
                    }
                    self.tabs.push(tab);
                    self.active = self.tabs.len() - 1;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Event::Focus(object) => {
                    if let Some(i) = self.tabs.iter().position(|t| t.object == object) {
                        self.active = i;
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Event::Gone(object) => {
                    if let Some(i) = self.tabs.iter().position(|t| t.object == object) {
                        // ibisOffice closed it: nothing more can be sent, only acknowledged.
                        let mut tab = self.tabs.remove(i);
                        if let Source::Office { sender, .. } = &mut tab.source {
                            let _ = sender.send(&EditorMsg::Closed);
                        }
                    }
                }
                Event::Failed(message) => {
                    log(&message);
                    self.status = message;
                }
            }
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
    }

    fn close_tab(&mut self, i: usize) {
        let tab = self.tabs.remove(i);
        tab.close();
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let (select, bond, eraser) = (self.t("選択", "Select"), self.t("結合", "Bond"), self.t("消しゴム", "Eraser"));
        let (wedge, hash, plain) = (self.t("くさび", "Wedge"), self.t("破線くさび", "Hash"), self.t("通常", "Plain"));
        let (undo, redo, clean) = (self.t("元に戻す", "Undo"), self.t("やり直し", "Redo"), self.t("整形", "Clean Up"));
        let paste = self.t("貼り付け", "Paste");
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        let ed = &mut tab.editor;
        ui.horizontal_wrapped(|ui| {
            for (tool, label) in [(Tool::Select, select), (Tool::Bond, bond), (Tool::Eraser, eraser)] {
                if ui.selectable_label(ed.tool == tool, label).clicked() {
                    ed.tool = tool;
                }
            }
            ui.separator();
            for (stereo, label) in [(BondStereo::None, plain), (BondStereo::WedgeUp, wedge), (BondStereo::WedgeDown, hash)] {
                if ui.selectable_label(ed.current_bond_stereo == stereo, label).clicked() {
                    ed.current_bond_stereo = stereo;
                }
            }
            ui.separator();
            if ui.button(undo).clicked() {
                ed.undo();
            }
            if ui.button(redo).clicked() {
                ed.redo();
            }
            if ui.button(paste).clicked() {
                ed.paste_clipboard();
            }
            if ui.button(clean).clicked() {
                ed.toggle_cleanup();
            }
        });
    }
}

/// What the status bar says becomes of an edit to an object from ibisOffice (`to_office`) or
/// to a file. An unreadable drawing (or a file another program changed) can still be drawn
/// on, but its edits are never sent or written (see [`Tab::flush`]) and are lost with the tab,
/// so it must not say they are kept.
fn edits_status(to_office: bool, unreadable: bool, ja: bool) -> &'static str {
    match (to_office, unreadable, ja) {
        (true, false, true) => "編集は自動的に反映されます",
        (true, false, false) => "Edits are sent back automatically",
        (true, true, true) => "編集は反映されません",
        (true, true, false) => "Edits are not sent back",
        (false, false, true) => "編集は自動的に保存されます",
        (false, false, false) => "Edits are saved automatically",
        (false, true, true) => "編集は保存されません",
        (false, true, false) => "Edits are not saved",
    }
}

/// Pan the view so the drawing is in the middle.
fn fit(editor: &mut ChemStructEditor) {
    let n = editor.molecule.atoms.len() as f32;
    if n == 0.0 {
        return;
    }
    let cx = editor.molecule.atoms.iter().map(|a| a.pos[0]).sum::<f32>() / n;
    let cy = editor.molecule.atoms.iter().map(|a| a.pos[1]).sum::<f32>() / n;
    editor.pan = egui::vec2(-cx * SCALE_FACTOR * editor.zoom, -cy * SCALE_FACTOR * editor.zoom);
}

/// The interop tests' edit: one carbon added beside the drawing.
fn run_script(script: &str, tab: &mut Tab) {
    if script == "add-atom" {
        let x = tab.editor.molecule.atoms.iter().map(|a| a.pos[0]).fold(0.0f32, f32::max) + 1.5;
        let y = tab.editor.molecule.atoms.first().map_or(0.0, |a| a.pos[1]);
        tab.editor.molecule.add_atom("C".into(), [x, y], 0);
        tab.changed = Some(Instant::now().checked_sub(SETTLE).unwrap_or_else(Instant::now));
    }
}

impl eframe::App for OfficeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_events(&ctx);

        if ctx.input(|i| i.viewport().close_requested()) {
            while !self.tabs.is_empty() {
                self.close_tab(0);
            }
            return;
        }
        if self.office && self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let title = match self.tabs.get(self.active) {
            Some(tab) => format!("{} — ChemBuilder", tab.title),
            None => "ChemBuilder".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        let mut close = None;
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, tab) in self.tabs.iter().enumerate() {
                    if ui.selectable_label(i == self.active, &tab.title).clicked() {
                        self.active = i;
                    }
                    if ui.small_button("×").clicked() {
                        close = Some(i);
                    }
                    ui.separator();
                }
            });
            self.toolbar(ui);
        });
        if let Some(i) = close {
            self.close_tab(i);
            return;
        }

        egui::Panel::bottom("status").show(ui, |ui| {
            let Some(tab) = self.tabs.get(self.active) else {
                ui.label(&self.status);
                return;
            };
            let what = tab.edits_status(self.ja);
            let outside = tab.changed_outside.then_some(self.t(
                "ファイルが別のアプリで変更されたため上書きしません",
                "The file was changed by another program and is not written over",
            ));
            let text = [tab.unreadable.as_deref(), outside, tab.note.as_deref(), Some(what), Some(self.status.as_str())]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("  ·  ");
            ui.label(text);
        });

        let pressed = ctx.input(|i| i.pointer.any_down());
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(tab) = self.tabs.get_mut(self.active) else { return };
            if tab.fit {
                fit(&mut tab.editor);
                tab.fit = false;
            }
            // Each edit starts the wait again, so a gesture goes back as one change.
            if tab.editor.ui(ui) {
                tab.changed = Some(Instant::now());
            }
        });

        // Every tab, not only the one shown: a scripted or background change still goes back.
        let mut failed = None;
        for tab in &mut self.tabs {
            if tab.editor.molecule != tab.sent && tab.changed.is_none() {
                tab.changed = Some(Instant::now());
            }
            if let Some(at) = tab.changed
                && !pressed
                && !tab.editor.is_cleaning()
                && at.elapsed() >= SETTLE
                && let Err(e) = tab.flush()
            {
                failed = Some(format!("{}: {e}", tab.title));
            }
        }
        if let Some(message) = failed {
            log(&message);
            self.status = message;
        }
        if self.script.is_some()
            && let Some(i) = self.tabs.iter().position(|t| t.editor.molecule == t.sent && t.changed.is_none())
        {
            // The scripted edit went out: done with this object.
            self.close_tab(i);
        }
        if self.tabs.iter().any(|t| t.changed.is_some()) {
            ctx.request_repaint_after(SETTLE / 4);
        }
    }
}

/// A line in `%LOCALAPPDATA%\ChemBuilder\office.log` (the window has no console).
pub fn log(message: &str) {
    use std::io::Write;
    let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("ChemBuilder")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("office.log")) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "{secs} {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::cdx_codec::{self, CdxObject};
    use crate::BondOrder;

    const OBJ_TEXT: u16 = 0x8006;
    const PROP_CREATION_PROGRAM: u16 = 0x0003;

    #[test]
    fn saving_a_file_keeps_what_the_editor_does_not_show() {
        // A ChemDraw drawing with a caption beside the structure.
        let mut doc = cdx_codec::parse(include_bytes!("../../tests/fixtures/chemdraw26_benzene.cdx")).unwrap();
        doc.children[0].children.push(CdxObject::new(OBJ_TEXT, 500));
        assert!(doc.prop(PROP_CREATION_PROGRAM).is_some(), "ChemDraw names itself");
        let original = cdx_codec::write(&doc);
        let path = std::env::temp_dir().join(format!("chembuilder-office-file-{}.cdx", std::process::id()));
        std::fs::write(&path, &original).unwrap();

        let mut tab = Tab::open_file(path.clone(), false);
        assert!(tab.note.is_some(), "the caption is announced as kept");
        for added in 1..=2 {
            let first = tab.editor.molecule.atoms[0].id;
            let pos = tab.editor.molecule.atoms[0].pos;
            let o = tab.editor.molecule.add_atom("O".to_string(), [pos[0] + added as f32, pos[1] - 1.5], 0);
            tab.editor.molecule.add_bond(first, o, BondOrder::Single);
            tab.flush().unwrap();

            let saved = std::fs::read(&path).unwrap();
            let import = cdx::read_cdx(&saved).unwrap();
            assert_eq!(import.molecule.atoms.len(), 6 + added, "the structure is replaced, not doubled");
            assert_eq!(import.other_objects, 1, "the caption stays");
            assert_eq!(cdx_codec::parse(&saved).unwrap().prop(PROP_CREATION_PROGRAM), doc.prop(PROP_CREATION_PROGRAM));
        }
        std::fs::remove_file(&path).unwrap();
    }

    /// A directory under the temp dir for one test, removed when the test ends (passed or not).
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("chembuilder-office-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn add_oxygen(tab: &mut Tab) {
        let pos = tab.editor.molecule.atoms.first().map_or([0.0, 0.0], |a| a.pos);
        tab.editor.molecule.add_atom("O".to_string(), [pos[0] + 1.5, pos[1]], 0);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_never_written_over() {
        // A directory stands in for a file that is there but cannot be read (locked, no access).
        let dir = TempDir::new("unreadable");
        let path = dir.0.join("drawing.cdx");
        std::fs::create_dir(&path).unwrap();

        let mut tab = Tab::open_file(path.clone(), false);
        assert!(tab.unreadable.is_some(), "the read error is shown, not an empty new drawing");
        add_oxygen(&mut tab);
        assert_eq!(tab.flush(), Ok(()), "nothing is written");
        assert!(path.is_dir(), "what was there is left alone");
    }

    #[test]
    fn a_file_that_cannot_be_read_says_its_edits_are_not_saved() {
        let dir = TempDir::new("unreadable-status");
        let path = dir.0.join("drawing.cdx");
        std::fs::create_dir(&path).unwrap();

        let tab = Tab::open_file(path, false);
        assert!(tab.unreadable.is_some());
        assert_eq!(tab.edits_status(false), "Edits are not saved");
        assert_eq!(tab.edits_status(true), "編集は保存されません");

        // One that can be read still says its edits are saved.
        let readable = Tab::open_file(dir.0.join("new.cdx"), false);
        assert_eq!(readable.edits_status(false), "Edits are saved automatically");
    }

    #[test]
    fn an_office_object_that_cannot_be_read_says_its_edits_are_not_sent_back() {
        // A tab for ibisOffice needs its connection, so the wording is checked on its own.
        assert_eq!(edits_status(true, true, false), "Edits are not sent back");
        assert_eq!(edits_status(true, true, true), "編集は反映されません");
        assert_eq!(edits_status(true, false, false), "Edits are sent back automatically");
    }

    #[test]
    fn a_file_that_is_not_there_yet_is_a_new_drawing() {
        let dir = TempDir::new("missing");
        let path = dir.0.join("new.cdx");

        let mut tab = Tab::open_file(path.clone(), false);
        assert!(tab.unreadable.is_none());
        assert!(tab.editor.molecule.atoms.is_empty());
        add_oxygen(&mut tab);
        tab.flush().unwrap();
        assert_eq!(cdx::read_cdx(&std::fs::read(&path).unwrap()).unwrap().molecule.atoms.len(), 1, "written on the first edit");
    }

    #[test]
    fn a_file_changed_by_another_program_is_not_written_over() {
        let dir = TempDir::new("changed-outside");
        let path = dir.0.join("drawing.cdx");
        std::fs::write(&path, include_bytes!("../../tests/fixtures/chemdraw26_benzene.cdx")).unwrap();
        let mut tab = Tab::open_file(path.clone(), false);
        add_oxygen(&mut tab);
        tab.flush().unwrap();
        assert!(!tab.changed_outside, "its own save is not taken for another program's");

        // ChemDraw saves the same file while it is open here.
        let theirs = cdx::molecule_to_cdx_document(&Molecule::default());
        std::fs::write(&path, &theirs).unwrap();
        add_oxygen(&mut tab);
        assert_eq!(tab.flush(), Ok(()));
        assert!(std::fs::read(&path).unwrap() == theirs, "their save is kept");
        assert!(tab.changed_outside, "the change is noticed");
        assert_eq!(tab.edits_status(false), "Edits are not saved");
        add_oxygen(&mut tab);
        tab.flush().unwrap();
        assert!(std::fs::read(&path).unwrap() == theirs, "nor written over later");

        // A new drawing whose file another program created in the meantime.
        let new = dir.0.join("new.cdx");
        let mut tab = Tab::open_file(new.clone(), false);
        std::fs::write(&new, &theirs).unwrap();
        add_oxygen(&mut tab);
        tab.flush().unwrap();
        assert!(std::fs::read(&new).unwrap() == theirs, "the file another program made is kept");
        assert!(tab.changed_outside);
    }

    #[test]
    fn an_earlier_untitled_drawing_is_opened_not_replaced() {
        let dir = TempDir::new("untitled");
        let original = include_bytes!("../../tests/fixtures/chemdraw26_benzene.cdx");
        std::fs::write(dir.0.join(UNTITLED), original).unwrap();

        let mut tab = Tab::untitled(&dir.0, false);
        assert_eq!(tab.title, UNTITLED);
        assert_eq!(tab.editor.molecule.atoms.len(), 6, "the earlier drawing is shown");
        add_oxygen(&mut tab);
        tab.flush().unwrap();
        let saved = cdx::read_cdx(&std::fs::read(dir.0.join(UNTITLED)).unwrap()).unwrap();
        assert_eq!(saved.molecule.atoms.len(), 7, "the edit goes onto the earlier drawing");
    }
}
