# chembuider-rs

*[English README →](README.md)*

Rust 製の **2D 化学構造式エディタ** を提供する [egui](https://github.com/emilk/egui) ウィジェット
ライブラリです。骨格式、立体結合、環・官能基テンプレート、2D 自動クリーンアップ、MOL2 / SMILES 出力、
そして（Windows 上では）ChemDraw 互換のクリップボード連携をサポートします。

中心となる型は [`ChemStructEditor`](src/widget/mod.rs) で、アプリ内に1つ保持して毎フレーム
`editor.ui(ui)` を呼ぶイミディエイトモードのウィジェットです。ツールバー・メニュー・ファイル
ダイアログといったアプリ固有の部分はホストアプリ側に置きます。完全な実装例は
[`examples/editor.rs`](examples/editor.rs) を参照してください。

## 機能

- **描画ツール** — Select / Bond / Eraser。ホバーのハイライトとスナップに対応。
- **原子・元素** — ヘテロ原子や電荷をワンキーで配置。炭素は省略表示し、暗黙の水素（CH₃, OH,
  NH₂ …）を下付き文字で補完。
- **リッチラベル** — 原子ラベルはベースラインを正確に揃えた本物の上付き・下付き文字で描画
  （NH₃⁺, SO₄²⁻）。カスタムラベルは上付き・下付き・太字・斜体の軽量マークアップに対応
  （[リッチテキストラベル](#リッチテキストラベル) を参照）。
- **結合** — 単結合 / 二重結合 / 三重結合（クリックで切り替え）に加え、楔（wedge）・破線楔（hash）・
  太線（bold）・破線（dashed）・波線（wavy）の立体結合。
- **テンプレート** — 環（3〜10 員環）、ジグザグ鎖、ベンゼン、組み込みの官能基**フラグメント**
  （Boc, Cbz, CF₃, CO₂Me, TMS, BPin …）。フラグメントは追加可能。
- **編集操作** — 投げ縄 / ダブルクリック選択、ドラッグで移動、`Alt`+ドラッグで回転、原子の上に
  ドラッグして結合、`Delete` で削除。
- **2D クリーンアップ** — 力学モデルによるレイアウト緩和。**バックグラウンドスレッド**で実行され
  UI を固めず、途中キャンセルも可能。
- **原子価ヒント** — 結合数が過剰な原子を赤い破線の円で警告。
- **エクスポート** — `molecule::mol2::to_mol2_string` で Tripos **MOL2** 形式を、
  `molecule::smiles::to_smiles` で正準 **SMILES**（ケクレ表記・決定論的・立体情報なし）を出力。
- **クリップボード（Windows）** — 構造を ChemDraw 互換の **CDX**・透過**画像**・**OLE** オブジェクト
  としてコピー/ペースト。PowerPoint に貼り付けると編集可能な ChemDraw 図として埋め込まれます。

## クイックスタート

同梱のエディタアプリを起動します:

```sh
cargo run --example editor
```

## 自分のアプリに組み込む

クレートと、対応する egui / eframe を依存に追加します:

```toml
[dependencies]
chembuider-rs = { git = "https://github.com/IndigoCarmine/chembuider-rs" }
eframe = "0.34"
```

ウィジェットは `egui` のみに依存します（`chembuider_rs::egui` として再エクスポートしているので、
常にバージョン整合した API を使えます）。任意の `egui::Ui` に埋め込めます:

```rust
use chembuider_rs::{egui, ChemStructEditor};

#[derive(Default)]
struct MyApp {
    editor: ChemStructEditor,
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // エディタを描画し、このフレームの入力を処理する。
        // 構造が変更されたフレームでは `true` を返す。
        let _changed = self.editor.ui(ui);
    }
}
```

`ChemStructEditor::default()` は**ファイル I/O を行いません** — 組み込みのショートカットと
フラグメント（`Config::embedded()`）を使います。ユーザーのショートカット/フラグメントを
ディスクから読み込みたい場合は `editor.config = Config::load();` を代入してください
（example アプリはこの方法を採っています）。

## 操作方法

### ツール

| ツール | 操作 |
| --- | --- |
| **Select** | クリック / 投げ縄ドラッグで選択 ・ ダブルクリックで分子全体 ・ ドラッグで移動 ・ `Alt`+ドラッグで回転 ・ `Delete` で削除 |
| **Bond** | 原子をクリックして鎖を伸ばす ・ 原子→原子へドラッグで結合 ・ 結合をクリックで単/二重/三重を切り替え |
| **Eraser** | 原子や結合をクリックして削除 |

どのツールでも、マウスホイール（またはピンチ）でカーソル位置を中心にズームし、中ボタンの
ドラッグで表示をパンします。上のツールのドラッグは左ボタンの操作なので、Select ツールで
中ボタンをドラッグしてもパンするだけで、投げ縄選択や原子の移動にはなりません。

### キーボード

キャンバスにホバー中、単キーで元素・フラグメント・テンプレートを配置できます。例:

| キー | 結果 |
| --- | --- |
| `o` / `n` / `s` / `f` | OH ・ NH₂ ・ SH ・ F |
| `O` / `N` | 単独の O ・ 単独の N 原子 |
| `2` | カルボニル（=O） |
| `a` | ベンゼン環 |
| `4`〜`8` | 4〜8 員環 |
| `z` / `Z` | ジグザグ鎖 |
| `+` / `-` | 電荷を増やす / 減らす |
| `Ctrl`+`K` | レイアウトをクリーンアップ |
| `Ctrl`+`Z` | undo（元に戻す） |
| `Ctrl`+`Y` / `Ctrl`+`Shift`+`Z` | redo（やり直し） |
| `Ctrl`+`C` / `Ctrl`+`V` | コピー / ペースト（Windows） |
| `Ctrl`+`0` | 構造全体がキャンバスに収まるように表示 |
| `Ctrl`+`1` / `2` / `3` | Select / Bond / Eraser ツールに切り替え |

テキスト欄（ラベル編集ボックスやツールバーの入力欄）にフォーカスがある間、キャンバスはツールの
キー・マウス操作と中ボタンのパン、`Ctrl`+`C` / `Ctrl`+`V`、`Ctrl`+`0` を受け付けません。
undo/redo、`Ctrl`+`K`、`Ctrl`+`1`/`2`/`3` はその間もキャンバスに効きます。クリーンアップの
実行中は、ツールのキー・マウス操作と中ボタンのパンが止まり、`Ctrl`+`K`（中止）、`Ctrl`+`0`、
ズーム、`Ctrl`+`1`/`2`/`3`、コピー / ペーストは効きます。実行中の undo はクリーンアップを取り消して
開始前の構造に戻し、redo とペーストはクリーンアップをその時点の配置で止めてから行われます。詳しい一覧は
[docs/MANUAL.ja.md](docs/MANUAL.ja.md) §2 を参照してください。

選択した原子は `Shift`+矢印（または `Ctrl`+矢印）で移動、`Alt`+矢印で回転できます。
編集可能なキーマップの全体は [`assets/default_config.json`](assets/default_config.json) にあります。

egui の既定の `zoom_with_keyboard` も `Ctrl`+`0` を「GUI の倍率を元に戻す」に使うため、`Ctrl`+`+`/`-` で
GUI の倍率を変えていた場合は同時に元に戻ります。ホストアプリは
`ctx.options_mut(|o| o.zoom_with_keyboard = false)` でこれを止められます。

## リッチテキストラベル

原子ラベルは専用のリッチテキストレンダラ（`widget::rich_text`）で描画されます。スタイルごとの
テキスト片をそれぞれ独立したガレー（galley）としてレイアウトし、ベースラインを厳密に計算して
配置するため、上付き・下付き・太字・斜体がどのズーム倍率でもきれいに描画されます。水素数は
下付き、電荷は上付きに自動でなります（NH₃⁺, SO₄²⁻）。

ラベルは ChemDraw の描画慣習に従います: ラベル全体の外接矩形ではなく**元素記号そのもの**が
原子座標にセンタリングされ、結合が右から来る場合は水素が空いている側に反転し（HO–, H₂N–）、
結合線はラベルを貫通せず境界でトリミングされます。多数の構造での描画確認には
`cargo run --example render_gallery` が使えます（`tests/fixtures/rendering/` の RDKit 生成
フィクスチャを、生座標とクリーンアップ後の 2 通りでヘッドレス描画し BMP を出力）。

カスタムラベル（原子上で `` ` `` キーを押して編集）と単体の `RichLabel` ウィジェットは、次の
軽量マークアップを解釈します。

| 記法            | 結果                                    |
|-----------------|-----------------------------------------|
| `_2` / `_{aq}`  | 下付き（1 文字 / `{}` グループ）        |
| `^+` / `^{2-}`  | 上付き（1 文字 / `{}` グループ）        |
| `**text**`      | 太字                                    |
| `//text//`      | 斜体                                    |
| `\x`            | リテラル `x`（エスケープ）              |

例えば `CO_2Et` は CO₂Et、`//t//Bu` は *t*Bu と描画されます。レンダラはホストアプリからも
使えるよう公開されています。

```rust
use chembuider_rs::{RichLabel, RichText};

ui.add(RichLabel::new("[Cu(NH_3)_4]^{2+}").size(20.0)); // egui ウィジェットとして
RichText::parse("SO_4^{2-}")                            // あるいは Painter に直接描画
    .paint(&painter, pos, egui::Align2::CENTER_CENTER, &style);
```

## 設定

`Config` はキーボードショートカット、描画スタイル（ラベルサイズ、結合の太さ、破線間隔 …）、
フラグメントライブラリを保持します。デフォルトはクレートに組み込まれています。`Config::load()`
は加えてカレントディレクトリの `chembuilder_config.json` と `fragments/` ディレクトリを読み込み、
`Config::save()` はそれらを書き戻します。新しいフラグメントは example アプリのツールバーから
直接保存できます。

## ibisOffice で ChemDraw オブジェクトを編集する

`chembuilder-office`（feature `office`、Windows）は ibisOffice の外部エディタです。
埋め込まれる構造式は ChemDraw のオブジェクト（`ChemDraw_x64.Document.6.0`、`CONTENTS` に CDX）
なので、ibisOffice で保存したファイルは PowerPoint/Word/Excel + ChemDraw でそのまま開いて編集
でき、その逆も同様です。

```sh
cargo build --release --features office --bin chembuilder-office
target/release/chembuilder-office --install-ibis-handler   # %APPDATA%\ibisOffice\embed-handlers\chembuilder.toml を書き出す
```

ibisOffice では次のことができます。

- ChemDraw オブジェクトをダブルクリックすると ChemBuilder で編集でき、編集は自動的に文書へ反映されます
- ChemDraw（または ChemBuilder）でコピーした構造式を Ctrl+V すると ChemDraw オブジェクトとして貼り付けられます
- 挿入 ▸ オブジェクト ▸ ChemBuilder で新規作成、▸ ファイルから で `.cdx` を挿入できます
- オブジェクトを Ctrl+C すると Office や ChemDraw にそのまま貼り付けられます

Office が保存した ChemDraw のプレビュー（ベクター EMF）は画面上では ChemBuilder が描き直します。
ファイル内のプレビューはオブジェクトを編集するまで元のまま保持されます。単体で
`chembuilder-office file.cdx` と起動すると CDX ファイルを編集できます。プロトコルは ibisOffice
が公開している仕様から実装しており、ibisOffice のコードは含みません。

## 対応プラットフォーム

エディタ本体はクロスプラットフォームです。CDX / 画像 / OLE の**クリップボード**機能は Windows
専用で（`#[cfg(windows)]` でガード）、他のプラットフォームではコピー/ペーストはプレーンテキストに
フォールバックします。

example は wgpu ではなく eframe の **glow**（OpenGL）バックエンドを使用します。これは現行の
Windows ツールチェインで発生する wgpu-hal の Direct3D12 ビルド不具合を回避するためです。

## プロジェクト構成

```
src/lib.rs            ライブラリのルートと公開 API
src/widget/           ChemStructEditor ウィジェット（描画 + 操作）
src/molecule/         データモデル、MOL2、SMILES、2D クリーンアップ、CDX/画像/OLE（Windows）
src/config.rs         ショートカット、スタイル、フラグメントライブラリ
src/bin/clipdump.rs   診断ツール: クリップボードの各フォーマットをダンプ（Windows）
examples/editor.rs    完全なホストアプリ
examples/render_gallery.rs  描画フィクスチャのヘッドレス BMP ギャラリー
scripts/gen_test_structures.py  tests/fixtures/rendering/ の RDKit 生成スクリプト
assets/               デフォルト設定と組み込みフラグメント JSON
```

## ステータス

1.0 以前で開発中です。公開 API はマイナーバージョン間で変更される可能性があります。

## ライセンス

Copyright © 2026 IndigoCarmine. All rights reserved. [LICENSE](LICENSE) を参照してください。
ライセンス条項は今後決定予定です。
