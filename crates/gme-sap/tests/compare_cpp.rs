//! Differential test: Rust SAP emulator must be bit-exact against the
//! vendored C++ Game_Music_Emu reference.

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_sap::sap_emu::SapEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

fn render_rust(path: &str, track: i32) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = SapEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&data).expect("load failed");
    emu.start_track_impl(track).expect("start_track failed");

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(CHUNK_SIZE);
        emu.play_impl(n as i32, &mut out[pos..pos + n])
            .expect("play failed");
        pos += n;
    }
    out
}

fn render_cpp(path: &str, track: i32) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(track as usize).unwrap();

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(CHUNK_SIZE);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn assert_bit_exact(path: &str, track: i32) {
    let rust_out = render_rust(path, track);
    let cpp_out = render_cpp(path, track);

    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    let first_diff = rust_out
        .iter()
        .zip(cpp_out.iter())
        .position(|(r, c)| r != c);

    if let Some(idx) = first_diff {
        let diff_count = rust_out
            .iter()
            .zip(cpp_out.iter())
            .filter(|(r, c)| r != c)
            .count();
        let max_diff = rust_out
            .iter()
            .zip(cpp_out.iter())
            .map(|(r, c)| (*r as i32 - *c as i32).unsigned_abs())
            .max()
            .unwrap();
        let time_ms = (idx / 2) as f64 / SAMPLE_RATE as f64 * 1000.0;
        eprintln!("{path} track {track}: first diff at sample index {idx} (~{time_ms:.1}ms)");
        eprintln!(
            "  {} / {} samples differ, max diff {}",
            diff_count,
            rust_out.len(),
            max_diff
        );
        let start = idx.saturating_sub(4);
        let end = (idx + 12).min(rust_out.len());
        for i in start..end {
            let marker = if rust_out[i] != cpp_out[i] {
                ">>>"
            } else {
                "   "
            };
            eprintln!(
                "  {marker} [{i}] cpp={:>6} rust={:>6}",
                cpp_out[i], rust_out[i]
            );
        }
        panic!("{path} track {track}: Rust SAP output is not bit-exact vs C++");
    }
}

#[test]
fn compare_sap_rust_vs_cpp() {
    assert_bit_exact("../../assets/test.sap", 0);
}

#[test]
fn compare_sap_two_tracks_vs_cpp() {
    // two_tracks.sap has multiple tracks; cover the first two.
    assert_bit_exact("../../assets/two_tracks.sap", 0);
    assert_bit_exact("../../assets/two_tracks.sap", 1);
}
