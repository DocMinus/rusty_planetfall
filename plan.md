# Planetfall conversion from Zil to Rust for modern Operating Systems

A faithful recreation (fan-port) of the Infocom game Planetfall converted to modern RUST.

Modern design patterns are to be used, no overengineering.

Kown bugs of the original should be fixed.

## Starting points, following folders:

- `.planetfall_historic_source/` full historic source, with source code as well as compiled code. a README.md is available. Mainly as general reference.
- `.planetfall_source/` most important source files; important README.md with some resource links.
- `.web_information`: selected information from the web, walkthroughs and maps.

## Target

A standalone app that runs the "terminal" style of this app in a window. Opens in original character number width, but can be resized by user with OS windows features.
First tartget: MacOS (silicone) then Windows (10/11) then Linux (Weyland?).

## Rust

if rust is not installed, install it. Mac has brew available if that is easiest.

## Direction (agreed 2026-10-31)

Instead of re-writing the game's logic in Rust, we build **Rusty Z**: a native Rust
Z-machine interpreter that runs the original Planetfall story file (Release 39,
`.planetfall_historic_source/planetfall.zip` is that V3 story file) unchanged.
Consequence: bugs in the game's own logic stay as shipped; we own interpreter
correctness. Detailed plan: `~/.claude/plans/read-the-plan-md-and-graceful-glacier.md`.

## Status

| Milestone | State |
|---|---|
| 1. Toolchain (rustup via brew, Rust 1.99), workspace, header/memory/text, `--dump` | done |
| 2. Decoder, all V1–V3 opcodes, objects, dictionary, headless mode | done |
| 3. Tests: 25 unit tests, Planetfall opening/save/restore/restart, Czech V3 (349 passed, 0 failed) | done |
| 4. Window: status line, reflowing scrollback, inline input, history, [MORE], scrollback | done, under manual test |
| 5. Quetzal save/restore, restart, transcript, file dialogs, settings, drag & drop | done, under manual test |
| 6. macOS `.app` bundle | done (`scripts/bundle-macos.sh`, ad hoc signed) |
| 7. Windows, then Linux (Wayland + X11) builds | next |
