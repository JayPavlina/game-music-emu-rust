# Test fixture provenance

## Generated audio fixtures

The following files contain original test programs, waveforms and register sequences. The generator and its outputs are distributed under the [repository MIT license](../LICENSE.md). Run `python3 tools/generate_test_music.py` to reproduce the binaries. Playback tests use these tones to exercise specific hardware behavior. [Generator](../tools/generate_test_music.py)

| Fixtures | What they exercise |
| --- | --- |
| `test.ay`, `test.kss` | Z80 initialization and PSG writes through Spectrum and MSX ports |
| `test.gbs` | Game Boy square wave and stereo routing |
| `test.hes` | HuC6280 wavetable playback, a silent first track and track-dependent pitch |
| `test.nsf`, `test.nsfe` | Two NES pulses, per-frame pitch updates, high NSF track numbers and NSFE metadata |
| `expansion/vrc6.nsf`, `expansion/fds.nsf`, `expansion/fme7.nsfe` | Expansion pulses and saw, a writable wavetable, and three Sunsoft tones alongside the NES APU |
| `test.sap`, `two_tracks.sap` | POKEY tones through SAP type C and two-track type B drivers |
| `test.spc`, `eight_voices.spc` | Eight SNES DSP voices, looping BRR samples and timed SPC700 pitch writes |
| `test.gym`, `test.vgm`, `test.vgz` | Genesis FM, PSG playback and a compressed mixed-chip log |

The existing `test.m3u` is a text playlist that selects the generated NSF tone.

## Brad Smith's NES audio tests

Three files are unmodified copies from Brad Smith's NES audio test suite at commit `99ef107f3f3ffe19f72937507ca0820a4f5c34cd`. The author grants permission in the upstream README: “These files may be freely redistributed and modified for any purpose. Credit to the original author and/or a link to the original source would be appreciated, but is not required.” [Upstream grant](https://github.com/bbbradsmith/nes-audio-tests/blob/99ef107f3f3ffe19f72937507ca0820a4f5c34cd/readme.md#L54-L55)

| Repository file | Byte-identical upstream file |
| --- | --- |
| `expansion/mmc5.nsf` | [build/db_mmc5.nsf](https://github.com/bbbradsmith/nes-audio-tests/blob/99ef107f3f3ffe19f72937507ca0820a4f5c34cd/build/db_mmc5.nsf) |
| `expansion/namco.nsf` | [build/db_n163.nsf](https://github.com/bbbradsmith/nes-audio-tests/blob/99ef107f3f3ffe19f72937507ca0820a4f5c34cd/build/db_n163.nsf) |
| `expansion/vrc7.nsf` | [build/db_vrc7.nsf](https://github.com/bbbradsmith/nes-audio-tests/blob/99ef107f3f3ffe19f72937507ca0820a4f5c34cd/build/db_vrc7.nsf) |
