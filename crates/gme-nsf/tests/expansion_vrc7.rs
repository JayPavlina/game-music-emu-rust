//! Differential test for the VRC7 (Konami FM) expansion chip against the
//! vendored C++ reference, using a synthesized NSF (no real-world test assets
//! exercise this chip).

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

/// Build a single-track NTSC NSF: init routine at $8000, play at $8100.
fn make_nsf(chip_flags: u8, init: &[u8], play: &[u8]) -> Vec<u8> {
    assert!(init.len() <= 0x100 && play.len() <= 0x100);
    let mut h = vec![0u8; 0x80];
    h[..5].copy_from_slice(b"NESM\x1a");
    h[5] = 1; // version
    h[6] = 1; // total songs
    h[7] = 1; // starting song
    h[0x08..0x0A].copy_from_slice(&0x8000u16.to_le_bytes()); // load
    h[0x0A..0x0C].copy_from_slice(&0x8000u16.to_le_bytes()); // init
    h[0x0C..0x0E].copy_from_slice(&0x8100u16.to_le_bytes()); // play
    h[0x6E..0x70].copy_from_slice(&0x411Au16.to_le_bytes()); // NTSC speed
    h[0x7B] = chip_flags;

    let mut code = vec![0u8; 0x200];
    code[..init.len()].copy_from_slice(init);
    code[0x100..0x100 + play.len()].copy_from_slice(play);
    h.extend_from_slice(&code);
    h
}

fn render_rust(data: &[u8]) -> Vec<i16> {
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.start_track_impl(0).expect("start_track failed");

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

fn render_cpp(data: &[u8]) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_data(data, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();

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

fn assert_bit_exact(name: &str, rust_out: &[i16], cpp_out: &[i16]) {
    assert_eq!(rust_out.len(), cpp_out.len());
    let nonzero = cpp_out.iter().filter(|&&s| s != 0).count();
    assert!(
        nonzero > 1000,
        "{name}: C++ reference rendered (almost) silence ({nonzero} nonzero) — test NSF is not exercising the chip"
    );
    let mut first_diff = None;
    let mut max_diff = 0i32;
    for i in 0..rust_out.len() {
        let d = (rust_out[i] as i32 - cpp_out[i] as i32).abs();
        if d != 0 && first_diff.is_none() {
            first_diff = Some(i);
        }
        max_diff = max_diff.max(d);
    }
    if let Some(i) = first_diff {
        let start = i.saturating_sub(4);
        for j in start..(start + 12).min(rust_out.len()) {
            eprintln!(
                "  [{j}] cpp={:>6} rust={:>6} diff={}",
                cpp_out[j],
                rust_out[j],
                rust_out[j] as i32 - cpp_out[j] as i32
            );
        }
        panic!("{name}: first diff at sample {i}, max diff {max_diff}");
    }
}

#[test]
fn compare_vrc7_synth_nsf() {
    // init: table-driven register writes through the $9010 latch / $9030 data
    // ports. Programs the user patch (regs $00-$07), then keys on three
    // channels: ch0 with preset instrument 1, ch1 with the user patch (and a
    // key toggled by the play routine), ch2 with preset instrument 3 and the
    // f-number high bit set.
    #[rustfmt::skip]
    let init: &[u8] = &[
        0xA2, 0x00,             // LDX #$00
        // loop:
        0xBD, 0x15, 0x80,       // LDA $8015,X       ; register number
        0x30, 0x0D,             // BMI done ($8014)
        0x8D, 0x10, 0x90,       // STA $9010
        0xE8,                   // INX
        0xBD, 0x15, 0x80,       // LDA $8015,X       ; data byte
        0x8D, 0x30, 0x90,       // STA $9030
        0xE8,                   // INX
        0xD0, 0xEE,             // BNE loop ($8002)
        0x60,                   // RTS               ; done ($8014)
        // table ($8015): reg, data pairs, $FF terminator
        // user patch (instrument 0)
        0x00, 0x21,             // mod: ML 1
        0x01, 0x21,             // car: ML 1
        0x02, 0x1D,             // mod: KL 0, TL 29
        0x03, 0x07,             // car KL/WS, mod WS/FB 7
        0x04, 0xF0,             // mod: AR 15, DR 0
        0x05, 0xF7,             // car: AR 15, DR 7
        0x06, 0x00,             // mod: SL 0, RR 0
        0x07, 0x11,             // car: SL 1, RR 1
        // ch0: preset instrument 1, vol 0, fnum $0AC, block 2, key on
        0x30, 0x10,
        0x10, 0xAC,
        0x20, 0x14,
        // ch1: user patch, vol 2, fnum $080, block 2, key on
        0x31, 0x02,
        0x11, 0x80,
        0x21, 0x14,
        // ch2: preset instrument 3, vol 1, fnum $157, block 3, key on
        0x32, 0x31,
        0x12, 0x57,
        0x22, 0x17,
        0xFF,                   // terminator
    ];
    // play: wobble ch0's f-number low byte every frame and toggle ch1's key
    // bit every 64 frames (exercising DAMP/ATTACK/RELEASE transitions).
    #[rustfmt::skip]
    let play: &[u8] = &[
        0xE6, 0x00,             // INC $00
        0xA9, 0x10,             // LDA #$10
        0x8D, 0x10, 0x90,       // STA $9010         ; latch reg $10 (ch0 fnum low)
        0xA5, 0x00,             // LDA $00
        0x29, 0x1F,             // AND #$1F
        0x09, 0x80,             // ORA #$80
        0x8D, 0x30, 0x90,       // STA $9030         ; fnum low = $80 | (n & $1F)
        0xA9, 0x21,             // LDA #$21
        0x8D, 0x10, 0x90,       // STA $9010         ; latch reg $21 (ch1 key/block)
        0xA5, 0x00,             // LDA $00
        0x29, 0x40,             // AND #$40
        0x4A,                   // LSR
        0x4A,                   // LSR               ; $40 -> $10 (key-on bit)
        0x09, 0x04,             // ORA #$04          ; block 2
        0x8D, 0x30, 0x90,       // STA $9030
        0x60,                   // RTS
    ];
    let nsf = make_nsf(0x02, init, play);
    let rust_out = render_rust(&nsf);
    let cpp_out = render_cpp(&nsf);
    assert_bit_exact("vrc7", &rust_out, &cpp_out);
}
