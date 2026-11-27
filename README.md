# Game Music Emu

[![Crates.io](https://img.shields.io/crates/v/game-music-emu.svg)](https://crates.io/crates/game-music-emu)
[![Docs](https://docs.rs/game-music-emu/badge.svg)](https://docs.rs/game-music-emu)

A pure Rust implementation of [Game Music Emu](https://bitbucket.org/mpyne/game-music-emu/) for playing music from retro video game consoles. No C/C++ dependencies required.

## Supported Formats

| Format | System           | Feature Flag |
| ------ | ---------------- | ------------ |
| NSF, NSFE | Nintendo NES     | `nsf`        |
| SPC    | Super Nintendo   | `spc`        |
| GBS    | Game Boy         | `gbs`        |
| VGM    | Sega SMS/Genesis | `vgm`        |
| VGZ    | gzipped VGM      | `vgm`        |
| SAP    | Atari XL         | `sap`        |
| KSS    | MSX              | `kss`        |
| AY     | ZX Spectrum      | `ay`         |
| GYM    | Sega Genesis     | `gym`        |
| HES    | PC Engine        | `hes`        |

## Getting Started

Add the following to your `Cargo.toml`:

```toml
[dependencies]
game-music-emu = "0.5"
```

All formats are enabled by default. To include only specific formats:

```toml
[dependencies]
game-music-emu = { version = "0.5", default-features = false, features = ["nsf", "spc"] }
```

## Usage

```rust
use game_music_emu::GameMusicEmu;

let mut emu = GameMusicEmu::from_file("song.nsf", 44100).unwrap();
emu.start_track(0).unwrap();

let mut samples = vec![0i16; 44100 * 2]; // 1 second, stereo
emu.play(&mut samples).unwrap();
```

See the [play_nsf example](crates/game-music-emu/examples/play_nsf.rs) for a complete audio playback demo using [cpal](https://crates.io/crates/cpal).

## Playback controls and migration from 0.3

Use `GameMusicEmu::from_file` or `from_data` for format detection. For an explicit format, create a mutable player with `GameMusicEmu::new(Format::Nsf, 44100)?`, then call `load_file` or `load_data`. Reloading keeps the player's mute mask, tempo, equalizer, stereo depth, silence preference, and accuracy setting. Call `start_track` after each load.

| 0.3 control | Pure Rust API |
| --- | --- |
| List formats and identify headers | `type_list`, `identify_header`, `Format` |
| Load and clear M3U playlists | `load_m3u`, `load_m3u_data`, `clear_playlist` |
| Select tracks and read metadata | `track_count`, `start_track`, `track_info` |
| Render, seek, and read position | `play(&mut samples)`, `seek(ms)`, `tell()` |
| Detect the end, set a fade, or ignore silence | `track_ended`, `set_fade(start_ms, length_ms)`, `ignore_silence`. Use 8000 ms for the legacy fade length. |
| Inspect and mute voices | `voice_count`, `voice_name`, `mute_voice`, `mute_voices` |
| Adjust tempo and sound | `set_tempo`, `equalizer`, `set_equalizer`, `set_stereo_depth`, `enable_accuracy` |

EQ and stereo enhancement apply to AY, GBS, HES, KSS, NSF/NSFE, SAP, and VGM/VGZ. Accuracy toggles the SPC output filter. NSFE shares the NSF decoder (`nsfe` is a Cargo alias for `nsf`), and VGZ shares the VGM decoder. VGM and VGZ support YM2413 melody and rhythm, dual YM2413 chips, and YM2612 synthesis with the Nuked core. For YM2413, the first five mute controls select individual melodic channels, the sixth groups channels 6–9, and the seventh controls percussion. The legacy wrapper retains the GENS and MAME alternatives for YM2612. KSS FM synthesis remains unsupported.

Applications that need the original binding signatures can alias the wrapper in Cargo. Adjust the path to your checkout.

```toml
game-music-emu = { package = "game-music-emu-wrapper", path = "crates/game-music-emu-wrapper" }
```

## C++ Wrapper

This workspace also includes `game-music-emu-wrapper`, which provides Rust bindings for the original C++ Game Music Emu library. The C++ wrapper is 10-15% faster than the pure Rust version in most benchmarks, so it may be preferred if you need maximum performance or want the original C++ API:

```toml
[dependencies]
game-music-emu-wrapper = "0.3"
```

The C++ wrapper requires a C/C++ toolchain to build.

## License

The pure Rust crates in this workspace are licensed under **MIT only**.

The C++ wrapper crate (`game-music-emu-wrapper`) is licensed under **MIT AND LGPL-2.1-or-later** because it bundles the original C++ Game Music Emu source code, which is LGPL-2.1.

Game Music Emu is originally by Shay Green, maintained by Michael Pyne. See the [upstream license](https://bitbucket.org/mpyne/game-music-emu/src/master/license.txt) for details.
