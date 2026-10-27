//! Test-only harness for driving [`ChemStructEditor`] through synthetic egui frames.
//!
//! This module is `#[cfg(test)]`, so nothing here reaches the public API.
//!
//! The central concern is **keyboard-only fidelity**. [`ChemStructEditor::ui`] reads the
//! pointer as `response.hover_pos().unwrap_or(egui::Pos2::ZERO)` and then asks whether the
//! canvas rect contains that point. When no pointer exists, that fallback reports screen
//! (0, 0) — so a canvas anchored at the origin silently hovers its own top-left corner and
//! "keyboard-only" tests quietly become keyboard-plus-a-phantom-cursor tests.
//!
//! Two things defend against that: [`CANVAS`] is offset away from the origin, and
//! [`kbd_frame`] asserts after every frame that no hover was fabricated.

use crate::widget::ChemStructEditor;

/// A canvas that does **not** contain `Pos2::ZERO`.
///
/// See the module docs: this offset is what keeps the no-pointer fallback in
/// `ChemStructEditor::ui` from fabricating a hover at the screen origin.
pub(crate) const CANVAS: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(900.0, 700.0));

/// Drive one frame with an arbitrary event list.
///
/// This is the shared version of the helper that was previously duplicated verbatim in
/// `widget::interact::tests` and `widget::mod::undo_redo_tests`.
pub(crate) fn run_frame(
    ctx: &egui::Context,
    screen_rect: egui::Rect,
    editor: &mut ChemStructEditor,
    events: Vec<egui::Event>,
) {
    run_frame_with_modifiers(ctx, screen_rect, editor, events, egui::Modifiers::default());
}

/// As [`run_frame`], but also sets `RawInput::modifiers`.
///
/// Some handlers read the *frame-level* modifier state rather than the per-event copy —
/// `ChemStructEditor::ui`'s Ctrl+Z/Ctrl+Y path does exactly that — so a chord carrying
/// modifiers has to set both or it will be seen as unmodified.
pub(crate) fn run_frame_with_modifiers(
    ctx: &egui::Context,
    screen_rect: egui::Rect,
    editor: &mut ChemStructEditor,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) {
    let raw = egui::RawInput {
        screen_rect: Some(screen_rect),
        modifiers,
        events,
        focused: true,
        ..Default::default()
    };
    let _ = ctx.run_ui(raw, |ui| {
        editor.ui(ui);
    });
}

