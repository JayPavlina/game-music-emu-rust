//! Differential tests for the Namco 163 and MMC5 expansion chips against the
//! vendored C++ reference, using synthesized NSFs (no real-world test assets
//! exercise these chips).

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
fn compare_namco_synth_nsf() {
    // init: upload a 32-sample square wave to wave RAM, configure osc 7
    // (single active oscillator), volume 15, freq 0x1040.
    #[rustfmt::skip]
    let init: &[u8] = &[
        0xA9, 0x80,             // LDA #$80          ; wave RAM addr 0, auto-inc
        0x8D, 0x00, 0xF8,       // STA $F800
        0xA2, 0x08,             // LDX #$08
        0xA9, 0xFF,             // LDA #$FF
        0x8D, 0x00, 0x48,       // loop1: STA $4800  ; 16 samples of 15
        0xCA,                   // DEX
        0xD0, 0xFA,             // BNE loop1
        0xA2, 0x08,             // LDX #$08
        0xA9, 0x00,             // LDA #$00
        0x8D, 0x00, 0x48,       // loop2: STA $4800  ; 16 samples of 0
        0xCA,                   // DEX
        0xD0, 0xFA,             // BNE loop2
        0xA9, 0xF8,             // LDA #$F8          ; reg $78, auto-inc
        0x8D, 0x00, 0xF8,       // STA $F800
        0xA9, 0x40,             // LDA #$40
        0x8D, 0x00, 0x48,       // STA $4800         ; $78 freq low
        0xA9, 0x00,             // LDA #$00
        0x8D, 0x00, 0x48,       // STA $4800         ; $79
        0xA9, 0x10,             // LDA #$10
        0x8D, 0x00, 0x48,       // STA $4800         ; $7A freq mid
        0xA9, 0x00,             // LDA #$00
        0x8D, 0x00, 0x48,       // STA $4800         ; $7B
        0xA9, 0xE0,             // LDA #$E0          ; gme enable bits, wave size 32
        0x8D, 0x00, 0x48,       // STA $4800         ; $7C freq hi/wave size
        0xA9, 0x00,             // LDA #$00
        0x8D, 0x00, 0x48,       // STA $4800         ; $7D
        0x8D, 0x00, 0x48,       // STA $4800         ; $7E wave address
        0xA9, 0x0F,             // LDA #$0F
        0x8D, 0x00, 0x48,       // STA $4800         ; $7F vol 15, 1 osc
        0x60,                   // RTS
    ];
    // play: wobble the frequency low byte
    #[rustfmt::skip]
    let play: &[u8] = &[
        0xE6, 0x00,             // INC $00
        0xA9, 0x78,             // LDA #$78          ; reg $78, no auto-inc
        0x8D, 0x00, 0xF8,       // STA $F800
        0xA5, 0x00,             // LDA $00
        0x29, 0x3F,             // AND #$3F
        0x09, 0x40,             // ORA #$40
        0x8D, 0x00, 0x48,       // STA $4800         ; freq low
        0x60,                   // RTS
    ];
    let nsf = make_nsf(0x10, init, play);
    let rust_out = render_rust(&nsf);
    let cpp_out = render_cpp(&nsf);
    assert_bit_exact("namco", &rust_out, &cpp_out);
}

#[test]
fn compare_mmc5_synth_nsf() {
    // init: enable both squares, set duty/volume/periods, write the PCM DAC,
    // and exercise the multiplier + exram.
    #[rustfmt::skip]
    let init: &[u8] = &[
        0xA9, 0x03,             // LDA #$03
        0x8D, 0x15, 0x50,       // STA $5015         ; enable squares
        0xA9, 0xBF,             // LDA #$BF          ; duty 10, halt, const vol 15
        0x8D, 0x00, 0x50,       // STA $5000
        0xA9, 0x00,             // LDA #$00
        0x8D, 0x02, 0x50,       // STA $5002         ; sq1 period low
        0xA9, 0x02,             // LDA #$02
        0x8D, 0x03, 0x50,       // STA $5003         ; sq1 period high + len
        0xA9, 0xBF,             // LDA #$BF
        0x8D, 0x04, 0x50,       // STA $5004
        0xA9, 0x50,             // LDA #$50
        0x8D, 0x06, 0x50,       // STA $5006         ; sq2 period low
        0xA9, 0x01,             // LDA #$01
        0x8D, 0x07, 0x50,       // STA $5007         ; sq2 period high + len
        0xA9, 0x40,             // LDA #$40
        0x8D, 0x11, 0x50,       // STA $5011         ; PCM DAC
        0xA9, 0x0D,             // LDA #$0D
        0x8D, 0x05, 0x52,       // STA $5205         ; multiplier a
        0xA9, 0x0B,             // LDA #$0B
        0x8D, 0x06, 0x52,       // STA $5206         ; multiplier b
        0xAD, 0x05, 0x52,       // LDA $5205         ; product low
        0x8D, 0x00, 0x5C,       // STA $5C00         ; exram write
        0xAD, 0x00, 0x5C,       // LDA $5C00         ; exram read
        0x60,                   // RTS
    ];
    // play: ramp the PCM DAC and retrigger square 1
    #[rustfmt::skip]
    let play: &[u8] = &[
        0xE6, 0x00,             // INC $00
        0xA5, 0x00,             // LDA $00
        0x8D, 0x11, 0x50,       // STA $5011         ; PCM ramp
        0x29, 0x07,             // AND #$07
        0x09, 0x01,             // ORA #$01
        0x8D, 0x03, 0x50,       // STA $5003         ; retrigger sq1
        0x60,                   // RTS
    ];
    let nsf = make_nsf(0x08, init, play);
    let rust_out = render_rust(&nsf);
    let cpp_out = render_cpp(&nsf);
    assert_bit_exact("mmc5", &rust_out, &cpp_out);
}

// ---------------------------------------------------------------- real files

fn compare_asset(path: &str, what: &str) {
    let data = std::fs::read(format!("../../{path}"))
        .or_else(|_| std::fs::read(path))
        .unwrap_or_else(|_| panic!("{path} not found"));
    let rust_out = render_rust(&data);
    let cpp_out = render_cpp(&data);
    assert_bit_exact(what, &rust_out, &cpp_out);
}

#[test]
fn compare_namco_asset_nsf() {
    compare_asset("assets/expansion/namco.nsf", "namco asset");
}

#[test]
fn compare_mmc5_asset_nsf() {
    compare_asset("assets/expansion/mmc5.nsf", "mmc5 asset");
}

#[test]
fn compare_vrc7_asset_nsf() {
    compare_asset("assets/expansion/vrc7.nsf", "vrc7 asset");
}
