//! キーボード操作を「読む」のではなく「見る」ためのデモアプリ。
//!
//! 実行: `cargo run --example keyboard_demo`
//!
//! 画面は左からキー入力の表示、中央にエディタのキャンバス、右にゴールデン列のリプレイ操作。
//! できることは 2 つ:
//!
//! 1. **ライブ表示** — キャンバスに向かって普通にキーを打つと、押されたキーが履歴として
//!    流れていく。スクリーンキャストのキーオーバーレイと同じ発想で、キーボードだけで構造を
//!    組む様子を第三者が追えるようにするためのもの。
//! 2. **リプレイ** — `scripts/gen_test_structures.py` が生成する 123 構造それぞれについて
//!    ソルバが見つけたキー列を、1 キーずつエディタに流し込んで組み上がる過程を見せる。
//!
//! キー列の出典は `target/keyboard_goldens.txt`（ビルド生成物なので無いこともある）。
//! 再生成は `cargo test --release --lib solve_all_fixtures -- --ignored --nocapture`。
//!
//! ネイティブ専用。`examples/editor.rs` と違い wasm のエントリポイントは持たない
//! （ゴールデン列をファイルから実行時に読むため）。

use chembuider_rs::egui;
use chembuider_rs::ChemStructEditor;
use eframe::App;
use std::collections::VecDeque;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 820.0])
            .with_title("ChemBuilder — キーボード操作デモ"),
        ..Default::default()
    };
    eframe::run_native(
        "ChemBuilder Keyboard Demo",
        options,
        Box::new(|cc| {
            install_japanese_font(&cc.egui_ctx);
            Ok(Box::new(DemoApp::new()))
        }),
    )
}

// ═══════════════════════════════════════════════════════════════════════════════
// フォント
// ═══════════════════════════════════════════════════════════════════════════════

/// egui の組み込みフォントには日本語のグリフが一切無く、放っておくと本アプリの UI 文字列は
/// すべて豆腐になる。OS のゴシック体を**フォールバックとして末尾に**足すことで、ASCII は
/// 既定フォントのまま、日本語だけがこちらに落ちてくるようにする。
///
/// 見つからなくてもアプリは動く（日本語が読めなくなるだけ）ので、失敗は stderr の警告に留める。
fn install_japanese_font(ctx: &egui::Context) {
    // `.ttc`（TrueType コレクション）はインデックス 0 の face を使う。
    const CANDIDATES: &[&str] = &[
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        r"C:\Windows\Fonts\msgothic.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
    ];

    let Some((path, bytes)) = CANDIDATES
        .iter()
        .find_map(|p| std::fs::read(p).ok().map(|b| (*p, b)))
    else {
        eprintln!(
            "警告: 日本語フォントが見つかりませんでした。UI の日本語が表示されません。\n\
             探索したパス: {CANDIDATES:?}"
        );
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("jp".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("jp".to_owned());
    }
    ctx.set_fonts(fonts);
    eprintln!("日本語フォント: {path}");
}

// ═══════════════════════════════════════════════════════════════════════════════
// コード（キー和音）のパースと表示
// ═══════════════════════════════════════════════════════════════════════════════

/// `text:` で始まるトークンは「ラベル編集ボックスにこの文字列を打ち込む」という意味。
///
/// `src/testkit.rs` の `TEXT_PREFIX` と同じ規約。あちらは `#[cfg(test)]` なので example
/// からは触れず、ここに書き写している（パーサも同様）。
const TEXT_PREFIX: &str = "text:";

/// 1 フレーム分のキー入力。
struct Chord {
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
}

/// コード指定を egui のキーイベント列と修飾キー集合に変換する。
///
/// 文法は `Ctrl+` / `Alt+` / `Shift+` の任意個に続けてキートークン 1 個。トークンは
/// `widget::interact::key_to_str` の語彙に対応する:
///
/// - `a`..`z` — 素の英字 / `A`..`Z` — Shift 付き英字
/// - `0`..`9` — 数字段 / `!@#$%^&*()` — Shift 付き数字段
/// - `` ` ``, `-`, `_`, `=`, `+`
/// - `Enter`, `Space`, `Tab`, `ArrowUp/Down/Left/Right`, `Delete`, `Backspace`, `Escape`
///
/// `Ctrl+` / `Alt+` が付くと大文字が暗黙に含む Shift は打ち消される。よって `Ctrl+Z` は
/// Undo、`Ctrl+Shift+Z` は Redo という慣用表記どおりに読める。
///
/// 返すのは**押下と離上の 2 イベント**で、離上は省略できない。egui は押しっぱなしのキーの
/// 押下に `repeat: true` を立て、`pressed_key_strings` は repeat を捨てるので、離上を送らない
/// と `c 1 1 1` の 2 個目以降が黙って無視される。
///
/// `testkit::chord` と違い、未知のトークンでは panic せずエラーを返す。ゴールデン列は
/// ビルド生成物であり、壊れていてもアプリを落とすべきではない。
fn parse_chord(spec: &str) -> Result<Chord, String> {
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
                return Err(format!("未知のキートークン {rest:?}（指定: {spec:?}）"));
            };
            match c {
                'a'..='z' => letter_key(c),
                'A'..='Z' => {
                    if !ctrl && !alt {
                        shift = true;
                    }
                    letter_key(c.to_ascii_lowercase())
                }
                '0'..='9' => digit_key(c),
                _ if SHIFTED_DIGITS.contains(c) => {
                    shift = true;
                    let idx = SHIFTED_DIGITS.find(c).expect("直前のガードで確認済み");
                    DIGIT_KEYS[idx]
                }
                _ => return Err(format!("未知のキー文字 {c:?}（指定: {spec:?}）")),
            }
        }
    };

    let modifiers = egui::Modifiers { alt, ctrl, shift, mac_cmd: false, command: ctrl };
    Ok(Chord {
        events: vec![
            egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers },
            egui::Event::Key { key, physical_key: Some(key), pressed: false, repeat: false, modifiers },
        ],
        modifiers,
    })
}

