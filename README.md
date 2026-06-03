# cornix-rmk-custom

Unofficial, custom RMK firmware for the Cornix split keyboard (nRF52840, BLE).

> ⚠️ **WORK IN PROGRESS — NOT YET HARDWARE-VERIFIED.**
> This firmware compiles and produces UF2 images, but it has **not** been flashed
> to or validated on real hardware yet. **Do not use it on your keyboard.** It may
> not work and could require recovery via the bootloader/SWD. Wait until this
> notice is removed.

*🇯🇵 日本語版は [README.ja.md](README.ja.md) を参照してください。*

[RMK](https://rmk.rs) firmware for the **Cornix** — a 50-key column-staggered
split keyboard built around the nRF52840 (Bluetooth LE).

## Features

- Wireless BLE split (left = central, right = peripheral), USB on the central.
- 4 layers, hold-tap / layer-tap thumb keys, rotary encoder on each half.
- 3 switchable BLE host profiles.
- Battery reporting from the on-board divider.
- WS2812 status LEDs (2 per half): Bluetooth profile, split-link, battery and
  charging indication.
- [Vial](https://get.vial.today) support for live remapping.

## Layout

```
Print  Q  W  E  R  T            Y  U  I  O  P  -
Caps   A  S  D  F  G            H  J  K  L  Ent !
Shift  Z  X  C  V  B            N  M  ,  .  Up ?
   Ctrl Gui Alt L1/Tab L2/Spc Sh        Sh Bspc L2 Lft Dn Rgt
```

Layers: **Base**, **Symbols** (L1), **Number** (L2), **Functions** (L3).
The layout is the power-on default; Vial edits are stored in flash and override it.

## Building

Prerequisites: a Rust toolchain (see `rust-toolchain.toml`) and
[`cargo-make`](https://github.com/sagiegurari/cargo-make):

```sh
cargo install cargo-make
```

`libclang` is required to compile the BLE controller's bindings — set
`LIBCLANG_PATH` if it is not on your system path (on Windows the build defaults
it to `C:\Program Files\LLVM\bin`).

Build both halves and generate the flashable UF2 images with a single command:

```sh
cargo make uf2
```

This compiles `central` and `peripheral` (release) and writes:

```
dist/cornix-left.uf2    # central  — left half (USB host side)
dist/cornix-right.uf2   # peripheral — right half
```

The first run installs the remaining tools automatically (`flip-link`,
`cargo-binutils` + the `llvm-tools` component, and `cargo-hex-to-uf2`). To only
compile, run `cargo make build`; the release ELFs land in
`target/thumbv7em-none-eabihf/release/{central,peripheral}` and can also be
flashed directly with `cargo run --release --bin central` via `probe-rs`.

### Build notes

- **Windows:** the BLE controller crate vendors its Nordic headers through a
  symlink that Git does not materialise on Windows. If the build reports a
  missing `sdc_soc.h`, recreate the link as a junction inside the cached
  checkout: `mklink /J <checkout>\nrf-sdc-sys\third_party <checkout>\nrf-mpsl-sys\third_party`.
  This is unnecessary on Linux/macOS (and in CI).
- If `rustc` aborts with a stack overflow while compiling the keycode tables,
  raise its stack: `set RUST_MIN_STACK=33554432` (PowerShell:
  `$env:RUST_MIN_STACK = "33554432"`).

> Bonds and the stored keymap live in the top of flash. Flashing this firmware
> over a different one will require re-pairing the halves and the host.

## License

Licensed under either of MIT or Apache-2.0 at your option.