/// Parse a chord spec into an egui key event plus its modifier set.
///
/// Grammar: zero or more of `Ctrl+` / `Alt+` / `Shift+` (case-insensitive), then one key
/// token. Key tokens mirror `interact::key_to_str`, which is what the shortcut tables are
/// matched against:
///
/// - `a`..`z` — the letter, unshifted
/// - `A`..`Z` — the letter, **shift implied** (how a user thinks of "press A")
/// - `0`..`9` — the digit row, unshifted
/// - `!@#$%^&*()` — the digit row, shift implied
/// - `` ` ``, `-`, `_`, `=`, `+`
/// - `Enter`, `Space`, `Tab`, `ArrowUp/Down/Left/Right`, `Delete`, `Backspace`, `Escape`
///
/// A `Ctrl+` or `Alt+` prefix suppresses the implied shift on an uppercase letter, so
/// `Ctrl+Z` is undo and `Ctrl+Shift+Z` is redo — matching conventional notation.
///
/// Note `Delete` / `Backspace` / `Escape` are accepted here but deliberately have no
/// `key_to_str` mapping in the widget; they reach handlers only via `Input::key_pressed`.
///
/// Returns a **press followed by a release**. The release is not optional: egui marks a
/// press as `repeat: true` when the key is already down, and `pressed_key_strings` drops
/// repeats — so a held key makes the second and later frames of `c 1 1 1` silently no-op.
pub(crate) fn chord(spec: &str) -> (Vec<egui::Event>, egui::Modifiers) {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut rest = spec;

    loop {
        let lower = rest.to_ascii_lowercase();
        if let Some(r) = lower.strip_prefix("ctrl+") {
            ctrl = true;
            rest = &rest[rest.len() - r.len()..];
        } else if let Some(r) = lower.strip_prefix("alt+") {
            alt = true;
            rest = &rest[rest.len() - r.len()..];
        } else if let Some(r) = lower.strip_prefix("shift+") {
            shift = true;
            rest = &rest[rest.len() - r.len()..];
        } else {
            break;
        }
    }

    use egui::Key;
    let key = match rest {
        "Enter" => Key::Enter,
        "Space" => Key::Space,
        "Tab" => Key::Tab,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "Delete" => Key::Delete,
        "Backspace" => Key::Backspace,
        "Escape" => Key::Escape,
        "`" => Key::Backtick,
        "-" => Key::Minus,
        "_" => {
            shift = true;
            Key::Minus
        }
        "=" => Key::Equals,
        "+" => {
            shift = true;
            Key::Equals
        }
        _ => {
            let mut chars = rest.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                panic!("testkit::chord: unrecognized key token {rest:?} in spec {spec:?}");
            };
            match c {
                'a'..='z' => letter_key(c),
                'A'..='Z' => {
                    // "A" means shift+a, but "Ctrl+A" means plain ctrl+a.
                    if !ctrl && !alt {
                        shift = true;
                    }
                    letter_key(c.to_ascii_lowercase())
                }
                '0'..='9' => digit_key(c),
                ')' | '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' => {
                    shift = true;
                    let idx = ")!@#$%^&*(".find(c).expect("checked above");
                    digit_key(char::from(b'0' + idx as u8))
                }
                _ => panic!("testkit::chord: unrecognized key char {c:?} in spec {spec:?}"),
            }
        }
    };

    let modifiers = egui::Modifiers { alt, ctrl, shift, mac_cmd: false, command: ctrl };
    let events = vec![
        egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers },
        egui::Event::Key { key, physical_key: Some(key), pressed: false, repeat: false, modifiers },
    ];
    (events, modifiers)
}

fn letter_key(c: char) -> egui::Key {
    use egui::Key::*;
    match c {
        'a' => A, 'b' => B, 'c' => C, 'd' => D, 'e' => E, 'f' => F, 'g' => G,
        'h' => H, 'i' => I, 'j' => J, 'k' => K, 'l' => L, 'm' => M, 'n' => N,
        'o' => O, 'p' => P, 'q' => Q, 'r' => R, 's' => S, 't' => T, 'u' => U,
        'v' => V, 'w' => W, 'x' => X, 'y' => Y, 'z' => Z,
        _ => unreachable!("letter_key called with {c:?}"),
    }
}

fn digit_key(c: char) -> egui::Key {
    use egui::Key::*;
    match c {
        '0' => Num0, '1' => Num1, '2' => Num2, '3' => Num3, '4' => Num4,
        '5' => Num5, '6' => Num6, '7' => Num7, '8' => Num8, '9' => Num9,
        _ => unreachable!("digit_key called with {c:?}"),
    }
}

/// Marks a token as *typed text* rather than a key chord: `text:P` types "P".
///
/// Deliberately unlike any chord — [`chord`] panics on unrecognized tokens, so a mistyped
/// prefix fails loudly instead of being silently read as a keypress.
pub(crate) const TEXT_PREFIX: &str = "text:";

/// Drive the frames that deliver `text` to whatever text field currently has focus.
///
/// Key events alone cannot do this: `egui::TextEdit` inserts from [`egui::Event::Text`], which
/// [`chord`] never emits. Without it the label editor — the app's route to any element with no
/// shortcut of its own, such as a bare P or Si — is unreachable from the harness, which reads
/// as a missing product feature when it is really a missing test capability.
///
/// **Two frames, and the empty one is load-bearing.** egui grants focus at the *start* of the
/// frame after `request_focus()`, and `ChemStructEditor::ui` requests focus only after showing
/// the `TextEdit` — so on the first frame the box exists it is not yet focused and silently
/// drops anything typed. A real user never notices, because frames tick past at 60 Hz between
/// one keystroke and the next; a one-chord-per-frame harness has to spend that frame on
/// purpose.
pub(crate) fn text_frames(ctx: &egui::Context, editor: &mut ChemStructEditor, text: &str) {
    run_frame(ctx, CANVAS, editor, Vec::new());
    run_frame(ctx, CANVAS, editor, vec![egui::Event::Text(text.to_string())]);
}

