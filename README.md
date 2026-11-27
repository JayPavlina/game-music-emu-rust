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

See the [play_nsf example](https://github.com/JayPavlina/game-music-emu-rust/blob/master/crates/game-music-emu/examples/play_nsf.rs) for a complete audio playback demo using [cpal](https://crates.io/crates/cpal).

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

EQ and stereo enhancement apply to AY, GBS, HES, KSS, NSF/NSFE, SAP, and VGM/VGZ. Accuracy toggles the SPC output filter. NSFE shares the NSF decoder (`nsfe` is a Cargo alias for `nsf`), and VGZ shares the VGM decoder. VGM and VGZ support YM2413 melody and rhythm, dual YM2413 chips, and YM2612 synthesis with Nuked (default), GENS, or MAME. For YM2413, the first five mute controls select individual melodic channels, the sixth groups channels 6–9, and the seventh controls percussion. KSS FM synthesis remains unsupported.

### Advanced playback

| Task | Rust API |
| --- | --- |
| Render individual voices | Construct with `new_multi_channel(format, rate)` and query `out_channels()`. AY, GBS, HES, KSS, NSF/NSFE and SAP produce 16 interleaved samples per frame, one left/right pair per voice slot. GYM, SPC and VGM/VGZ produce stereo. Voices beyond eight share slots. |
| Position playback | `tell_samples` and `seek_samples` count interleaved samples aligned to a complete frame. `tell_scaled` and `seek_scaled` use musical milliseconds, accounting for playback tempo. |
| Control SPC playback | `disable_echo(bool)` switches DSP echo. `set_autoload_playback_limit(bool)` controls whether starting a track applies its tagged duration and fade. |
| Read metadata or a collection | `new_info_only(format)` loads tags without starting audio. `load_reader` accepts any `std::io::Read`. `load_tracks` combines same-format GYM, SPC, or VGM/VGZ files as tracks. `warning()` takes the latest nonfatal warning. |
| Select a Genesis FM core | Enable `ym2612_emu_gens` or `ym2612_emu_mame` alongside GYM or VGM, then call `GameMusicEmu::new_with_ym2612(format, rate, Ym2612Backend::Gens)` or select `Mame`. Both features can coexist. Linking MAME brings GPL-2.0-or-later terms. |

Applications that use the original binding signatures can keep their existing imports by aliasing the wrapper in Cargo.

```toml
game-music-emu = { package = "game-music-emu-wrapper", version = "0.3" }
```

## C++ Wrapper

This workspace also includes `game-music-emu-wrapper`, which provides Rust bindings for the original C++ Game Music Emu library. The C++ wrapper is 10-15% faster than the pure Rust version in most benchmarks, so it may be preferred if you need maximum performance or want the original C++ API:

```toml
[dependencies]
game-music-emu-wrapper = "0.3"
```

The C++ wrapper requires a C/C++ toolchain to build.

## License

The Rust facade and original project code use MIT. The shared audio engine and all nine Rust decoders contain Game_Music_Emu ports under LGPL-2.1-or-later. The Nuked YM2612 core uses LGPL-2.1-or-later, and the emu2413 core uses MIT. Commercial games can use the Rust library. If you distribute a statically linked game, review the relinking requirements. [Source notices and distribution guide](https://github.com/JayPavlina/game-music-emu-rust/blob/master/crates/game-music-emu/LICENSING.md)

The C++ wrapper uses the LGPL-covered Nuked backend by default. Its optional MAME backend makes the resulting library GPL-2.0-or-later. [Wrapper backend licenses](https://github.com/JayPavlina/game-music-emu-rust/blob/master/crates/game-music-emu-wrapper/NOTICES.md) · [Test fixture permissions](https://github.com/JayPavlina/game-music-emu-rust/blob/master/assets/LICENSE-SOURCES.md)
