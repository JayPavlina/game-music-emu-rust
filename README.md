# Game Music Emu Rust
[![Game Music Emu Crate](https://img.shields.io/crates/v/gme.svg)](https://crates.io/crates/gme)
[![Game Music Emu Documentation](https://docs.rs/gme/badge.svg)](https://docs.rs/gme)

This crate contains Rust bindings for [Game Music Emu](https://bitbucket.org/mpyne/game-music-emu/wiki/Home). It is pretty barebones at the moment and does not cover everything, but eventually it will have bindings for most of the functions in [gme.h](./src/gme/gme.h). 

## Getting Started

Add the following to your `Cargo.toml`.

```
game-music-emu = "0.2"
```

## Conditional Compilation

Just like the regular version of Game Music Emu, you can choose which emulators are included by adding features to your `Cargo.toml`.

For example, if you only want to use *Nintendo* and *Game Boy* emulators, you'd write:

```
game-music-emu = { version = 0.1, default-features = false, features = ["gbs", "nsf"] }
```
See [Cargo.toml](Cargo.toml) for all available features. The build logic is in [build.rs](build.rs). You can call `gme::type_list()` at runtime for a list of emulators you compiled with.

##Usage

See the [example](examples/play_nsf.rs) for usage.

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details. 

Game Music Emu is licensed under LGPLv2.1. See its [license](src/gme/license.txt) for details.

## Acknowledgments

* Shay Green for creating Game Music Emu
* Michael Pyne for maintaining Game Music Emu
