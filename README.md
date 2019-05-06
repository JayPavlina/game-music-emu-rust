# Game Music Emu Rust

This project contains Rust bindings for [Game Music Emu](https://bitbucket.org/mpyne/game-music-emu/wiki/Home). It is pretty barebones at the moment and does not cover everything, but eventually it will have bindings for all functions in [gme.h](./src/gme/gme.h). 

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

## Usage

Functions from [gme.h](./src/gme/gme.h) are exposed at the root level, and can be viewed in [native.rs](src/native.rs). Most of them require an `EmuHandle`, which holds the pointer to a gme instance.

You can get an `EmuHandle` simply like this:
```rust
let handle = gme::new_emu(gme::EmuType::Nsf, sample_rate);
```
You can also get one by loading a file.

```rust
use std::io::Read;
use std::fs::File;
    
let mut file = File::open("test.nsf").unwrap();
let mut buffer = Vec::new();
file.read_to_end(&mut buffer).unwrap();
let handle = gme::open_data(&buffer, 44100).ok().unwrap();

```

Once you have the handle, you can access any of the functions with it:
```rust
let track_count = gme::get_track_count(&handle);
gme::start_track(&handle, 0);
```

`EmuHandle`s are reference counted and the gme instance they reference is automatically freed when they are dropped. 

Note that the [experimental module](src/experimental.rs) is not yet ready for use and will change. This mainly includes the `GameMusicEmu` struct, which will allow simplified access to gme's functions.

## Authors

* **[Jay Pavlina](https://github.com/JayPavlina)** - *Initial work*

See also the list of [contributors](https://github.com/your/project/contributors) who participated in this project.

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details. 

Game Music Emu is licensed under LGPLv2.1. See its [license](src/gme/license.txt) for details.

## Acknowledgments

* Shay Green for creating Game Music Emu