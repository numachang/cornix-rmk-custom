# cornix-rmk-custom

Cornix split キーボード（nRF52840, BLE）向けの非公式・独自 RMK ファームウェア。

*🇬🇧 English version: [README.md](README.md)*

> 🔀 同じキーボード向けの ZMK 版もあります:
> [numachang/cornix-zmk-custom](https://github.com/numachang/cornix-zmk-custom)。

nRF52840（Bluetooth LE）ベースの 50 キー列スタッガード分割キーボード **Cornix** 向けの
[RMK](https://rmk.rs) ファームウェアです。左右両半とも実機で動作確認済み — 低レイテンシな
無線入力、信頼できるタップホールド親指キー、WS2812 ステータス LED に対応しています。

## 機能

- ワイヤレス BLE 分割（左 = セントラル / 右 = ペリフェラル）、USB はセントラル側。
- 4 レイヤー、ホールドタップ／レイヤータップの親指キー、各半分にロータリーエンコーダ。
- 切替可能な 3 つの BLE ホストプロファイル（1 回押しで切替）。
- 低レイテンシな分割リンク（1M PHY）。高速タイプ中でも遅延しません。
- 信頼できるタップホールド: 速いロールはタップのまま、そっと押した単独タップも入力され、
  ホールド（レイヤー／修飾）は別キーを押した瞬間に確定します。
- WS2812 ステータス LED（左右各 2 個）: Bluetooth プロファイル・分割リンク・バッテリー・
  充電状態を表示。PWM ＋ DMA 駆動なので無線を妨げません。
- オンボード分圧によるバッテリー残量レポート。
- [Vial](https://get.vial.today) によるリアルタイムのキーマップ変更に対応。

## タップホールドの挙動

親指のレイヤータップ／モッドタップは permissive-hold ＋ flow-tap プロファイルで動作し、
上流のタップホールド／morse 修正を取り込んだ RMK のリビジョンに固定してビルドしています。
実際の挙動:

- `space → 文字`（スペースを先に離す）ロールはタップのまま＝文字が潰れません。
- タップホールドキーをそっと単独で押しても、タップが出ます。
- タップホールドキーを押したまま別キーを押すと、hold timeout を待たずに即ホールド
  （レイヤー／修飾）が確定します。
- BLE ホストプロファイルの切替は 1 回押しで済みます。

調整は `keyboard.toml` の `[behavior.morse]`: `permissive_hold` ＋ `enable_flow_tap`
（prior-idle 150ms）＋ hold/gap timeout 1500ms。

## LED インジケータ

各半分が WS2812 LED を 2 個駆動します。状態変化のときに点灯し、その後消灯するので、
通常タイプ中は点きっぱなしになりません:

- **内側** — バッテリー／充電（充電中は呼吸、満充電で緑）とピアリンクのロスト。
- **外側** — 有効な Bluetooth プロファイルと接続状態（セントラル）、または分割ピアリンク
  （ペリフェラル）。

## レイアウト

```
Print  Q  W  E  R  T            Y  U  I  O  P  -
Caps   A  S  D  F  G            H  J  K  L  Ent !
Shift  Z  X  C  V  B            N  M  ,  .  Up ?
   Ctrl Gui Alt L1/Tab L2/Spc Sh        Sh Bspc L2 Lft Dn Rgt
```

レイヤー: **Base**、**Symbols**（L1）、**Number**（L2）、**Functions**（L3）。
このレイアウトは電源投入時の初期値で、Vial による編集はフラッシュに保存され上書きされます。

## ビルド

前提: Rust ツールチェイン（`rust-toolchain.toml` 参照）と
[`cargo-make`](https://github.com/sagiegurari/cargo-make):

```sh
cargo install cargo-make
```

BLE コントローラのバインディングのコンパイルに `libclang` が必要です。パスが通っていなければ
`LIBCLANG_PATH` を設定してください（Windows ではビルド時に `C:\Program Files\LLVM\bin` を自動設定します）。

左右両方をビルドし、フラッシュ用 UF2 を1コマンドで生成します:

```sh
cargo make uf2
```

`central` と `peripheral` を release ビルドし、次を出力します:

```
dist/cornix-left.uf2    # central  — 左半分（USB ホスト側）
dist/cornix-right.uf2   # peripheral — 右半分
```

初回実行時に残りのツール（`flip-link`、`cargo-binutils` ＋ `llvm-tools` コンポーネント、
`cargo-hex-to-uf2`）が自動インストールされます。コンパイルだけ行うなら `cargo make build`。
release ELF は `target/thumbv7em-none-eabihf/release/{central,peripheral}` に生成され、
`probe-rs` 経由で `cargo run --release --bin central` として直接書き込むこともできます。

### デフォルトキーマップの再生成

電源投入時のデフォルトキーマップは手書きではなく、Vial のエクスポートから生成します。
Vial GUI でキーマップを編集したら `keymaps/cornix.vil` に上書きエクスポートし、次を実行します:

```sh
python tools/vil_to_keyboard_toml.py
cargo make uf2
```

スクリプトがエクスポートから `keyboard.toml` の `[[layer]]` ブロックを書き換えます
（`--dry-run` でプレビュー可）。なお `keyboard.toml` のデフォルトは storage が空のデバイスにしか
ロードされないため、通常の書き込みでは Vial で保存済みのキーマップは上書きされません。新しい
デフォルトを反映するには一度 storage をクリア（`cargo make uf2-reset`）してください。

### ビルドに関する注意

- **Windows:** BLE コントローラのクレートは Nordic のヘッダを symlink で同梱しており、Git は
  Windows でこれを実体化しません。`sdc_soc.h` が無いというエラーが出たら、キャッシュされた
  checkout 内でジャンクションとしてリンクを作り直してください:
  `mklink /J <checkout>\nrf-sdc-sys\third_party <checkout>\nrf-mpsl-sys\third_party`。
  Linux/macOS（および CI）では不要です。
- キーコードテーブルのコンパイル中に `rustc` がスタックオーバーフローで落ちる場合は、スタックを
  拡張してください: `set RUST_MIN_STACK=33554432`（PowerShell: `$env:RUST_MIN_STACK = "33554432"`）。

> ボンド情報と保存済みキーマップはフラッシュ上部に置かれます。別のファームウェアに上書きして
> 書き込むと、左右ペアおよびホストとの再ペアリングが必要になります。

## ライセンス

MIT または Apache-2.0 のいずれかを選択して利用できます。
