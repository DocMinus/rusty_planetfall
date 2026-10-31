# Rusty Z

## Fan port of Planetfall (by Infocom)

A Z-machine interpreter (story file versions 1–3) in Rust, with a
terminal-style window.
Built to play Infocom's _Planetfall_ from its original story file. Source: (https://github.com/the-infocom-files/planetfall)

Existing interpreters such as [Frotz](https://davidgriffith.gitlab.io/frotz/) or
[Gargoyle](https://github.com/garglk/garglk) already run Infocom story files; this is just a
one-file app, with none of the original's bugs fixed.

Other Z-machine story files (v1–3), like Zork, should run too (with some possible testing and adjustments).

## Play

1. Install [Rust](https://rustup.rs).
2. Copy the Planetfall story file (e.g. `planetfall.zip`) into [stories/](stories/).
3. Run:

```sh
cargo run --release
```

Building, the macOS app, keys and tests: [INSTALL.md](INSTALL.md).

## Resources: Maps and Walkthroughs

A collections of spread out web resources can be found in the folder: [planetfall_resources/](planetfall_resources/)

## License

Rusty Z is MIT-licensed, see [LICENSE](LICENSE). That covers the interpreter
only: _Planetfall_ is © 1983 Infocom, Inc. Its story file is not included here
and is not covered by this license.
