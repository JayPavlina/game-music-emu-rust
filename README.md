# Game Music Emu Rust
[![Game Music Emu Crate](https://img.shields.io/crates/v/game-music-emu.svg)](https://crates.io/crates/game-music-emu)
[![Game Music Emu Documentation](https://docs.rs/game-music-emu/badge.svg)](https://docs.rs/game-music-emu)

This crate contains Rust bindings for [Game Music Emu](https://github.com/libgme/game-music-emu/wiki).

## Getting Started

Add the following to your `Cargo.toml`.

```
game-music-emu = "0.3"
```

## Conditional Compilation

Just like the regular version of Game Music Emu, you can choose which emulators are included by adding features to your `Cargo.toml`.

For example, if you only want to use *Nintendo* and *Game Boy* emulators, you'd write:

```
game-music-emu = { version = "0.3", default-features = false, features = ["gbs", "nsf"] }
```
See [Cargo.toml](Cargo.toml) for all available features. The build logic is in [build.rs](build.rs). You can call `gme::type_list()` at runtime for a list of emulators you compiled with.

## Usage

See the [example](examples/play_nsf.rs) for usage.

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details. 

Game Music Emu is licensed under LGPLv2.1. See its [license](src/gme/license.txt) for details.

## Acknowledgments

* Shay Green for creating Game Music Emu
* Michael Pyne for maintaining Game Music Emu
