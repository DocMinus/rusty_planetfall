# Rusty Z

A Z-machine interpreter (story file versions 1–3) in Rust, with a
terminal-style window that opens 80×25 characters and reflows when resized.
Built to play Infocom's *Planetfall* from its original story file.

## Run

```sh
cargo run --release                                # window, first story in stories/
cargo run --release -- path/to/story.z3            # window
cargo run --release -- --headless path/to/story.z3 # stdin/stdout
cargo run --release -- --dump path/to/story.z3     # header + object names
```

Without a file argument, the first story file in [stories/](stories/) is opened
(compiled story files only, recognised by header, so `planetfall.zip` works);
failing that, the last story you opened; failing that, the window asks for one.
You can also drop a file onto the window. `--seed N` makes the random number
generator repeatable.

Planetfall keeps its chronometer in the "moves" slot, so for its releases the
status line reads `Score: 3  Time: 4929` instead of `Moves:`.

Keys (Ctrl instead of Cmd on Windows/Linux):

| Key | Action |
|---|---|
| Cmd-O | open a story file |
| Cmd + / Cmd - / Cmd 0 | font size bigger / smaller / reset |
| Cmd-T | cycle colour theme (classic, amber, green, paper) |
| ↑ / ↓ | command history |
| Page Up / Page Down, mouse wheel | scrollback |
| any key at `[MORE]` | next page |

In-game SAVE and RESTORE use native file dialogs and write standard Quetzal
(`.qzl`) files, which other interpreters can read too. SCRIPT asks where to
write the transcript.

## macOS app

```sh
scripts/bundle-macos.sh               # target/release/bundle/Planetfall.app (arm64)
scripts/bundle-macos.sh --universal   # arm64 + x86_64; needs the x86_64-apple-darwin target
```

Story files in `stories/` are copied into the bundle (the script stops if there
are none), so the app opens straight into the game wherever it lives; drag
`Planetfall.app` to `/Applications` to install it. Finder, the menu bar and
the About box show Planetfall with Infocom's credit and the story's release
and serial number; Rusty Z is credited underneath. To bundle another game,
change `name` and `credit` at the top of the script. The bundle
is signed ad hoc, which is enough on the Mac that built it. Copied to other
Macs, it needs a Developer ID signature and notarization (or a right-click ▸
Open past Gatekeeper). The icon is drawn by `scripts/make-icon.py` into
`assets/icon.png`.

## Layout

- `crates/zmachine`: the VM, with no UI dependencies. `Vm::run()` returns an
  `Event` when the story needs input, a save, a restore, or has quit.
- `crates/app`: the `rustyz` binary (eframe/egui window, headless mode).

## Test

```sh
cargo test
scripts/fetch-testdata.sh   # optional: Czech conformance suite (needs inform6)
```

Tests that need a story file skip themselves when it is missing. The
Planetfall tests use `.planetfall_historic_source/planetfall.zip`.