/// Drive exactly one chord — or one [`TEXT_PREFIX`] token — with **no pointer events at all**,
/// then assert that the frame really was keyboard-only.
///
/// The post-condition is the point of this helper: if a future change to the hover path
/// resurrects the phantom-cursor behaviour, every keyboard test fails loudly instead of
/// silently testing the mouse path.
///
/// One chord per frame is required, not stylistic: `pressed_key_strings` filters on
/// `repeat: false`, and the arrow-key handlers use `Input::key_pressed`, which is
/// edge-triggered per frame. (A `text:` token spends two frames; see [`text_frames`].)
pub(crate) fn kbd_frame(ctx: &egui::Context, editor: &mut ChemStructEditor, spec: &str) {
    match spec.strip_prefix(TEXT_PREFIX) {
        Some(text) => text_frames(ctx, editor, text),
        None => {
            let (events, modifiers) = chord(spec);
            run_frame_with_modifiers(ctx, CANVAS, editor, events, modifiers);
        }
    }

    assert!(
        editor.hovered_atom.is_none() && editor.hovered_bond.is_none(),
        "testkit: frame for {spec:?} fabricated hover state (atom={:?}, bond={:?}) — \
         the canvas must not contain Pos2::ZERO, or ChemStructEditor::ui's \
         `hover_pos().unwrap_or(ZERO)` fallback will pretend a cursor is present",
        editor.hovered_atom,
        editor.hovered_bond,
    );
}

/// Run a whitespace-separated chord sequence against a fresh editor, keyboard only.
///
/// A token may also be `text:<s>` ([`TEXT_PREFIX`]), which types `<s>` into an open text field
/// — so `` "c 1 ` text:P Enter" `` labels the second atom "P" through the real label editor.
/// Text tokens cannot contain whitespace, the sequence being whitespace-separated; element
/// symbols never do.
///
/// The editor uses [`Config::default`](crate::Config::default), which is
/// [`Config::embedded`](crate::Config::embedded) — compiled-in shortcuts and fragments, no
/// file I/O. Never switch this to `Config::load`: that reads `chembuilder_config.json` and
/// `fragments/` from the current directory, so a developer's local config would silently
/// change test outcomes.
pub(crate) fn type_keys(seq: &str) -> ChemStructEditor {
    let mut editor = ChemStructEditor::default();
    type_keys_into(&mut editor, seq);
    editor
}

/// As [`type_keys`], but against an editor you have already set up.
pub(crate) fn type_keys_into(editor: &mut ChemStructEditor, seq: &str) {
    let ctx = egui::Context::default();
    for spec in seq.split_whitespace() {
        kbd_frame(&ctx, editor, spec);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════════════════

/// The two fixtures in `tests/fixtures/rendering/` that `gen_test_structures.py` does not
/// produce. They are hand-made geometry edge cases for the render gallery, not chemistry
/// targets, so they are excluded from the keyboard-reachability goal.
pub(crate) const NON_SCRIPT_FIXTURES: [&str; 2] = ["bent_alkyne", "short_bond"];

/// Absolute path to the fixture directory.
///
/// Resolved from `CARGO_MANIFEST_DIR` rather than the process CWD: the existing integration
/// tests read fixtures through relative paths and break when run from anywhere but the repo
/// root, which is a trap worth not repeating.
pub(crate) fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rendering")
}

/// Load one fixture by bare name (no `.json`).
///
/// Fixtures are stored as bare `ClipMol` JSON — the `chembuilder-mol:` marker is added by
/// the consumer, as `examples/render_gallery.rs` does — so they are parsed through the same
/// public `paste_from_string` path the app uses. Positions get recentred on paste; that is
/// irrelevant here because comparison ignores coordinates.
pub(crate) fn load_fixture(name: &str) -> crate::molecule::Molecule {
    let path = fixture_dir().join(format!("{name}.json"));
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));

    let mut editor = ChemStructEditor::default();
    assert!(
        editor.paste_from_string(&format!("chembuilder-mol:{}", json.trim())),
        "fixture {name} is not in the editor's clipboard format",
    );
    editor.molecule
}

