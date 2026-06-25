# chembuider-rs

*[English README →](README.md)*

Rust 製の **2D 化学構造式エディタ** を提供する [egui](https://github.com/emilk/egui) ウィジェット
ライブラリです。骨格式、立体結合、環・官能基テンプレート、2D 自動クリーンアップ、MOL2 出力、
そして（Windows 上では）ChemDraw 互換のクリップボード連携をサポートします。

中心となる型は [`ChemStructEditor`](src/widget/mod.rs) で、アプリ内に1つ保持して毎フレーム
`editor.ui(ui)` を呼ぶイミディエイトモードのウィジェットです。ツールバー・メニュー・ファイル
ダイアログといったアプリ固有の部分はホストアプリ側に置きます。完全な実装例は
[`examples/editor.rs`](examples/editor.rs) を参照してください。

## 機能

- **描画ツール** — Select / Bond / Eraser。ホバーのハイライトとスナップに対応。
- **原子・元素** — ヘテロ原子や電荷をワンキーで配置。炭素は省略表示し、暗黙の水素（CH₃, OH,
  NH₂ …）を下付き文字で補完。
- **結合** — 単結合 / 二重結合 / 三重結合（クリックで切り替え）に加え、楔（wedge）・破線楔（hash）・
  太線（bold）・破線（dashed）・波線（wavy）の立体結合。
- **テンプレート** — 環（3〜10 員環）、ジグザグ鎖、ベンゼン、組み込みの官能基**フラグメント**
  （Boc, Cbz, CF₃, CO₂Me, TMS, BPin …）。フラグメントは追加可能。
- **編集操作** — 投げ縄 / ダブルクリック選択、ドラッグで移動、`Alt`+ドラッグで回転、原子の上に
  ドラッグして結合（マージ）、`Delete` で削除。
- **2D クリーンアップ** — 力学モデルによるレイアウト緩和。**バックグラウンドスレッド**で実行され
  UI を固めず、途中キャンセルも可能。
- **原子価ヒント** — 結合数が過剰な原子を赤い破線の円で警告。
- **エクスポート** — `molecule::mol2::to_mol2_string` で Tripos **MOL2** 形式を出力。
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
| `Ctrl`+`C` / `Ctrl`+`V` | コピー / ペースト（Windows） |

選択した原子は `Shift`+矢印（または `Ctrl`+矢印）で移動、`Alt`+矢印で回転できます。
編集可能なキーマップの全体は [`assets/default_config.json`](assets/default_config.json) にあります。

## 設定

`Config` はキーボードショートカット、描画スタイル（ラベルサイズ、結合の太さ、破線間隔 …）、
フラグメントライブラリを保持します。デフォルトはクレートに組み込まれています。`Config::load()`
は加えてカレントディレクトリの `chembuilder_config.json` と `fragments/` ディレクトリを読み込み、
`Config::save()` はそれらを書き戻します。新しいフラグメントは example アプリのツールバーから
直接保存できます。

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
src/molecule/         データモデル、MOL2、2D クリーンアップ、CDX/画像/OLE（Windows）
src/config.rs         ショートカット、スタイル、フラグメントライブラリ
src/bin/clipdump.rs   診断ツール: クリップボードの各フォーマットをダンプ（Windows）
examples/editor.rs    完全なホストアプリ
assets/               デフォルト設定と組み込みフラグメント JSON
```

## ステータス

1.0 以前で開発中です。公開 API はマイナーバージョン間で変更される可能性があります。

## ライセンス

Copyright © 2026 IndigoCarmine. All rights reserved. [LICENSE](LICENSE) を参照してください。
ライセンス条項は今後決定予定です。
