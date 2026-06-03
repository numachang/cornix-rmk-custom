#!/usr/bin/env python3
"""Bake a Vial keymap export (.vil) into keyboard.toml as the firmware defaults.

Vial stores the live keymap on the device; keyboard.toml holds the *defaults*
that load when storage is empty (i.e. after a clear_storage flash). When you
tweak the keymap in the Vial GUI and want those tweaks to become the firmware's
baked-in defaults, export the keymap from Vial (File -> Export, produces a .vil)
and run this script. It rewrites the `[[layer]]` `keys`/`encoders` blocks in
keyboard.toml from the .vil, leaving everything else (matrix_map, behavior,
split, etc.) untouched.

Why a custom script: neither Vial nor RMK ships a .vil -> keyboard.toml bridge.
The .vil keymap is indexed `layout[layer][row][col]`, which maps 1:1 onto
keyboard.toml's matrix `(row,col)` coordinates, so we walk keyboard.toml's
`matrix_map` (the authoritative physical->matrix order) and look each position
up in the .vil, converting the QMK/VIA keycode to the RMK toml token.

Safety: any keycode this script can't map raises an error naming the offending
code, rather than silently emitting something wrong. Extend KEYCODES / the
wrapper handling below when that happens. Re-running is idempotent.

Usage (Python is not on PATH on this box; see CLAUDE.md):
  python tools/vil_to_keyboard_toml.py                 # rewrite keyboard.toml in place
  python tools/vil_to_keyboard_toml.py --dry-run       # print the new layers, write nothing
  python tools/vil_to_keyboard_toml.py --vil keymaps/cornix.vil --toml keyboard.toml
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# QMK/VIA keycode name -> RMK keyboard.toml token, for codes whose RMK name is
# not a trivial transform of the QMK name. Every value here is a token RMK's
# keyboard.toml parser accepts (cross-checked against the existing layers).
KEYCODES: dict[str, str] = {
    "KC_NO": "No",
    "KC_TRNS": "Trans",
    "KC_TRANSPARENT": "Trans",
    # Modifiers
    "KC_LSHIFT": "LShift", "KC_RSHIFT": "RShift",
    "KC_LCTRL": "LCtrl", "KC_RCTRL": "RCtrl",
    "KC_LALT": "LAlt", "KC_RALT": "RAlt",
    "KC_LGUI": "LGui", "KC_RGUI": "RGui",
    # Editing / navigation
    "KC_ENTER": "Enter", "KC_ESCAPE": "Escape", "KC_BSPACE": "Backspace",
    "KC_TAB": "Tab", "KC_SPACE": "Space", "KC_DELETE": "Delete",
    "KC_INSERT": "Insert", "KC_HOME": "Home", "KC_END": "End",
    "KC_PGUP": "PageUp", "KC_PGDOWN": "PageDown",
    "KC_UP": "Up", "KC_DOWN": "Down", "KC_LEFT": "Left", "KC_RIGHT": "Right",
    "KC_CAPSLOCK": "CapsLock", "KC_PSCREEN": "PrintScreen",
    # Punctuation / symbols
    "KC_MINUS": "Minus", "KC_EQUAL": "Equal", "KC_SLASH": "Slash",
    "KC_BSLASH": "Backslash", "KC_SCOLON": "Semicolon", "KC_QUOTE": "Quote",
    "KC_COMMA": "Comma", "KC_DOT": "Dot", "KC_GRAVE": "Grave",
    "KC_LBRACKET": "LeftBracket", "KC_RBRACKET": "RightBracket",
    "KC_NONUS_HASH": "NonusHash", "KC_NONUS_BSLASH": "NonusBackslash",
    # International / language (JIS)
    "KC_RO": "International1", "KC_KANA": "International2",
    "KC_JYEN": "International3", "KC_HENK": "International4",
    "KC_MHEN": "International5",
    "KC_LANG1": "Language1", "KC_LANG2": "Language2",
    # Media / mouse
    "KC_MUTE": "AudioMute", "KC_VOLU": "AudioVolUp", "KC_VOLD": "AudioVolDown",
    "KC_BTN1": "MouseBtn1", "KC_BTN2": "MouseBtn2", "KC_BTN3": "MouseBtn3",
    "KC_BTN4": "MouseBtn4", "KC_BTN5": "MouseBtn5",
}

# QMK mod token (inside _T / wrapper) -> RMK modifier name.
MODS = {"SFT": "Shift", "CTL": "Ctrl", "ALT": "Alt", "GUI": "Gui"}

_MOD_TAP = re.compile(r"^([LR])(SFT|CTL|ALT|GUI)_T\((.+)\)$")
_MOD_WRAP = re.compile(r"^([LR])(SFT|CTL|ALT|GUI)\((.+)\)$")
_LAYER_TAP = re.compile(r"^LT(\d+)\((.+)\)$")
_LAYER_MO = re.compile(r"^(MO|TO|TG|TT|DF|OSL)\((\d+)\)$")
_USER = re.compile(r"^USER(\d+)$")


def conv(kc) -> str:
    """Convert one QMK/VIA keycode (string, or -1 for an unused slot) to an
    RMK keyboard.toml token. Raises ValueError on anything unmapped."""
    if kc == -1 or kc is None:
        return "No"
    kc = kc.strip()

    # Layered / tap-hold / modded wrappers (inner keycode recurses).
    m = _LAYER_TAP.match(kc)
    if m:
        return f"LT({m.group(1)},{conv(m.group(2))})"
    m = _MOD_TAP.match(kc)
    if m:
        return f"MT({conv(m.group(3))},{m.group(1)}{MODS[m.group(2)]})"
    m = _MOD_WRAP.match(kc)
    if m:
        return f"WM({conv(m.group(3))},{m.group(1)}{MODS[m.group(2)]})"
    m = _LAYER_MO.match(kc)
    if m:
        return f"{m.group(1)}({m.group(2)})"
    m = _USER.match(kc)
    if m:
        return f"User{int(m.group(1))}"

    # Plain keycodes.
    if kc in KEYCODES:
        return KEYCODES[kc]
    if re.fullmatch(r"KC_[A-Z]", kc):  # letters
        return kc[3]
    if re.fullmatch(r"KC_[0-9]", kc):  # number row -> Kc0..Kc9
        return f"Kc{kc[3]}"
    m = re.fullmatch(r"KC_F([0-9]{1,2})", kc)  # function row
    if m:
        return f"F{m.group(1)}"

    raise ValueError(
        f"Unmapped keycode {kc!r}. Add it to KEYCODES (or the wrapper handling) "
        f"in tools/vil_to_keyboard_toml.py."
    )


def extract_matrix_map(toml: str) -> str:
    m = re.search(r'matrix_map\s*=\s*"""\n(.*?)"""', toml, re.DOTALL)
    if not m:
        sys.exit("ERROR: could not find `matrix_map = \"\"\"...\"\"\"` in keyboard.toml")
    return m.group(1)


def extract_layer_names(toml: str) -> list[str]:
    return re.findall(r'\[\[layer\]\]\s*\nname\s*=\s*"([^"]+)"', toml)


def render_keys(matrix_map: str, layer: list[list]) -> str:
    """Substitute each (row,col) token in the matrix_map text with the converted
    keycode for this layer, preserving the matrix_map's line/gap structure."""
    def repl(mo: re.Match) -> str:
        r, c = int(mo.group(1)), int(mo.group(2))
        return conv(layer[r][c])

    return re.sub(r"\((\d+),(\d+)\)", repl, matrix_map)


