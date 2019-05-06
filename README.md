# Game Music Emu Rust

This crate contains Rust bindings for [Game Music Emu](https://bitbucket.org/mpyne/game-music-emu/wiki/Home). It is pretty barebones at the moment and does not cover everything, but eventually it will have bindings for most of the functions in [gme.h](./src/gme/gme.h). 

So far, it has only been verified to work with NSF, but it should work with other formats as well.

## Getting Started

Add the following to your `Cargo.toml`.

```
gme = "0.1"
```

## Conditional Compilation

Just like the regular version of Game Music Emu, you can choose which emulators are included by adding features to your `Cargo.toml`.

For example, if you only want to use *Nintendo* and *Game Boy* emulators, you'd write:

```
gme = { version = 0.1, default-features = false, features = ["gbs", "nsf"]
```
See [Cargo.toml](Cargo.toml) for all available features. The build logic is in [build.rs](build.rs).

## Usage Through Native Functions

Functions from [gme.h](./src/gme/gme.h) are exposed at the root level, and can be viewed in [native.rs](src/native.rs). Most of them require an `EmuHandle`, which holds the pointer to a `MusicEmu` instance in the C++ code.

You can get an `EmuHandle` simply like this:
```rust
let handle = gme::new_emu(gme::EmuType::Nsf, 44100);
```
You can also get a handle by loading a file. This is a convenience function that will create an instance with the file data already loaded.

```rust
let handle = gme::open_file("test.nsf", 44100).ok().unwrap();

```

Once you have the handle, you can access any of the functions with it:
```rust
let track_count = gme::track_count(&handle);
gme::start_track(&handle, 0);
```

`EmuHandles` are reference counted and the `MusicEmu` instance they reference is automatically freed when they are dropped.
## Simplified Usage

Instead of using native functions, you can use the `GameMusicEmu` struct, which provides a wrapper around the functions that take an `EmuHandle`. You can use it like this:
```rust
use gme::{EmuType, GameMusicEmu};

let emu = GameMusicEmu::new(EmuType::Nsf, 44100);
let track_count = emu.track_count();
emu.start_track(0);
```

Is it weird that there are two ways to use the API? Maybe. If there is no reason to have both, we could switch to the wrapper version only, but let's see how it goes.

## Authors

* **[Jay Pavlina](https://github.com/JayPavlina)** - *Initial work*

See also the list of [contributors](https://github.com/your/project/contributors) who participated in this project.

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details. 

Game Music Emu is licensed under LGPLv2.1. See its [license](src/gme/license.txt) for details.

## Acknowledgments

* Shay Green for creating Game Music Emu