/// 添字がそのまま数字に対応する数字段のキー。パースと表示の両方がこの 1 本の対応表を使う。
const DIGIT_KEYS: [egui::Key; 10] = {
    use egui::Key::*;
    [Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9]
};

/// 数字段を Shift と一緒に押したときの記号。添字は [`DIGIT_KEYS`] と共通。
const SHIFTED_DIGITS: &str = ")!@#$%^&*(";

/// 添字 0 が `a`。こちらもパースと表示で共有する。
const LETTER_KEYS: [egui::Key; 26] = {
    use egui::Key::*;
    [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z]
};

fn letter_key(c: char) -> egui::Key {
    LETTER_KEYS[(c as u8 - b'a') as usize]
}

fn digit_key(c: char) -> egui::Key {
    DIGIT_KEYS[(c as u8 - b'0') as usize]
}

/// キーと修飾キーを [`parse_chord`] が受け付けるトークン表記に戻す。
///
/// 表示専用だが、あえて同じ文法に揃えてある。オーバーレイに出た文字列をそのままゴールデン列に
/// 貼れる（＝人手でキー列を書き起こせる）方が、このデモの用途に合う。
fn format_chord(key: egui::Key, m: egui::Modifiers) -> String {
    let ctrl_like = m.ctrl || m.command;
    // Ctrl/Alt が付くと大文字は Shift を意味しなくなるので、慣用表記に合わせて英字は大文字で
    // 出し、Shift は明示的な接頭辞に回す（`Ctrl+Z` と `Ctrl+Shift+Z` が見分けられる）。
    let upper_letters = ctrl_like || m.alt;
    let (token, shift_absorbed) = key_token(key, m.shift && !upper_letters, upper_letters);

    let mut out = String::new();
    if ctrl_like {
        out.push_str("Ctrl+");
    }
    if m.alt {
        out.push_str("Alt+");
    }
    // `Shift+ArrowLeft` のように、トークン自身が Shift を表現できないキーだけ接頭辞を足す。
    if m.shift && !shift_absorbed {
        out.push_str("Shift+");
    }
    out.push_str(&token);
    out
}

/// トークン本体と、「そのトークンが Shift を自分に取り込んだか」を返す。
///
/// 取り込めるのは `A`・`!`・`_`・`+` のように Shift 付きの字形を持つキーだけ。矢印や Enter は
/// 取り込めないので、呼び出し側が `Shift+` を接頭辞として足す必要がある。
///
/// `shift` はトークン自身に Shift を織り込んでよいか、`upper_letters` は英字を大文字で出すか。
///
/// 矢印や数字を `egui::Key::name()` 任せにはできない。あれは `ArrowDown` を `"Down"`、
/// `Num1` を `"1"` と返すので、そのままではトークン文法とずれるうえ Shift も落ちる。
fn key_token(key: egui::Key, shift: bool, upper_letters: bool) -> (String, bool) {
    use egui::Key::*;
    let plain = match key {
        Enter => "Enter",
        Space => "Space",
        Tab => "Tab",
        ArrowUp => "ArrowUp",
        ArrowDown => "ArrowDown",
        ArrowLeft => "ArrowLeft",
        ArrowRight => "ArrowRight",
        Delete => "Delete",
        Backspace => "Backspace",
        Escape => "Escape",
        Backtick => "`",
        Minus => return (if shift { "_" } else { "-" }.to_owned(), shift),
        Equals => return (if shift { "+" } else { "=" }.to_owned(), shift),
        _ => "",
    };
    if !plain.is_empty() {
        return (plain.to_owned(), false);
    }

    if let Some(i) = DIGIT_KEYS.iter().position(|k| *k == key) {
        return if shift {
            (SHIFTED_DIGITS.chars().nth(i).expect("SHIFTED_DIGITS は 10 文字").to_string(), true)
        } else {
            (char::from(b'0' + i as u8).to_string(), false)
        };
    }
    if let Some(i) = LETTER_KEYS.iter().position(|k| *k == key) {
        let c = char::from(b'a' + i as u8);
        return if shift {
            (c.to_ascii_uppercase().to_string(), true)
        } else if upper_letters {
            // Ctrl/Alt 付きの大文字は Shift を意味しない（`Ctrl+Z` は Undo）。
            (c.to_ascii_uppercase().to_string(), false)
        } else {
            (c.to_string(), false)
        };
    }

    // 語彙外のキー（F1 など）。表示はするが [`parse_chord`] には戻せない。実キーボードには
    // エディタが使わないキーがいくらでもあるので、黙って落とすより出したほうが分かりやすい。
    (key.name().to_owned(), false)
}

// ═══════════════════════════════════════════════════════════════════════════════
// ゴールデン列の読み込み
// ═══════════════════════════════════════════════════════════════════════════════

const REGEN_CMD: &str = "cargo test --release --lib solve_all_fixtures -- --ignored --nocapture";

