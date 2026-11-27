//! Per-voice solo comparison (each DSP voice in isolation via mute masks)
//! and public-API-order regression test (set rate before load, then Box-move
//! the emulator — traps dangling-pointer/init-order bugs).

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 2;
const CHUNK_SIZE: usize = 4096;
const SPC_FILE: &str = "../../assets/test.spc";

fn render_cpp_muted(mask: i32) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(SPC_FILE, SAMPLE_RATE).unwrap();
    emu.mute_voices(mask);
    emu.start_track(0).unwrap();
    let total = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(CHUNK_SIZE);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn render_rust_muted(mask: i32) -> Vec<i16> {
    let data = std::fs::read(SPC_FILE).unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.load_mem_impl(&data).unwrap();
    emu.mute_voices_impl(mask);
    // Box AFTER init/load — forces a move, catching self-referential pointers
    let mut emu = Box::new(emu);
    emu.start_track_impl(0).unwrap();
    let total = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(CHUNK_SIZE);
        emu.play_impl(n as i32, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn assert_bit_exact(cpp: &[i16], rust: &[i16], what: &str) {
    assert_eq!(cpp.len(), rust.len());
    if let Some(d) = cpp.iter().zip(rust.iter()).position(|(a, b)| a != b) {
        let differing = cpp.iter().zip(rust.iter()).filter(|(a, b)| a != b).count();
        panic!(
            "{}: differs at sample {} (cpp={} rust={}), {} of {} samples differ",
            what,
            d,
            cpp[d],
            rust[d],
            differing,
            cpp.len()
        );
    }
}

/// Each voice solo'd must be bit-exact; this localizes any future DSP
/// regression to a specific voice.
#[test]
fn compare_per_voice_solo() {
    for v in 0..8 {
        let mask = 0xFF & !(1 << v);
        let cpp = render_cpp_muted(mask);
        let rust = render_rust_muted(mask);
        assert_bit_exact(&cpp, &rust, &format!("voice {} solo", v));
        println!("voice {} solo: bit-exact", v);
    }
}

/// Public-API call order: sample rate set BEFORE load, emulator Box-moved
/// after init. Matches how game-music-emu's public API drives emulators.
#[test]
fn public_api_order_regression() {
    let cpp = render_cpp_muted(0);
    let rust = render_rust_muted(0);
    assert_bit_exact(&cpp, &rust, "public API order render");
    println!("public API order: bit-exact ({} samples)", cpp.len());
}