/// Every fixture on disk, sorted by name.
pub(crate) fn all_fixtures() -> Vec<(String, crate::molecule::Molecule)> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("fixture directory should exist")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    names.sort();
    names.into_iter().map(|n| { let m = load_fixture(&n); (n, m) }).collect()
}

/// The goal set: every structure `scripts/gen_test_structures.py` generates.
///
/// This is the definition of "any structure" for the keyboard-only objective.
pub(crate) fn script_fixtures() -> Vec<(String, crate::molecule::Molecule)> {
    all_fixtures()
        .into_iter()
        .filter(|(n, _)| !NON_SCRIPT_FIXTURES.contains(&n.as_str()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Shape fingerprints
// ═══════════════════════════════════════════════════════════════════════════════

/// A line-per-shape text fingerprint of a frame's painted shapes, in paint order.
///
/// Used to pin what the editor draws so a restructuring of the draw path can show that the
/// output did not change. `Shape::Vec` groups are flattened (grouping is not visible on
/// screen), `Noop` is skipped, and a `clip` line is emitted whenever the clip rect changes.
///
/// Coordinates are rounded to 3 decimals so the fingerprint does not hinge on the last bits
/// of float arithmetic. Text is recorded by its string, section colours and font sizes and
/// its rounded position, not by its glyph geometry, so it does not depend on how the font
/// is rasterized.
///
/// One ordering is deliberately not pinned: within a run of consecutive unstroked rects of
/// the same fill colour, the lines are sorted. Painting one colour over itself gives the same
/// pixels in any order, and the editor paints its white label backgrounds by iterating a
/// `HashMap`, so their order varies from run to run without any visible effect.
pub(crate) fn shape_fingerprint(shapes: &[egui::epaint::ClippedShape]) -> String {
    let mut lines = Vec::new();
    let mut clip = None;
    for clipped in shapes {
        if clip != Some(clipped.clip_rect) {
            clip = Some(clipped.clip_rect);
            lines.push(FpLine::new(format!("clip {}", fp_rect(clipped.clip_rect))));
        }
        fp_shape(&clipped.shape, &mut lines);
    }

    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let mut j = i + 1;
        if let Some(fill) = lines[i].plain_fill {
            while j < lines.len() && lines[j].plain_fill == Some(fill) {
                j += 1;
            }
        }
        let mut run: Vec<&str> = lines[i..j].iter().map(|l| l.text.as_str()).collect();
        run.sort_unstable();
        out.extend(run);
        i = j;
    }
    out.join("\n")
}

/// One fingerprint line. `plain_fill` is the fill colour of an unstroked rect, which decides
/// the neighbours it commutes with (see [`shape_fingerprint`]); `None` for every other shape.
struct FpLine {
    text: String,
    plain_fill: Option<egui::Color32>,
}

impl FpLine {
    fn new(text: String) -> Self {
        Self { text, plain_fill: None }
    }
}

fn fp_num(v: f32) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    // Fold -0.000 into 0.000 so the sign of a vanishing value is not part of the fingerprint.
    format!("{:.3}", if r == 0.0 { 0.0 } else { r })
}

fn fp_pos(p: egui::Pos2) -> String {
    format!("({},{})", fp_num(p.x), fp_num(p.y))
}

fn fp_rect(r: egui::Rect) -> String {
    format!("{}-{}", fp_pos(r.min), fp_pos(r.max))
}

fn fp_color(c: egui::Color32) -> String {
    let [r, g, b, a] = c.to_array();
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

fn fp_stroke(s: egui::Stroke) -> String {
    format!("{}/{}", fp_num(s.width), fp_color(s.color))
}

fn fp_shape(shape: &egui::Shape, out: &mut Vec<FpLine>) {
    use egui::Shape;
    let text = match shape {
        Shape::Noop => return,
        Shape::Vec(shapes) => {
            for s in shapes {
                fp_shape(s, out);
            }
            return;
        }
        Shape::Rect(r) => {
            let unstroked = r.stroke.width == 0.0 || r.stroke.color.a() == 0;
            out.push(FpLine {
                text: format!(
                    "rect {} fill={} stroke={}",
                    fp_rect(r.rect),
                    fp_color(r.fill),
                    fp_stroke(r.stroke),
                ),
                plain_fill: unstroked.then_some(r.fill),
            });
            return;
        }
        Shape::Circle(c) => format!(
            "circle c={} r={} fill={} stroke={}",
            fp_pos(c.center),
            fp_num(c.radius),
            fp_color(c.fill),
            fp_stroke(c.stroke),
        ),
        Shape::Ellipse(e) => format!(
            "ellipse c={} r=({},{}) fill={} stroke={}",
            fp_pos(e.center),
            fp_num(e.radius.x),
            fp_num(e.radius.y),
            fp_color(e.fill),
            fp_stroke(e.stroke),
        ),
        Shape::LineSegment { points, stroke } => format!(
            "line {} {} stroke={}",
            fp_pos(points[0]),
            fp_pos(points[1]),
            fp_stroke(*stroke),
        ),
        Shape::Path(p) => {
            let stroke_color = match &p.stroke.color {
                egui::epaint::ColorMode::Solid(c) => fp_color(*c),
                egui::epaint::ColorMode::UV(_) => "uv".to_string(),
            };
            let pts: Vec<String> = p.points.iter().map(|q| fp_pos(*q)).collect();
            format!(
                "path closed={} fill={} stroke={}/{} pts=[{}]",
                p.closed,
                fp_color(p.fill),
                fp_num(p.stroke.width),
                stroke_color,
                pts.join(" "),
            )
        }
        Shape::Text(t) => {
            let job = &t.galley.job;
            let sections: Vec<String> = job
                .sections
                .iter()
                .map(|s| {
                    format!(
                        "{:?}:{}@{}",
                        &job.text[s.byte_range.start.0..s.byte_range.end.0],
                        fp_color(s.format.color),
                        fp_num(s.format.font_id.size),
                    )
                })
                .collect();
            format!("text {:?} at={} sections=[{}]", job.text, fp_pos(t.pos), sections.join(" "))
        }
        Shape::Mesh(m) => format!("mesh vertices={} indices={}", m.vertices.len(), m.indices.len()),
        Shape::QuadraticBezier(b) => {
            let pts: Vec<String> = b.points.iter().map(|q| fp_pos(*q)).collect();
            format!("qbezier [{}] fill={}", pts.join(" "), fp_color(b.fill))
        }
        Shape::CubicBezier(b) => {
            let pts: Vec<String> = b.points.iter().map(|q| fp_pos(*q)).collect();
            format!("cbezier [{}] fill={}", pts.join(" "), fp_color(b.fill))
        }
        Shape::Callback(_) => "callback".to_string(),
    };
    out.push(FpLine::new(text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_excludes_the_screen_origin() {
        assert!(
            !CANVAS.contains(egui::Pos2::ZERO),
            "the whole keyboard-only harness rests on this"
        );
    }

    #[test]
    fn chord_parses_modifiers_and_shift_conventions() {
        let plain = chord("a").1;
        assert!(!plain.shift && !plain.ctrl && !plain.alt);

        // Uppercase implies shift...
        assert!(chord("A").1.shift);
        // ...but not when Ctrl/Alt are present, so Ctrl+Z is undo not redo.
        let undo = chord("Ctrl+Z").1;
        assert!(undo.ctrl && !undo.shift);
        let redo = chord("Ctrl+Shift+Z").1;
        assert!(redo.ctrl && redo.shift);

        // Shifted digit-row symbols.
        assert!(chord("^").1.shift);
        assert!(!chord("6").1.shift);
    }

    /// The harness must reproduce what the widget's own key parser would see, otherwise
    /// tests would exercise a key vocabulary the app does not actually have.
    #[test]
    fn chord_agrees_with_the_widgets_key_parser() {
        for (spec, expected) in [
            ("a", "a"), ("A", "A"), ("6", "6"), ("^", "^"),
            ("`", "`"), ("-", "-"), ("=", "="), ("+", "+"),
            ("Enter", "Enter"), ("Space", "Space"), ("Tab", "Tab"),
            ("ArrowLeft", "ArrowLeft"),
        ] {
            let (events, _) = chord(spec);
            let egui::Event::Key { key, modifiers, .. } = events[0] else {
                panic!("chord({spec:?}) did not produce a key event");
            };
            assert_eq!(
                crate::widget::interact::key_to_str(key, modifiers.shift).as_deref(),
                Some(expected),
                "chord({spec:?}) must round-trip through key_to_str",
            );
        }
    }

    /// A chord must be delivered on *every* frame, including when the same key repeats.
    ///
    /// egui stamps `repeat: true` on a press whose key is still down, and
    /// `pressed_key_strings` drops repeats — so without the release event in [`chord`], the
    /// second `o` here is silently swallowed and the test reads as "the app cannot place two
    /// oxygens", which is false. Any golden sequence with a doubled key depends on this.
    #[test]
    fn a_repeated_key_is_delivered_on_every_frame() {
        let editor = type_keys("o o");
        assert_eq!(
            editor.molecule.atoms.len(),
            2,
            "the second `o` must not be swallowed as an auto-repeat",
        );
    }

    /// A `text:` token must actually reach a text field, not just tick a frame.
    ///
    /// The whole point of [`text_frames`] is that `egui::TextEdit` inserts from
    /// `Event::Text`, which [`chord`] cannot produce — so a harness built only on chords
    /// silently cannot drive the label editor, and the feature looks broken when it is only
    /// untestable. This pins the capability itself, independently of what the widget does
    /// with it.
    #[test]
    fn a_text_token_types_into_the_label_editor() {
        let editor = type_keys("c ` text:Xx Enter");
        assert_eq!(
            editor.molecule.atoms.first().map(|a| a.element.as_str()),
            Some("Xx"),
            "text: should be delivered as typed characters, not as key chords",
        );
    }

    /// The settling frame in [`text_frames`] is not padding. egui grants focus at the start of
    /// the frame *after* `request_focus()`, so text sent on the frame the box first appears is
    /// dropped — this asserts the harness spends that frame, by showing the naive one-frame
    /// version fails where the real helper succeeds.
    #[test]
    fn text_needs_the_frame_egui_spends_granting_focus() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        for spec in ["c", "`"] {
            kbd_frame(&ctx, &mut editor, spec);
        }
        // One frame only: the TextEdit exists but is not focused yet, so this is swallowed.
        run_frame(&ctx, CANVAS, &mut editor, vec![egui::Event::Text("Xx".to_string())]);
        assert_eq!(
            editor.label_edit.as_ref().map(|(_, t)| t.as_str()),
            Some("C"),
            "text on the editor's very first frame must land nowhere — if this starts passing, \
             the settling frame in text_frames is no longer needed",
        );
    }

    /// No pointer means no hover — even when the canvas contains the screen origin.
    ///
    /// This is the regression guard for `ChemStructEditor::ui`'s hover fallback. It
    /// deliberately uses an origin-anchored canvas (not [`CANVAS`]) so that it tests the
    /// widget rather than the harness's offset. Before the fix, the second `o` here found a
    /// phantom cursor sitting on the atom the first `o` had just placed, and was routed into
    /// `dispatch_atom_key` instead of placing a second atom.
    #[test]
    fn no_pointer_means_no_hover_even_on_an_origin_canvas() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let origin_canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));

        for _ in 0..2 {
            let (events, modifiers) = chord("o");
            run_frame_with_modifiers(&ctx, origin_canvas, &mut editor, events, modifiers);
            assert!(
                editor.hovered_atom.is_none() && editor.hovered_bond.is_none(),
                "a pointerless frame must not fabricate hover",
            );
        }
        assert_eq!(
            editor.last_mouse_mol, [0.0, 0.0],
            "a pointerless frame must not move the insertion cursor",
        );
        assert_eq!(
            editor.molecule.atoms.len(),
            2,
            "both keypresses should place an atom; a phantom hover would divert the second",
        );
    }

    /// Empirical validation of the canonicaliser against 125 real, distinct molecules.
    ///
    /// [`canonical_key`](crate::molecule::canonical::canonical_key) is a WL refinement, so
    /// collisions are possible in principle. These fixtures are all chemically different, so
    /// any pair sharing a key is a genuine false positive — a far stronger check than
    /// hand-written cases can give.
    #[test]
    fn every_fixture_has_a_distinct_canonical_key() {
        use crate::molecule::canonical::canonical_key;
        use std::collections::HashMap;

        let mut seen: HashMap<String, String> = HashMap::new();
        for (name, mol) in all_fixtures() {
            if let Some(prev) = seen.insert(canonical_key(&mol), name.clone()) {
                panic!("canonical_key collision between distinct structures: {prev} and {name}");
            }
        }
        assert_eq!(seen.len(), 125, "expected 125 fixtures on disk");
    }

    /// Identity must survive re-ordering, which is the whole reason this comparison exists:
    /// a keyboard-built structure never shares atom ids or ordering with a fixture.
    #[test]
    fn fixtures_are_isomorphic_to_their_own_reordering() {
        use crate::molecule::canonical::{canonical_key, is_isomorphic};

        for (name, mol) in all_fixtures() {
            let shuffled = reverse_atom_order(&mol);
            assert_eq!(
                canonical_key(&mol),
                canonical_key(&shuffled),
                "{name}: canonical key changed under reordering",
            );
            assert!(is_isomorphic(&mol, &shuffled), "{name}: not isomorphic to itself reordered");
        }
    }

    /// Rebuild a molecule with atoms added in reverse order, so every id and index differs.
    fn reverse_atom_order(mol: &crate::molecule::Molecule) -> crate::molecule::Molecule {
        let mut out = crate::molecule::Molecule::default();
        let mut id_map = std::collections::HashMap::new();
        for a in mol.atoms.iter().rev() {
            id_map.insert(a.id, out.add_atom(a.element.clone(), a.pos, a.charge));
        }
        for b in mol.bonds.iter().rev() {
            // Swap begin/end too, to catch any residual endpoint-order sensitivity.
            let nb = out.add_bond(id_map[&b.end], id_map[&b.begin], b.order.clone());
            if let Some(bond) = out.bond_by_id_mut(nb) {
                bond.stereo = b.stereo.clone();
            }
        }
        out
    }

    #[test]
    fn the_goal_set_is_the_generator_script_output() {
        assert_eq!(
            script_fixtures().len(),
            123,
            "the keyboard-only goal is defined as gen_test_structures.py's output",
        );
    }

    #[test]
    fn keyboard_only_frames_place_and_track_a_hotspot() {
        let editor = type_keys("o");
        assert_eq!(editor.molecule.atoms.len(), 1, "`o` should place one atom");
        assert!(editor.hovered_atom.is_none());
        assert!(
            editor.hotspot_atom.is_some(),
            "placing at the cursor must set the hotspot, or nothing can be built from it",
        );
    }
}