/// 1 構造ぶんのキー列。
struct Golden {
    name: String,
    tokens: Vec<String>,
    /// パースできないトークンがあった場合の理由。再生ボタンを塞ぐために持っておく。
    problem: Option<String>,
}

/// `target/keyboard_goldens.txt` を実行時に読む。
///
/// `include_str!` にしないのは意図的で、このファイルはビルド生成物（ソルバが書き出す）
/// なので、無い状態でもビルドが通り、アプリが起動し、再生成コマンドを案内できる必要がある。
fn load_goldens() -> Result<Vec<Golden>, String> {
    let mut tried = Vec::new();
    let mut last_err = None;

    // ワークスペース直下と、カレントディレクトリからの相対の両方を見る。前者は
    // `cargo run` をどこから叩いても効き、後者は成果物だけ持ち出した場合に効く。
    let candidates = [
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/keyboard_goldens.txt"),
        std::path::PathBuf::from("target/keyboard_goldens.txt"),
    ];
    for path in &candidates {
        match std::fs::read_to_string(path) {
            Ok(text) => return Ok(parse_goldens(&text)),
            Err(e) => {
                tried.push(path.display().to_string());
                last_err = Some(e);
            }
        }
    }

    Err(format!(
        "ゴールデン列のファイルが読めませんでした。\n\n\
         探索したパス:\n  {}\n\n\
         理由: {}\n\n\
         これはビルド生成物なので、リポジトリには含まれていません。\n\
         次のコマンドで再生成してください（数分かかります）:\n\n  {REGEN_CMD}\n\n\
         再生成しなくても、キャンバスに直接キーを打つライブ表示は使えます。",
        tried.join("\n  "),
        last_err.map(|e| e.to_string()).unwrap_or_default(),
    ))
}

