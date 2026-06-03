# cornix-rmk-custom

Unofficial, custom RMK firmware for the Cornix split keyboard (nRF52840, BLE).

*🇯🇵 日本語版は [README.ja.md](README.ja.md) を参照してください。*

> 🔀 Prefer ZMK? A ZMK build for the same keyboard is also available:
> [numachang/cornix-zmk-custom](https://github.com/numachang/cornix-zmk-custom).

[RMK](https://rmk.rs) firmware for the **Cornix** — a 50-key column-staggered
split keyboard built around the nRF52840 (Bluetooth LE). Hardware-verified on
both halves: low-latency wireless typing, reliable tap-hold thumb keys, and
WS2812 status LEDs.

## Features

- Wireless BLE split (left = central, right = peripheral), USB on the central.
- 4 layers, hold-tap / layer-tap thumb keys, a rotary encoder on each half.
- 3 switchable BLE host profiles, switched with a single press.
- Low-latency split link (1M PHY) that stays responsive under fast typing.
- Reliable tap-hold: fast rolls keep their taps, a lone soft tap registers, and
  holds (layers / modifiers) engage the instant another key is pressed.
- WS2812 status LEDs (2 per half) for Bluetooth profile, split link, battery and
  charging — driven by the PWM peripheral + DMA so they never disturb the radio.
- Battery reporting from the on-board divider.
- [Vial](https://get.vial.today) support for live remapping.

## Tap-hold behaviour

The thumb layer-tap / mod-tap keys use a permissive-hold + flow-tap profile,
built against a pinned RMK revision that carries the upstream tap-hold / morse
fixes. In practice:

- Rolling `space → letter` (releasing space first) stays a tap — no crushed words.
- A lone, soft press of a tap-hold key still emits its tap.
- Holding a tap-hold key and pressing another key engages the hold (layer /
  modifier) immediately, independent of the hold timeout.
- Switching the BLE host profile takes a single press.

Tuning lives in `[behavior.morse]` in `keyboard.toml`: `permissive_hold` +
`enable_flow_tap` (150 ms prior-idle window) with a 1500 ms hold/gap timeout.

## LED indicators

Each half drives two WS2812 LEDs. They pulse on a state change and then go dark,
so the strip is not lit during normal typing:

- **Inner** — battery / charging (breathing while charging, green when full) and
  peer-link loss.
- **Outer** — the active Bluetooth profile and connection state (central), or the
  split peer link (peripheral).

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

### Regenerating the default keymap

The power-on default keymap is generated from a Vial export, not hand-edited.
After changing the keymap in the Vial GUI, export it over `keymaps/cornix.vil`
and run:

```sh
python tools/vil_to_keyboard_toml.py
cargo make uf2
```

The script rewrites the `[[layer]]` blocks in `keyboard.toml` from the export
(use `--dry-run` to preview). Note that `keyboard.toml` defaults only load into a
device with empty storage, so a normal flash will not overwrite a keymap you have
already stored via Vial — clear storage (`cargo make uf2-reset`) once to adopt
new defaults.

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
