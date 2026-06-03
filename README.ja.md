# cornix-rmk-custom

Cornix split キーボード（nRF52840, BLE）向けの非公式・独自 RMK ファームウェア。

> ⚠️ **開発中 — 未動作確認。**
> このファームウェアはビルドが通り UF2 を生成できますが、**実機での書き込み・動作確認はまだ行っていません**。
> **キーボードにはまだ使用しないでください。** 動作しない可能性があり、ブートローダ／SWD でのリカバリが
> 必要になる場合があります。この注意書きが消えるまでお待ちください。

*🇬🇧 English version: [README.md](README.md)*

nRF52840（Bluetooth LE）ベースの 50 キー列スタッガード分割キーボード **Cornix** 向けの
[RMK](https://rmk.rs) ファームウェアです。

## 機能

- ワイヤレス BLE 分割（左 = セントラル / 右 = ペリフェラル）、USB はセントラル側。
- 4 レイヤー、ホールドタップ／レイヤータップの親指キー、各半分にロータリーエンコーダ。
- 切替可能な 3 つの BLE ホストプロファイル。
- オンボード分圧によるバッテリー残量レポート。
- WS2812 ステータス LED（左右各 2 個）: Bluetooth プロファイル・分割リンク・バッテリー・充電状態を表示。
- [Vial](https://get.vial.today) によるリアルタイムのキーマップ変更に対応。

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