fn parse_goldens(text: &str) -> Vec<Golden> {
    text.lines()
        .filter_map(|line| {
            let (name, seq) = line.split_once('\t')?;
            let (name, seq) = (name.trim(), seq.trim());
            if name.is_empty() {
                return None;
            }
            // ソルバは解けなかった構造を "UNSOLVED" と書き出す。現状は 123 個すべて解けて
            // いるが、古い/部分的なファイルでも落ちないように扱っておく。
            if seq == "UNSOLVED" {
                return Some(Golden {
                    name: name.to_owned(),
                    tokens: Vec::new(),
                    problem: Some("この構造はソルバが未解決（UNSOLVED）です。".to_owned()),
                });
            }
            let tokens: Vec<String> = seq.split_whitespace().map(str::to_owned).collect();
            let problem = tokens.iter().find_map(|t| {
                if t.starts_with(TEXT_PREFIX) {
                    return None;
                }
                parse_chord(t).err()
            });
            Some(Golden { name: name.to_owned(), tokens, problem })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// リプレイ
// ═══════════════════════════════════════════════════════════════════════════════

/// 1 フレームぶんの送出結果。
enum Emitted {
    /// トークンを 1 個消化した。表示用の文字列を返す（`text:` トークンのみ Some）。
    Token(Option<String>),
    /// このトークンにはもう 1 フレーム必要（フォーカス確定待ち）。
    NeedsAnotherFrame,
}

struct Replay {
    /// 選択中のゴールデン列のインデックス。
    selected: Option<usize>,
    tokens: Vec<String>,
    /// 次に送るトークンの位置。
    idx: usize,
    playing: bool,
    /// 「1 キー進む」が押された。
    step: bool,
    /// `text:` トークンの整定フレームを消化済みで、次フレームで実際に文字を流す状態。
    awaiting_text: bool,
    /// 1 キーあたりの秒数。
    interval: f32,
    /// 次に送出してよい時刻（`InputState::time`）。
    next_at: f64,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            selected: None,
            tokens: Vec::new(),
            idx: 0,
            playing: false,
            step: false,
            awaiting_text: false,
            interval: 0.6,
            next_at: 0.0,
        }
    }
}

impl Replay {
    fn finished(&self) -> bool {
        self.idx >= self.tokens.len()
    }

    fn rewind(&mut self) {
        self.idx = 0;
        self.playing = false;
        self.step = false;
        self.awaiting_text = false;
        self.next_at = 0.0;
    }

    /// このフレームに 1 コードぶんの入力を egui へ流し込む。
    ///
    /// **1 フレームに 1 コードまで**というのは様式ではなく必須の制約。egui は押しっぱなしの
    /// キーの押下に `repeat: true` を立て、ウィジェット側の `pressed_key_strings` は repeat を
    /// 捨てる。矢印キーのハンドラも `Input::key_pressed` のフレーム単位のエッジを見ている。
    /// 同じフレームに 2 個詰め込むと、片方が黙って無かったことになる。
    fn emit(&mut self, ctx: &egui::Context) -> Emitted {
        let token = self.tokens[self.idx].clone();

        if let Some(text) = token.strip_prefix(TEXT_PREFIX) {
            if !self.awaiting_text {
                // 何も送らないフレームを 1 枚はさむ。`` ` `` を受けたエディタは
                // `request_focus()` を呼ぶが、egui がフォーカスを与えるのは**次フレームの
                // 冒頭**なので、直後のフレームに流した文字は行き先が無く捨てられる。実利用では
                // 60 Hz のフレームが打鍵の合間に勝手に流れて誰も気付かないが、1 フレーム 1 コード
                // で駆動する側は自分でこの 1 枚を使わなければならない。
                self.awaiting_text = true;
                return Emitted::NeedsAnotherFrame;
            }
            self.awaiting_text = false;
            ctx.input_mut(|i| i.events.push(egui::Event::Text(text.to_owned())));
            self.idx += 1;
            // キーイベントが出ないので、履歴には送出側から積む。
            return Emitted::Token(Some(token));
        }

        match parse_chord(&token) {
            Ok(chord) => ctx.input_mut(|i| {
                // 修飾キーはイベントに載せるだけでは足りない。Ctrl+Z / Ctrl+K や
                // Shift+矢印 のハンドラは*フレーム単位*の `i.modifiers` を読むので、
                // 両方を揃えないと素のキーとして扱われる。
                i.modifiers = chord.modifiers;
                i.events.extend(chord.events);
            }),
            // パース不能は読み込み時に検出済み。ここに来たら黙って読み飛ばす。
            Err(_) => {}
        }
        self.idx += 1;
        Emitted::Token(None)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// キー履歴
// ═══════════════════════════════════════════════════════════════════════════════

const LOG_LEN: usize = 15;

struct LogEntry {
    text: String,
    /// リプレイが送った合成キーか、人間が実際に押したキーか。
    synthetic: bool,
}

// ═══════════════════════════════════════════════════════════════════════════════
// アプリ
// ═══════════════════════════════════════════════════════════════════════════════

struct DemoApp {
    editor: ChemStructEditor,
    goldens: Result<Vec<Golden>, String>,
    filter: String,
    replay: Replay,
    log: VecDeque<LogEntry>,
    /// 直近のキーが押された時刻。大きく表示する現在のコードを一瞬だけ強調するために使う。
    last_key_at: f64,
    show_legend: bool,
}

impl DemoApp {
    fn new() -> Self {
        Self {
            // `ChemStructEditor::default()` は `Config::embedded()`、つまり組み込みの
            // ショートカット表とフラグメントを使う。`examples/editor.rs` のように
            // `Config::load()` にしてはいけない: カレントディレクトリの
            // `chembuilder_config.json` を読むので、利用者のローカル設定でキー割り当てが
            // 変わり、ゴールデン列が前提とする挙動から静かにずれる。
            editor: ChemStructEditor::default(),
            goldens: load_goldens(),
            filter: String::new(),
            replay: Replay::default(),
            log: VecDeque::new(),
            last_key_at: f64::NEG_INFINITY,
            show_legend: true,
        }
    }

    /// パネルのボタンやスライダーにフォーカスが残らないようにする。
    ///
    /// egui の「キーボード入力を欲しがっているか」は文字どおり「何かにフォーカスがあるか」
    /// （`egui_wants_keyboard_input()` == `focused().is_some()`）で、エディタはそれが真の間
    /// キャンバスのキー処理を丸ごと止める。そして `Tab` は エディタでは「ホットスポット解除」
    /// だが egui ではフォーカス送りのキーでもある。つまり Tab を 1 回押すと右のパネルの
    /// ボタンにフォーカスが移り、以後キーボード操作が沈黙する。
    ///
    /// テキスト欄だけは対象外。あちらはキーボードを取るのが正しい動作で、ラベル編集ボックス
    /// （エディタ内の `TextEdit`）もこれに含まれる。
    fn release_stray_focus(ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }

    /// リプレイを進め、このフレームに流し込むべき入力を egui に積む。
    ///
    /// パネルの描画より前に呼ぶこと。合成イベントはこのフレームの `InputState` に足すので、
    /// エディタが読むのと同じ入力をライブ表示もそのまま拾える。
    /// 戻り値は「このフレームの入力はリプレイが作ったものか」。履歴の色分けに使う。
    fn tick_replay(&mut self, ctx: &egui::Context) -> bool {
        if self.replay.finished() {
            self.replay.playing = false;
            self.replay.step = false;
            return false;
        }
        let now = ctx.input(|i| i.time);
        let due = self.replay.step || (self.replay.playing && now >= self.replay.next_at);
        if !due {
            if self.replay.playing {
                ctx.request_repaint_after(std::time::Duration::from_secs_f32(
                    self.replay.interval.min(0.1),
                ));
            }
            return false;
        }

        match self.replay.emit(ctx) {
            Emitted::NeedsAnotherFrame => {
                // 整定フレームは待たせない。`step` は落とさず、次フレームで即座に続きを送る。
                self.replay.next_at = now;
                ctx.request_repaint();
                false
            }
            Emitted::Token(display) => {
                self.replay.step = false;
                self.replay.next_at = now + self.replay.interval as f64;
                if let Some(text) = display {
                    self.push_log(text, true, now);
                }
                if self.replay.playing {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f32(
                        self.replay.interval,
                    ));
                }
                true
            }
        }
    }

    /// このフレームのキー押下を履歴に積む。
    ///
    /// 合成イベントも実際の打鍵もここで同じ経路を通る（[`Self::tick_replay`] が先に積んで
    /// いるため）。`Event::Text` は無視する: 人が英数字を打つと Key と Text の両方が飛んで
    /// くるので、拾うと二重に出る。リプレイの `text:` トークンだけは Key を伴わないので、
    /// 送出側が直接履歴に積んでいる。
    fn collect_keys(&mut self, ctx: &egui::Context, synthetic: bool) {
        let now = ctx.input(|i| i.time);
        let pressed: Vec<String> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => {
                        Some(format_chord(*key, *modifiers))
                    }
                    _ => None,
                })
                .collect()
        });
        for text in pressed {
            self.push_log(text, synthetic, now);
        }
    }

    fn push_log(&mut self, text: String, synthetic: bool, now: f64) {
        self.log.push_front(LogEntry { text, synthetic });
        self.log.truncate(LOG_LEN);
        self.last_key_at = now;
    }

    fn reset_editor(&mut self) {
        self.editor = ChemStructEditor::default();
    }
}

