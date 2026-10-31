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