def render_encoders(enc_layer: list[list]) -> str:
    # Vial stores [counter_clockwise, clockwise]; RMK toml wants [cw, ccw].
    pairs = []
    for ccw, cw in enc_layer:
        pairs.append(f'["{conv(cw)}", "{conv(ccw)}"]')
    return "[" + ", ".join(pairs) + "]"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--vil", default="keymaps/cornix.vil")
    ap.add_argument("--toml", default="keyboard.toml")
    ap.add_argument("--dry-run", action="store_true", help="print the regenerated layers, write nothing")
    args = ap.parse_args()

    toml_path = Path(args.toml)
    toml = toml_path.read_text(encoding="utf-8")
    vil = json.loads(Path(args.vil).read_text(encoding="utf-8"))

    matrix_map = extract_matrix_map(toml)
    names = extract_layer_names(toml)
    layers = vil["layout"]
    encoders = vil.get("encoder_layout", [])

    if len(names) != len(layers):
        sys.exit(
            f"ERROR: keyboard.toml has {len(names)} [[layer]] blocks but the .vil "
            f"has {len(layers)} layers. Resolve the mismatch first."
        )

    blocks = []
    for i, (name, layer) in enumerate(zip(names, layers)):
        keys = render_keys(matrix_map, layer)
        block = f'[[layer]]\nname = "{name}"\nkeys = """\n{keys}"""'
        if i < len(encoders):
            block += f"\nencoders = {render_encoders(encoders[i])}"
        blocks.append(block)

    layers_section = "\n\n".join(blocks) + "\n"

    # The [[layer]] blocks are the tail of keyboard.toml; replace from the first
    # one to EOF, keeping the head ([layout]/matrix_map and everything above).
    head_match = re.search(r"\n\[\[layer\]\]", toml)
    if not head_match:
        sys.exit("ERROR: no [[layer]] block found in keyboard.toml")
    head = toml[: head_match.start()].rstrip("\n")
    new_toml = head + "\n\n" + layers_section

    if args.dry_run:
        sys.stdout.write(layers_section)
        return

    if new_toml == toml:
        print("keyboard.toml already matches the .vil; nothing to do.")
        return

    toml_path.write_text(new_toml, encoding="utf-8", newline="\n")
    print(f"Updated {toml_path} layers from {args.vil} ({len(names)} layers).")
    print("Review with `git diff keyboard.toml`, then rebuild (cargo make uf2).")


if __name__ == "__main__":
    main()