impl App for DemoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        Self::release_stray_focus(&ctx);
        let synthetic = self.tick_replay(&ctx);
        self.collect_keys(&ctx, synthetic);

        egui::Panel::top("title").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("キーボード操作デモ");
                ui.separator();
                ui.label("キャンバスをクリックせずそのままキーを打てます。右のリプレイで 123 構造の組み立てを再生できます。");
            });
            // テキスト欄にフォーカスがある間はキャンバスのキー処理が丸ごと止まる。
            // 黙って効かなくなるより、なぜ効かないかをその場で言うほうがデモとして誠実。
            if ctx.text_edit_focused() {
                ui.colored_label(
                    egui::Color32::from_rgb(200, 120, 0),
                    "⚠ テキスト欄にフォーカス中 — キャンバスのキー操作は無効です（Esc または欄の外をクリックで解除）",
                );
            }
        });

        self.state_panel(ui);
        self.keys_panel(ui);
        self.replay_panel(ui);

        egui::CentralPanel::default().show(ui, |ui| {
            self.editor.ui(ui);
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// パネル
// ═══════════════════════════════════════════════════════════════════════════════

impl DemoApp {
    /// キーボード操作でしか動かない内部状態の可視化。
    ///
    /// ホットスポットが見えないと、矢印キーでの移動は「何も起きていない」ようにしか見えない。
    /// この 2 つがキーボード操作の要なので、常時出しておく。
    ///
    /// なお表示されるのは 1 フレーム前の値。パネルは `CentralPanel`（＝エディタ本体）より
    /// 前に描かれるため。60 Hz で 1 フレームぶんなので体感できる差ではない。
    fn state_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("state").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let hotspot = match (self.editor.hotspot_atom, self.editor.hotspot_bond) {
                    (None, None) => "なし".to_owned(),
                    (a, b) => {
                        let atom = a.map_or("なし".to_owned(), |id| format!("原子 #{id}"));
                        let bond = b.map_or("なし".to_owned(), |id| format!("結合 #{id}"));
                        format!("{atom} / {bond}")
                    }
                };
                ui.label(
                    egui::RichText::new(format!("hotspot: {hotspot}"))
                        .strong()
                        .color(egui::Color32::from_rgb(60, 140, 220)),
                );
                ui.separator();
                ui.label(format!(
                    "原子 {} 個   結合 {} 本",
                    self.editor.molecule.atoms.len(),
                    self.editor.molecule.bonds.len(),
                ));
                ui.separator();
                ui.checkbox(&mut self.show_legend, "主要キーの凡例");
            });

            if self.show_legend {
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    // 正式な割り当て表は `assets/default_config.json`、日本語の解説は
                    // `docs/SHORTCUTS.ja.md`。ここはデモを追うのに要るぶんだけ。
                    for (key, what) in [
                        ("c", "炭素を置く"),
                        ("1", "鎖を 1 個伸ばす"),
                        ("a", "ベンゼン環"),
                        ("6", "6 員環"),
                        ("O / N", "単独のヘテロ原子"),
                        ("矢印", "ホットスポットを隣の原子へ"),
                        ("Shift+矢印", "隣へ向かう結合を選ぶ"),
                        ("2 / 3", "結合次数を二重 / 三重に"),
                        ("Space", "ホットスポット解除"),
                        ("`", "ラベル編集"),
                        ("Delete", "削除"),
                        ("Ctrl+Z", "元に戻す"),
                    ] {
                        ui.label(egui::RichText::new(key).monospace().strong());
                        ui.label(egui::RichText::new(what).weak());
                        ui.separator();
                    }
                });
            }
        });
    }

    fn keys_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("keys")
            .default_size(230.0)
            .show(ui, |ui| {
                ui.heading("キー入力");
                ui.separator();

                let now = ui.ctx().input(|i| i.time);
                let fresh = now - self.last_key_at < 0.35;
                match self.log.front() {
                    Some(entry) => {
                        let color = if fresh {
                            if entry.synthetic {
                                egui::Color32::from_rgb(80, 180, 120)
                            } else {
                                egui::Color32::from_rgb(70, 150, 240)
                            }
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(
                            egui::RichText::new(&entry.text)
                                .size(30.0)
                                .monospace()
                                .strong()
                                .color(color),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("—").size(30.0).monospace().weak(),
                        );
                    }
                }
                // 直後のキーで即座に更新されるよう、強調が消えるタイミングで 1 枚描き直す。
                if fresh {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
                }

                ui.add_space(6.0);
                ui.label(egui::RichText::new("履歴（新しい順）").weak());
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, entry) in self.log.iter().enumerate() {
                        // 古いものほど薄く。何が「今」なのかを色だけで示す。
                        let fade = 1.0 - (i as f32 / LOG_LEN as f32) * 0.8;
                        let base = if entry.synthetic {
                            egui::Color32::from_rgb(60, 150, 100)
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(
                            egui::RichText::new(&entry.text)
                                .monospace()
                                .size(16.0)
                                .color(base.gamma_multiply(fade)),
                        );
                    }
                    if self.log.is_empty() {
                        ui.label(egui::RichText::new("まだキーが押されていません").weak());
                    }
                });
            });
    }

    fn replay_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("replay")
            .default_size(330.0)
            .show(ui, |ui| {
                ui.heading("ゴールデン列のリプレイ");
                ui.separator();

                let goldens = match &self.goldens {
                    Ok(g) => g,
                    Err(msg) => {
                        ui.colored_label(egui::Color32::from_rgb(200, 90, 60), msg);
                        return;
                    }
                };

                ui.horizontal(|ui| {
                    ui.label("絞り込み:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("構造名")
                            .desired_width(f32::INFINITY),
                    );
                });

                let filter = self.filter.trim().to_ascii_lowercase();
                let matches: Vec<usize> = goldens
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| filter.is_empty() || g.name.to_ascii_lowercase().contains(&filter))
                    .map(|(i, _)| i)
                    .collect();
                ui.label(
                    egui::RichText::new(format!("{} / {} 構造", matches.len(), goldens.len()))
                        .weak(),
                );

                let mut pick = None;
                egui::ScrollArea::vertical()
                    .id_salt("structure_list")
                    .max_height(230.0)
                    .show(ui, |ui| {
                        for i in matches {
                            let g = &goldens[i];
                            let selected = self.replay.selected == Some(i);
                            let label = if g.problem.is_some() {
                                egui::RichText::new(&g.name).weak().italics()
                            } else {
                                egui::RichText::new(&g.name)
                            };
                            if ui.selectable_label(selected, label).clicked() {
                                pick = Some(i);
                            }
                        }
                    });

                if let Some(i) = pick {
                    self.replay.selected = Some(i);
                    self.replay.tokens = goldens[i].tokens.clone();
                    self.replay.rewind();
                    // 選び直したら必ず空のキャンバスから。ゴールデン列は白紙前提で解かれている。
                    // `reset_editor()` を呼ばず直接代入するのは借用の都合（`goldens` が
                    // `self` を不変借用したままなので `&mut self` メソッドは呼べない）。
                    self.editor = ChemStructEditor::default();
                }

                ui.separator();

                let Some(sel) = self.replay.selected else {
                    ui.label("上の一覧から構造を選ぶと、キー列を 1 キーずつ再生できます。");
                    return;
                };
                // ここで `goldens` の借用を切ってから、以降の `&mut self` メソッドに移る。
                let (name, problem) = {
                    let g = &goldens[sel];
                    (g.name.clone(), g.problem.clone())
                };
                ui.label(egui::RichText::new(name).heading());
                if let Some(problem) = problem {
                    ui.colored_label(egui::Color32::from_rgb(200, 90, 60), problem);
                    return;
                }

                self.replay_controls(ui);
                self.sequence_view(ui);
            });
    }

    fn replay_controls(&mut self, ui: &mut egui::Ui) {
        let finished = self.replay.finished();
        let mut restart = false;
        let mut reset = false;

        ui.horizontal(|ui| {
            let play_label = if self.replay.playing { "⏸ 一時停止" } else { "▶ 再生" };
            if ui.button(play_label).clicked() {
                if finished {
                    restart = true;
                } else {
                    self.replay.playing = !self.replay.playing;
                    // 押した直後に 1 キー出したいので、次の送出時刻を過去にしておく。
                    self.replay.next_at = 0.0;
                }
            }
            if ui.add_enabled(!finished, egui::Button::new("⏭ 1 キー進む")).clicked() {
                self.replay.playing = false;
                self.replay.step = true;
            }
            if ui.button("⟲ リセット").clicked() {
                reset = true;
            }
        });

        if restart || reset {
            self.replay.rewind();
            self.reset_editor();
            self.replay.playing = restart;
        }

        ui.add(
            egui::Slider::new(&mut self.replay.interval, 0.2..=3.0)
                .text("秒 / キー")
                .fixed_decimals(2),
        );

        let total = self.replay.tokens.len();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{} / {}", self.replay.idx, total))
                    .heading()
                    .monospace(),
            );
            if finished && total > 0 {
                ui.label(egui::RichText::new("完了").color(egui::Color32::from_rgb(60, 160, 100)));
            }
        });
        ui.add(egui::ProgressBar::new(if total == 0 {
            0.0
        } else {
            self.replay.idx as f32 / total as f32
        }));
    }

    /// キー列そのものを本文として出す。消化済みは薄く、次に送るキーは強調。
    fn sequence_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("キー列").weak());
        egui::ScrollArea::vertical()
            .id_salt("sequence")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (i, token) in self.replay.tokens.iter().enumerate() {
                        let text = egui::RichText::new(token).monospace().size(15.0);
                        if i < self.replay.idx {
                            ui.label(text.weak().strikethrough());
                        } else if i == self.replay.idx {
                            ui.label(
                                text.strong()
                                    .background_color(egui::Color32::from_rgb(70, 150, 240))
                                    .color(egui::Color32::WHITE),
                            );
                        } else {
                            ui.label(text);
                        }
                    }
                });
            });
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// テスト
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// One headless frame. egui (0.37+) debug-asserts that a frame's texture uploads were applied
    /// before its output is dropped; there is no renderer here, so they are discarded.
    fn run_ui(ctx: &egui::Context, raw: egui::RawInput, f: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let mut output = ctx.run_ui(raw, f);
        output.textures_delta.clear();
        output
    }

    /// 表示とパースが同じ文法を共有していること。ここがずれると、オーバーレイに出た文字列を
    /// ゴールデン列に貼り戻せなくなる。
    #[test]
    fn format_and_parse_round_trip() {
        for spec in [
            "a", "A", "c", "O", "N", "1", "6", "!", "^", "`", "-", "+", "=", "_",
            "Enter", "Space", "Tab", "Delete", "Backspace", "Escape",
            "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
            "Shift+ArrowLeft", "Shift+ArrowDown", "Ctrl+Z", "Ctrl+Shift+Z", "Alt+ArrowRight",
        ] {
            let chord = parse_chord(spec).unwrap_or_else(|e| panic!("{spec:?}: {e}"));
            let egui::Event::Key { key, modifiers, .. } = chord.events[0] else {
                panic!("{spec:?} がキーイベントを生まなかった");
            };
            let shown = format_chord(key, modifiers);
            let reparsed = parse_chord(&shown).unwrap_or_else(|e| panic!("{shown:?}: {e}"));
            assert_eq!(
                chord.modifiers, reparsed.modifiers,
                "{spec:?} -> 表示 {shown:?} -> 再パースで修飾キーが変わった",
            );
            assert_eq!(chord.events, reparsed.events, "{spec:?} -> 表示 {shown:?} で往復しない");
        }
    }

    /// `Ctrl+Z` は Undo、`Ctrl+Shift+Z` は Redo。大文字が暗黙に持つ Shift を Ctrl が
    /// 打ち消すという規約が崩れると、この 2 つが区別できなくなる。
    #[test]
    fn ctrl_suppresses_the_shift_implied_by_an_uppercase_letter() {
        let undo = parse_chord("Ctrl+Z").unwrap().modifiers;
        assert!(undo.ctrl && !undo.shift);
        let redo = parse_chord("Ctrl+Shift+Z").unwrap().modifiers;
        assert!(redo.ctrl && redo.shift);
        assert!(parse_chord("Z").unwrap().modifiers.shift);
        assert!(!parse_chord("z").unwrap().modifiers.shift);
    }

    /// コードは押下と離上の対で送る。離上が無いと、egui が次の同じキーの押下に
    /// `repeat: true` を立て、ウィジェット側で黙って捨てられる。
    #[test]
    fn a_chord_sends_a_press_and_a_release() {
        let chord = parse_chord("c").unwrap();
        assert_eq!(chord.events.len(), 2);
        assert!(matches!(chord.events[0], egui::Event::Key { pressed: true, repeat: false, .. }));
        assert!(matches!(chord.events[1], egui::Event::Key { pressed: false, .. }));
    }

    #[test]
    fn unknown_tokens_are_reported_not_panicked() {
        assert!(parse_chord("Meta+Q").is_err());
        assert!(parse_chord("F13").is_err());
        assert!(parse_chord("").is_err());
    }

    #[test]
    fn goldens_are_parsed_into_names_and_tokens() {
        let goldens = parse_goldens(
            "benzene\tc a\nphosphate\tO ` text:P Enter\nbroken\tUNSOLVED\n",
        );
        assert_eq!(goldens.len(), 3);
        assert_eq!(goldens[0].name, "benzene");
        assert_eq!(goldens[0].tokens, ["c", "a"]);
        assert!(goldens[0].problem.is_none());
        assert_eq!(goldens[1].tokens, ["O", "`", "text:P", "Enter"]);
        assert!(goldens[1].problem.is_none(), "text: トークンは和音として読まない");
        assert!(goldens[2].problem.is_some(), "UNSOLVED は再生不可として印を付ける");
    }

    /// ゴールデン列のトークンがすべて解釈できること。ファイルはビルド生成物なので、
    /// 無いときは検査を飛ばす（このテストの目的は語彙のずれの検出であって、生成の強制ではない）。
    #[test]
    fn every_token_in_the_real_goldens_file_is_understood() {
        let Ok(goldens) = load_goldens() else {
            eprintln!("target/keyboard_goldens.txt が無いため検査を省略");
            return;
        };
        for g in &goldens {
            assert!(g.problem.is_none(), "{}: {}", g.name, g.problem.as_deref().unwrap_or(""));
            assert!(!g.tokens.is_empty(), "{}: キー列が空", g.name);
        }
        assert_eq!(goldens.len(), 123, "gen_test_structures.py の出力は 123 構造");
    }

    /// `text:` トークンは 2 フレーム使い、1 枚目は何も送らない。
    ///
    /// この 1 枚は詰め物ではない。egui がフォーカスを与えるのは `request_focus()` の
    /// **次フレームの冒頭**なので、`` ` `` の直後のフレームに流した文字は捨てられる。
    #[test]
    fn a_text_token_spends_a_settling_frame_before_typing() {
        let ctx = egui::Context::default();
        let mut replay = Replay { tokens: vec!["text:Si".to_owned()], ..Default::default() };

        let _ = run_ui(&ctx, egui::RawInput::default(), |_| {});

        let _ = run_ui(&ctx, egui::RawInput::default(), |_| {
            assert!(matches!(replay.emit(&ctx), Emitted::NeedsAnotherFrame));
        });
        assert_eq!(replay.idx, 0, "整定フレームではトークンを消化しない");
        assert!(replay.awaiting_text);

        let _ = run_ui(&ctx, egui::RawInput::default(), |_| {
            assert!(matches!(replay.emit(&ctx), Emitted::Token(Some(_))));
            let texts = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            assert_eq!(texts, ["Si"], "2 枚目で初めて文字が流れる");
        });
        assert_eq!(replay.idx, 1);
        assert!(!replay.awaiting_text);
    }

    /// アプリのフレーム進行そのままでキー列を最後まで流し、実際のエディタを組み上げる。
    ///
    /// キャンバス矩形が `Pos2::ZERO` を含まないようにしてあるのが要点。`ChemStructEditor::ui`
    /// はポインタが無いときのホバー判定に画面座標を使うので、原点を含む矩形だと「原点に
    /// カーソルが居る」ことになり、キーボードだけのつもりが実はマウス経路の検証になる。
    /// 実アプリでもキャンバスは左右のパネルぶん右にずれているので、条件は同じ。
    fn replay_to_completion(tokens: &[String]) -> ChemStructEditor {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let mut replay = Replay { tokens: tokens.to_vec(), ..Default::default() };
        let screen = egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(900.0, 700.0));

        let mut frames = 0;
        while !replay.finished() {
            frames += 1;
            assert!(frames < 10_000, "リプレイが終わらない: {tokens:?}");
            let raw = egui::RawInput {
                screen_rect: Some(screen),
                focused: true,
                ..Default::default()
            };
            let _ = run_ui(&ctx, raw, |ui| {
                // アプリと同じ順序。`release_stray_focus` を含めているのは、これがラベル編集
                // ボックスのフォーカスを巻き添えで奪わないことを `text:` を含むキー列で
                // 確かめるため（奪うと P が置かれず、静かに失敗する）。
                DemoApp::release_stray_focus(&ctx);
                replay.emit(&ctx);
                editor.ui(ui);
            });
            assert!(
                editor.hovered_atom.is_none() && editor.hovered_bond.is_none(),
                "ポインタが無いフレームでホバーが捏造された（キャンバスが原点を含んでいる）",
            );
        }
        editor
    }

    fn golden_tokens(name: &str) -> Option<Vec<String>> {
        let goldens = load_goldens().ok()?;
        goldens.into_iter().find(|g| g.name == name).map(|g| g.tokens)
    }

    /// リプレイが本当にエディタを動かすことを、ヘッドレスで通しで確認する。
    #[test]
    fn replaying_benzene_builds_a_six_membered_ring() {
        let Some(tokens) = golden_tokens("benzene") else {
            eprintln!("target/keyboard_goldens.txt が無いため検査を省略");
            return;
        };
        let editor = replay_to_completion(&tokens);
        assert_eq!(editor.molecule.atoms.len(), 6, "ベンゼンは炭素 6 個");
        assert_eq!(editor.molecule.bonds.len(), 6, "ベンゼンは結合 6 本");
    }

    /// `text:` トークンが実際のラベル編集ボックスに届くこと。
    ///
    /// リプレイで唯一まともに壊れうる箇所。`` ` `` の直後に文字を流すと egui のフォーカス
    /// 付与が間に合わず、キー列は最後まで流れたのに P が置かれない、という静かな失敗になる。
    /// `phosphate` を含む 4 つのゴールデン列がこの経路に依存している。
    #[test]
    fn replaying_phosphate_types_the_label_through_the_real_editor() {
        let Some(tokens) = golden_tokens("phosphate") else {
            eprintln!("target/keyboard_goldens.txt が無いため検査を省略");
            return;
        };
        assert!(
            tokens.iter().any(|t| t == "text:P"),
            "phosphate のキー列がラベル編集を経由しなくなった。この検査の前提が崩れている",
        );
        let editor = replay_to_completion(&tokens);
        assert!(
            editor.molecule.atoms.iter().any(|a| a.element == "P"),
            "text: トークンがラベル編集ボックスに届いていない（原子: {:?}）",
            editor.molecule.atoms.iter().map(|a| a.element.as_str()).collect::<Vec<_>>(),
        );
    }

    /// 123 本すべてが最後まで流れ、実際に構造が生えること。
    ///
    /// 目標構造との同型性までは見ない（フィクスチャ読み込みはライブラリの `#[cfg(test)]` 側に
    /// あり example からは触れない）。それはソルバ側のテストの仕事で、ここで押さえたいのは
    /// 「アプリのフレーム進行でキー列が途中から無言にならない」こと。1 フレーム 1 コードや
    /// `text:` の整定フレームを壊すと、まさにここが静かに落ちる。
    #[test]
    fn every_golden_sequence_replays_and_builds_something() {
        let Ok(goldens) = load_goldens() else {
            eprintln!("target/keyboard_goldens.txt が無いため検査を省略");
            return;
        };
        for g in &goldens {
            let editor = replay_to_completion(&g.tokens);
            assert!(
                !editor.molecule.atoms.is_empty(),
                "{}: キー列を流し切ったのに原子が 1 個も置かれていない（{}）",
                g.name,
                g.tokens.join(" "),
            );
            assert!(
                editor.label_edit.is_none(),
                "{}: ラベル編集ボックスが開いたままキー列が終わった",
                g.name,
            );
        }
    }

    /// 和音はフレーム単位の `modifiers` にも反映されること。Shift+矢印 のハンドラは
    /// イベントではなく `i.modifiers.shift` を読むので、ここが抜けると素の矢印になる。
    #[test]
    fn emitting_a_chord_sets_the_frame_level_modifiers() {
        let ctx = egui::Context::default();
        let mut replay = Replay {
            tokens: vec!["Shift+ArrowLeft".to_owned()],
            ..Default::default()
        };
        let _ = run_ui(&ctx, egui::RawInput::default(), |_| {});
        let _ = run_ui(&ctx, egui::RawInput::default(), |_| {
            assert!(matches!(replay.emit(&ctx), Emitted::Token(None)));
            assert!(ctx.input(|i| i.modifiers.shift));
            assert!(ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)));
        });
    }
}
