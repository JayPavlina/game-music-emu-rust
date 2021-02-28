//! # Game Music Emu Rust
//!
//!This crate contains Rust bindings for [Game Music Emu](https://!bitbucket.org/mpyne/game-music-emu/wiki/Home). It is pretty barebones at the moment and does not cover everything, but eventually it will have bindings for most of the functions in [gme.h](./src/gme/gme.h).
//!
//!## Conditional Compilation
//!
//!Just like the C++ version of Game Music Emu, you can choose which emulators are included by adding features to your `Cargo.toml`.
//!
//!For example, if you only want to use *Nintendo* and *Game Boy* emulators, you'd write:
//!
//!```toml
//!game-music-emu = { version = "0.1", default-features = false, features = ["gbs", "nsf"] }
//!```
//!See [Cargo.toml](Cargo.toml) for all available features. The build logic is in [build.rs](build.rs). You can call `gme::type_list()` at runtime for a list of emulators you compiled with.
//!
//!## Example Usage
//!
//!```rust
//!use game_music_emu::{EmuType, GameMusicEmu};
//!
//!let emu = GameMusicEmu::new(EmuType::Nsf, 44100);
//!emu.load_file("assets/test.nsf")?;
//!emu.start_track(0);
//! # Ok::<(), game_music_emu::GmeOrIoError>(())
//!```
//!
//! There is also an [example](examples/play_nsf.rs) that plays a song.
//!

#![deny(unused_must_use)]

pub use self::{
    native::{
        type_list, identify_header,
    },
    error::*,
    emu_type::*,
    wrapper::{GameMusicEmu},
};

mod native;
mod wrapper;
mod emu_type;
mod error;
pub mod test_utils;

