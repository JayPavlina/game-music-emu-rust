//! Differential seek tests: the public GameMusicEmu::seek must be bit-exact
//! vs the C++ library for identical call sequences.
//!
//! Regression tests for two wrapper bugs: seek used to round-trip the current
//! position through milliseconds (tell() is lossy, so seeking from a
//! non-millisecond-aligned position skipped a few samples more or fewer than
//! C++), and backward seeks were silently ignored instead of restarting the
//! track like C++ Music_Emu::seek_samples.

use game_music_emu::GameMusicEmu;
use game_music_emu_wrapper::GameMusicEmu as CppEmu;

const SAMPLE_RATE: u32 = 44100;
/// An odd number of stereo sample pairs, so the mid-play position is not
/// millisecond-aligned (12345 pairs = 279.93... ms at 44.1 kHz).
const ODD_PRELUDE: usize = 12_345 * 2;
const CHUNK: usize = 1024;

fn read_asset(name: &str) -> Vec<u8> {
    std::fs::read(format!("../../assets/{name}"))
        .or_else(|_| std::fs::read(format!("assets/{name}")))
        .unwrap_or_else(|e| panic!("{name} not found: {e}"))
}

fn play_chunked_rust(emu: &mut GameMusicEmu, out: &mut [i16]) {
    let mut pos = 0;
    while pos < out.len() {
        let n = (out.len() - pos).min(CHUNK);
        emu.play(&mut out[pos..pos + n]).unwrap();
        pos += n;
    }
}

fn play_chunked_cpp(emu: &CppEmu, out: &mut [i16]) {
    let mut pos = 0;
    while pos < out.len() {
        let n = (out.len() - pos).min(CHUNK);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
}

/// Run start_track, play `prelude_samples`, seek to `seek_msec`, then play one
/// second on both implementations and assert the post-seek audio is identical.
fn assert_seek_bit_exact(asset: &str, track: i32, prelude_samples: usize, seek_msec: i32) {
    let data = read_asset(asset);
    let total = SAMPLE_RATE as usize * 2; // 1 second of stereo samples

    let mut rust = GameMusicEmu::from_data(&data, SAMPLE_RATE).unwrap();
    rust.start_track(track).unwrap();
    let mut prelude = vec![0i16; prelude_samples];
    if prelude_samples > 0 {
        play_chunked_rust(&mut rust, &mut prelude);
    }
    rust.seek(seek_msec).unwrap();
    let mut rust_out = vec![0i16; total];
    play_chunked_rust(&mut rust, &mut rust_out);

    let cpp = CppEmu::from_data(&data, SAMPLE_RATE).unwrap();
    cpp.start_track(track as usize).unwrap();
    if prelude_samples > 0 {
        play_chunked_cpp(&cpp, &mut prelude);
    }
    cpp.seek(seek_msec as u32).unwrap();
    let mut cpp_out = vec![0i16; total];
    play_chunked_cpp(&cpp, &mut cpp_out);

    let first_diff = (0..total).find(|&i| rust_out[i] != cpp_out[i]);
    assert_eq!(
        first_diff, None,
        "{asset}: post-seek output diverges from C++ \
         (seek to {seek_msec}ms after playing {prelude_samples} samples)"
    );
}

/// Forward seek from a non-millisecond-aligned mid-play position, and a
/// backward seek (which must restart the track and skip forward, like C++).
fn seek_both_directions(asset: &str, track: i32) {
    assert_seek_bit_exact(asset, track, ODD_PRELUDE, 3_000);
    assert_seek_bit_exact(asset, track, ODD_PRELUDE, 100);
}

#[cfg(feature = "nsf")]
#[test]
fn seek_nsf() {
    seek_both_directions("test.nsf", 0);
}

#[cfg(feature = "spc")]
#[test]
fn seek_spc() {
    seek_both_directions("test.spc", 0);
}

#[cfg(feature = "gbs")]
#[test]
fn seek_gbs() {
    seek_both_directions("test.gbs", 0);
}

#[cfg(feature = "vgm")]
#[test]
fn seek_vgm() {
    seek_both_directions("test.vgm", 0);
}

#[cfg(feature = "sap")]
#[test]
fn seek_sap() {
    seek_both_directions("test.sap", 0);
}

#[cfg(feature = "kss")]
#[test]
fn seek_kss() {
    seek_both_directions("test.kss", 0);
}

#[cfg(feature = "ay")]
#[test]
fn seek_ay() {
    seek_both_directions("test.ay", 0);
}

#[cfg(feature = "gym")]
#[test]
fn seek_gym() {
    seek_both_directions("test.gym", 0);
}

/// test.hes track 0 is genuinely silent (matches C++), so use track 1.
#[cfg(feature = "hes")]
#[test]
fn seek_hes() {
    seek_both_directions("test.hes", 1);
}
