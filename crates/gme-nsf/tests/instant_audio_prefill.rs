// Regression: an instant-audio NSF (no leading silence) must be bit-exact across
// the Music_Emu pre-fill handoff (C++ keeps stereo-buffer leftovers; the old
// Rust pre-fill discarded them — diverged at sample 2048).
use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

fn make_nsf(chip_flags: u8, init: &[u8], play: &[u8]) -> Vec<u8> {
    let mut h = vec![0u8; 0x80];
    h[..5].copy_from_slice(b"NESM\x1a");
    h[5] = 1;
    h[6] = 1;
    h[7] = 1;
    h[0x08..0x0A].copy_from_slice(&0x8000u16.to_le_bytes());
    h[0x0A..0x0C].copy_from_slice(&0x8000u16.to_le_bytes());
    h[0x0C..0x0E].copy_from_slice(&0x8100u16.to_le_bytes());
    h[0x6E..0x70].copy_from_slice(&0x411Au16.to_le_bytes());
    h[0x7B] = chip_flags;
    let mut code = vec![0u8; 0x200];
    code[..init.len()].copy_from_slice(init);
    code[0x100..0x100 + play.len()].copy_from_slice(play);
    h.extend_from_slice(&code);
    h
}

#[test]
fn probe_base_apu_instant_audio() {
    // init: square 1 on, instant audio
    let init: &[u8] = &[
        0xA9, 0xBF, 0x8D, 0x00, 0x40, // LDA #$BF; STA $4000
        0xA9, 0x00, 0x8D, 0x02, 0x40, // period low
        0xA9, 0x02, 0x8D, 0x03, 0x40, // period high
        0x60,
    ];
    let play: &[u8] = &[
        0xE6, 0x00, 0xA5, 0x00, 0x29, 0x07, 0x09, 0x01, 0x8D, 0x03, 0x40, 0x60,
    ];
    let nsf = make_nsf(0, init, play);

    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&nsf).unwrap();
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(44100).unwrap();
    emu.start_track_impl(0).unwrap();
    let total = 44100 * 2 * 2;
    let mut rust_out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(4096);
        emu.play_impl(n as i32, &mut rust_out[pos..pos + n])
            .unwrap();
        pos += n;
    }

    let cemu = game_music_emu_wrapper::GameMusicEmu::from_data(&nsf, 44100).unwrap();
    cemu.start_track(0).unwrap();
    let mut cpp_out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(4096);
        cemu.play(n, &mut cpp_out[pos..pos + n]).unwrap();
        pos += n;
    }

    let first_diff = rust_out.iter().zip(&cpp_out).position(|(a, b)| a != b);
    eprintln!("base APU instant-audio: first diff = {:?}", first_diff);
    assert_eq!(first_diff, None);
}
