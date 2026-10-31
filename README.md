# Rusty Z

## Fan port of Planetfall (by Infocom)

A Z-machine interpreter (story file versions 1–3) in Rust, with a
terminal-style window.
Built to play Infocom's _Planetfall_ from its original story file. Source: (https://github.com/the-infocom-files/planetfall)

With minor changes, other zil based adventures like Zork could be plugged in, if so desired.

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
