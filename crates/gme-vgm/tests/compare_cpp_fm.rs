//! Synthetic VGM command streams exercise YM2612 and YM2413 playback.
//! The vendored C++ decoder provides the YM2612 reference and its bundled
//! emu2413 C core provides the native YM2413 sample reference.

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_vgm::vgm_emu::VgmEmu;

const SAMPLE_RATE: i32 = 44100;
const RENDER_SECS: usize = 3;

const YM2612_CLOCK: u32 = 7_670_453;
const PSG_CLOCK: u32 = 3_579_545;
const YM2413_CLOCK: u32 = 3_579_545;

// ---------------------------------------------------------------- builders

struct VgmBuilder {
    data: Vec<u8>,
}

impl VgmBuilder {
    fn new(psg_rate: u32, ym2413_rate: u32, ym2612_rate: u32) -> Self {
        let mut h = vec![0u8; 0x40];
        h[..4].copy_from_slice(b"Vgm ");
        h[0x08..0x0C].copy_from_slice(&0x150u32.to_le_bytes()); // version 1.50
        h[0x0C..0x10].copy_from_slice(&psg_rate.to_le_bytes());
        h[0x10..0x14].copy_from_slice(&ym2413_rate.to_le_bytes());
        h[0x28] = 0x09; // noise feedback (low byte)
        h[0x2A] = 16; // noise shift register width
        h[0x2C..0x30].copy_from_slice(&ym2612_rate.to_le_bytes());
        h[0x34..0x38].copy_from_slice(&0x0Cu32.to_le_bytes()); // data offset -> 0x40
        Self { data: h }
    }

    /// 0x52: YM2612 chip 0, port 0
    fn fm0(&mut self, reg: u8, val: u8) {
        self.data.extend_from_slice(&[0x52, reg, val]);
    }

    /// 0x53: YM2612 chip 0, port 1
    fn fm1(&mut self, reg: u8, val: u8) {
        self.data.extend_from_slice(&[0x53, reg, val]);
    }

    /// 0xA2: YM2612 chip 1, port 0
    fn fm0_b(&mut self, reg: u8, val: u8) {
        self.data.extend_from_slice(&[0xA2, reg, val]);
    }

    /// 0xA3: YM2612 chip 1, port 1
    fn fm1_b(&mut self, reg: u8, val: u8) {
        self.data.extend_from_slice(&[0xA3, reg, val]);
    }

    /// 0x50: PSG write
    fn psg(&mut self, val: u8) {
        self.data.extend_from_slice(&[0x50, val]);
    }

    /// 0x4F: Game Gear stereo mask
    fn gg_stereo(&mut self, val: u8) {
        self.data.extend_from_slice(&[0x4F, val]);
    }

    /// 0x62: wait one 60 Hz frame (735 samples)
    fn wait_frame(&mut self) {
        self.data.push(0x62);
    }

    fn wait_frames(&mut self, n: usize) {
        for _ in 0..n {
            self.wait_frame();
        }
    }

    /// 0x61 nn nn: wait n samples
    fn wait(&mut self, n: u16) {
        self.data.push(0x61);
        self.data.extend_from_slice(&n.to_le_bytes());
    }

    /// 0x67 0x66 0x00: uncompressed PCM data block
    fn data_block(&mut self, pcm: &[u8]) {
        self.data.extend_from_slice(&[0x67, 0x66, 0x00]);
        self.data
            .extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        self.data.extend_from_slice(pcm);
    }

    /// 0xE0: PCM seek
    fn pcm_seek(&mut self, offset: u32) {
        self.data.push(0xE0);
        self.data.extend_from_slice(&offset.to_le_bytes());
    }

    /// 0x80+n: play one PCM sample, wait n samples
    fn pcm_delay(&mut self, n: u8) {
        assert!(n <= 0x0F);
        self.data.push(0x80 | n);
    }

    fn finish(mut self) -> Vec<u8> {
        self.data.push(0x66); // end of stream
        let eof = (self.data.len() - 4) as u32;
        self.data[0x04..0x08].copy_from_slice(&eof.to_le_bytes());
        self.data
    }
}

/// Program a typical 4-op patch on the given port/channel (channel = 0..2).
fn setup_channel(b: &mut VgmBuilder, port: u8, ch: u8, algorithm: u8) {
    let w = |b: &mut VgmBuilder, reg: u8, val: u8| {
        if port == 0 {
            b.fm0(reg, val)
        } else {
            b.fm1(reg, val)
        }
    };
    // operator registers: 0x30..0x9F, slot offsets 0,4,8,12 + channel
    let dt_mul = [0x71, 0x0D, 0x33, 0x01];
    let tl = [0x23, 0x2D, 0x26, 0x00];
    let rs_ar = [0x5F, 0x99, 0x5F, 0x94];
    let am_d1r = [0x05, 0x05, 0x05, 0x07];
    let d2r = [0x02, 0x02, 0x02, 0x02];
    let d1l_rr = [0x11, 0x11, 0x11, 0xA6];
    for op in 0..4u8 {
        let off = ch + op * 4;
        w(b, 0x30 + off, dt_mul[op as usize]);
        w(b, 0x40 + off, tl[op as usize]);
        w(b, 0x50 + off, rs_ar[op as usize]);
        w(b, 0x60 + off, am_d1r[op as usize]);
        w(b, 0x70 + off, d2r[op as usize]);
        w(b, 0x80 + off, d1l_rr[op as usize]);
        w(b, 0x90 + off, 0x00);
    }
    w(b, 0xB0 + ch, (3 << 3) | algorithm); // feedback 3
    w(b, 0xB4 + ch, 0xC0); // pan L+R
}

/// Set frequency on the given port/channel.
fn set_freq(b: &mut VgmBuilder, port: u8, ch: u8, block: u8, fnum: u16) {
    let w = |b: &mut VgmBuilder, reg: u8, val: u8| {
        if port == 0 {
            b.fm0(reg, val)
        } else {
            b.fm1(reg, val)
        }
    };
    w(b, 0xA4 + ch, ((block & 7) << 3) | ((fnum >> 8) as u8 & 7));
    w(b, 0xA0 + ch, (fnum & 0xFF) as u8);
}

/// Key on/off via reg 0x28 (always port 0). channel = 0..5.
fn key(b: &mut VgmBuilder, ch: u8, ops: u8) {
    let code = if ch < 3 { ch } else { ch - 3 + 4 };
    b.fm0(0x28, (ops << 4) | code);
}

fn fm_global_init(b: &mut VgmBuilder) {
    b.fm0(0x22, 0x00); // LFO off
    b.fm0(0x27, 0x00); // ch3 normal mode
    b.fm0(0x2B, 0x00); // DAC off
}

// ---------------------------------------------------------------- render

fn render_rust(data: &[u8]) -> Vec<i16> {
    let mut emu = VgmEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(data).expect("rust load failed");
    emu.start_track_impl(0).expect("start_track failed");
    let total = SAMPLE_RATE as usize * RENDER_SECS * 2;
    let mut out = vec![0i16; total];
    emu.play_impl(total as i32, &mut out).unwrap();
    out
}

fn render_cpp(data: &[u8]) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_data(data, SAMPLE_RATE as u32)
        .expect("cpp load failed");
    emu.start_track(0).unwrap();
    let total = SAMPLE_RATE as usize * RENDER_SECS * 2;
    let mut out = vec![0i16; total];
    emu.play(total, &mut out).unwrap();
    out
}

fn assert_bit_exact(data: &[u8], what: &str) {
    let rust_out = render_rust(data);
    let cpp_out = render_cpp(data);
    assert_eq!(rust_out.len(), cpp_out.len());

    let nonzero = cpp_out.iter().filter(|&&s| s != 0).count();
    assert!(
        nonzero > 1000,
        "{what}: C++ reference produced only {nonzero} nonzero samples; test is not meaningful"
    );

    let mut diff_count = 0usize;
    let mut first_diff = None;
    for i in 0..cpp_out.len() {
        if rust_out[i] != cpp_out[i] {
            if first_diff.is_none() {
                first_diff = Some(i);
            }
            diff_count += 1;
        }
    }
    if let Some(i) = first_diff {
        panic!(
            "{what}: {diff_count} samples differ; first at index {i} (sample {}, {}): cpp={} rust={}",
            i / 2,
            if i % 2 == 0 { "L" } else { "R" },
            cpp_out[i],
            rust_out[i]
        );
    }
}

// ---------------------------------------------------------------- tests

/// YM2612 FM synthesis: two channels (one per port), key on/off and
/// frequency changes over time, SSG-EG and LFO enabled partway through.
#[test]
fn compare_fm_ym2612() {
    let mut b = VgmBuilder::new(0, 0, YM2612_CLOCK);
    fm_global_init(&mut b);

    setup_channel(&mut b, 0, 0, 2); // chip ch 0 (port 0)
    setup_channel(&mut b, 1, 1, 4); // chip ch 4 (port 1)

    key(&mut b, 0, 0x0);
    key(&mut b, 4, 0x0);
    set_freq(&mut b, 0, 0, 4, 0x269);
    set_freq(&mut b, 1, 1, 3, 0x1CD);
    key(&mut b, 0, 0xF);
    key(&mut b, 4, 0xF);

    let fnums: [u16; 6] = [0x269, 0x2B4, 0x309, 0x1CD, 0x20E, 0x26A];
    for (i, &fnum) in fnums.iter().cycle().take(12).enumerate() {
        b.wait_frames(12);
        // retrigger with a new note
        key(&mut b, 0, 0x0);
        set_freq(&mut b, 0, 0, 3 + (i as u8 & 1), fnum);
        key(&mut b, 0, 0xF);
        if i == 4 {
            b.fm0(0x22, 0x0B); // LFO on, 5.98 Hz
            b.fm0(0xB4, 0xF7); // ch0: AMS/PMS + pan
        }
        if i == 6 {
            b.fm1(0x90 + 1, 0x08); // SSG-EG on ch4 op1
        }
        b.wait(400);
        key(&mut b, 4, 0x0);
        set_freq(&mut b, 1, 1, 4, fnum.wrapping_sub(0x40) & 0x7FF);
        key(&mut b, 4, 0xF);
        b.wait_frames(3);
    }
    b.wait_frames(40);

    assert_bit_exact(&b.finish(), "FM YM2612");
}

/// DAC path: PCM data block streamed via 0x8n commands into the blip buffer,
/// with DAC enable/disable transitions, alongside an FM tone.
#[test]
fn compare_fm_dac() {
    // deterministic "sample": a coarse sine-ish wave
    let pcm: Vec<u8> = (0..6000usize)
        .map(|i| {
            let t = i as f64 * 0.07;
            (128.0 + 100.0 * t.sin() + 20.0 * (t * 3.1).sin()) as u8
        })
        .collect();

    let mut b = VgmBuilder::new(0, 0, YM2612_CLOCK);
    fm_global_init(&mut b);
    b.data_block(&pcm);

    // FM tone on channel 1 so the DAC mixes with chip output
    setup_channel(&mut b, 0, 1, 5);
    key(&mut b, 1, 0x0);
    set_freq(&mut b, 0, 1, 4, 0x284);
    key(&mut b, 1, 0xF);

    b.fm0(0x2B, 0x80); // DAC enable
    b.pcm_seek(0);

    // stream PCM at ~8.8 kHz (one sample per 5 output samples), with
    // periodic DAC off/on gaps
    let mut pos = 0usize;
    for burst in 0..40usize {
        for _ in 0..600 {
            b.pcm_delay(5);
            pos += 1;
            if pos >= pcm.len() {
                b.pcm_seek(0);
                pos = 0;
            }
        }
        if burst % 8 == 7 {
            b.fm0(0x2B, 0x00); // DAC off
            b.wait_frames(2);
            b.fm0(0x2B, 0x80); // DAC back on
            b.pcm_seek((burst * 16 % 1024) as u32);
            pos = burst * 16 % 1024;
        }
    }
    b.wait_frames(20);

    assert_bit_exact(&b.finish(), "FM DAC");
}

/// PSG + FM mix: the SN76489 renders into the mono blip buffer that
/// Dual_Resampler mixes under the FM stereo stream.
#[test]
fn compare_fm_psg_mix() {
    let mut b = VgmBuilder::new(PSG_CLOCK, 0, YM2612_CLOCK);
    fm_global_init(&mut b);

    setup_channel(&mut b, 0, 0, 7);
    key(&mut b, 0, 0x0);
    set_freq(&mut b, 0, 0, 4, 0x269);
    key(&mut b, 0, 0xF);

    // PSG tone 0: frequency latch + data, full volume
    b.psg(0x80 | 0x06); // latch tone 0 freq, low bits
    b.psg(0x1A); // data: high bits
    b.psg(0x90 | 0x02); // tone 0 volume
    // PSG noise
    b.psg(0xE0 | 0x04); // noise control
    b.psg(0xF0 | 0x08); // noise volume

    for i in 0..30usize {
        b.wait_frames(6);
        // wiggle the PSG tone
        b.psg(0x80 | ((i as u8) & 0x0F));
        b.psg(0x10 + (i as u8 % 0x30));
        if i % 5 == 0 {
            key(&mut b, 0, 0x0);
            set_freq(&mut b, 0, 0, 3, 0x2B4);
            key(&mut b, 0, 0xF);
        }
    }
    b.wait_frames(20);

    assert_bit_exact(&b.finish(), "FM+PSG mix");
}

/// Dual-chip YM2612 (header flag 0x40000000; commands 0xA2/0xA3).
#[test]
fn compare_fm_dual_chip() {
    let mut b = VgmBuilder::new(0, 0, YM2612_CLOCK | 0x4000_0000);
    fm_global_init(&mut b);
    b.fm0_b(0x22, 0x00);
    b.fm0_b(0x27, 0x00);
    b.fm0_b(0x2B, 0x00);

    // chip A: channel 0
    setup_channel(&mut b, 0, 0, 2);
    key(&mut b, 0, 0x0);
    set_freq(&mut b, 0, 0, 4, 0x269);
    key(&mut b, 0, 0xF);

    // chip B: channel 0 (via 0xA2) and channel 3 (via 0xA3)
    let regs: [(u8, [u8; 4]); 6] = [
        (0x30, [0x71, 0x0D, 0x33, 0x01]),
        (0x40, [0x23, 0x2D, 0x26, 0x00]),
        (0x50, [0x5F, 0x99, 0x5F, 0x94]),
        (0x60, [0x05, 0x05, 0x05, 0x07]),
        (0x70, [0x02, 0x02, 0x02, 0x02]),
        (0x80, [0x11, 0x11, 0x11, 0xA6]),
    ];
    for (base, vals) in regs {
        for op in 0..4u8 {
            b.fm0_b(base + op * 4, vals[op as usize]);
            b.fm1_b(base + op * 4, vals[op as usize]);
        }
    }
    b.fm0_b(0xB0, (3 << 3) | 4);
    b.fm0_b(0xB4, 0xC0);
    b.fm1_b(0xB0, (2 << 3) | 5);
    b.fm1_b(0xB4, 0xC0);
    b.fm0_b(0xA4, (3 << 3) | 0x01);
    b.fm0_b(0xA0, 0xCD);
    b.fm1_b(0xA4, (4 << 3) | 0x02);
    b.fm1_b(0xA0, 0x0E);
    b.fm0_b(0x28, 0xF0); // key on chip B ch 0
    b.fm0_b(0x28, 0xF4); // key on chip B ch 3

    for i in 0..15usize {
        b.wait_frames(12);
        key(&mut b, 0, 0x0);
        set_freq(
            &mut b,
            0,
            0,
            3 + (i as u8 & 1),
            0x269 + (i as u16 * 37) % 0x180,
        );
        key(&mut b, 0, 0xF);
        b.fm0_b(0x28, 0x04); // key off chip B ch 3
        b.fm1_b(0xA0, (0x0E + i * 11) as u8);
        b.fm0_b(0x28, 0xF4);
    }
    b.wait_frames(20);

    assert_bit_exact(&b.finish(), "dual-chip YM2612");
}

/// Hard-panned Game Gear stereo: exercises the Effects_Buffer stereo path
/// (C++ gme_new_emu wraps VGM in Effects_Buffer(1); its mono/stereo switching
/// differs from Stereo_Buffer exactly when content hard-pans).
#[test]
fn compare_gg_stereo_panning() {
    let mut b = VgmBuilder::new(PSG_CLOCK, 0, 0);
    // Two tones on squares 1 and 2
    b.psg(0x80 | 0x06); // ch0 freq low
    b.psg(0x10); // ch0 freq high
    b.psg(0x90); // ch0 volume max
    b.psg(0xA0 | 0x09); // ch1 freq low
    b.psg(0x18); // ch1 freq high
    b.psg(0xB0 | 0x02); // ch1 volume
    b.gg_stereo(0xFF); // both sides
    b.wait_frames(30);
    // ch0 hard left, ch1 hard right
    b.gg_stereo(0x01 << 5 | 0x02);
    b.wait_frames(30);
    // swap sides
    b.gg_stereo(0x02 << 5 | 0x01);
    b.wait_frames(30);
    // back to mono-ish (both center), then silence one side entirely
    b.gg_stereo(0xFF);
    b.wait_frames(20);
    b.gg_stereo(0x0F); // all right-only bits... value carries both nibbles
    b.wait_frames(60);
    let data = b.finish();
    assert_bit_exact(&data, "gg stereo panning");
}

const MELODY: &[(u8, u8)] = &[(0x30, 0x10), (0x10, 0xa0), (0x20, 0x19)];
const RHYTHM: &[(u8, u8)] = &[
    (0x16, 0x80),
    (0x17, 0xa0),
    (0x18, 0xc0),
    (0x26, 0x09),
    (0x27, 0x09),
    (0x28, 0x09),
    (0x36, 0),
    (0x37, 0),
    (0x38, 0),
    (0x0e, 0x3f),
];

fn opll_writes(b: &mut VgmBuilder, chip: u8, registers: &[(u8, u8)]) {
    for &(reg, value) in registers {
        b.data.extend_from_slice(&[chip, reg, value]);
    }
}

fn play_opll(data: &[u8], rate: i32, mute: i32, block: usize) -> Vec<i16> {
    let mut emu = Box::new(VgmEmu::new());
    emu.load_mem_impl(data).unwrap();
    emu.set_sample_rate_impl(rate).unwrap();
    emu.emu_state.ignore_silence_flag = true;
    emu.mute_voices_impl(mute);
    emu.start_track_impl(0).unwrap();
    let mut audio = vec![0; rate as usize];
    for samples in audio.chunks_mut(block) {
        emu.play_impl(samples.len() as i32, samples).unwrap();
    }
    audio
}

#[test]
fn opll_timed_writes_dual_chips_and_rhythm() {
    let mut b = VgmBuilder::new(4_000_000, 0x4000_0000 | YM2413_CLOCK, 0);
    b.wait(4410);
    opll_writes(&mut b, 0x51, MELODY);
    b.wait(4410);
    opll_writes(&mut b, 0xa1, RHYTHM);
    b.wait(44100);
    let data = b.finish();
    for rate in [32000, 44100, 48000] {
        let all = play_opll(&data, rate, 0, 2048);
        let melody = play_opll(&data, rate, 1 << 6, 346);
        let drums = play_opll(&data, rate, 1, 512);
        assert_eq!(all, play_opll(&data, rate, 0, 66));
        let onset = |s: &[i16]| {
            s.as_chunks::<2>()
                .0
                .iter()
                .position(|p| p[0].unsigned_abs() > 8)
                .unwrap() as i32
        };
        // Allow 5 ms for the chip envelope and the output filter.
        assert!((onset(&melody) - rate / 10).abs() < rate / 200);
        assert!((onset(&drums) - rate / 5).abs() < rate / 200);
        assert!(all.iter().zip(&melody).any(|(a, m)| a != m));
        assert!(play_opll(&data, rate, -1, 2048).iter().all(|&s| s == 0));
    }
}

#[test]
fn opll_mixes_with_ym2612_and_psg() {
    let mut b = VgmBuilder::new(PSG_CLOCK, YM2413_CLOCK, YM2612_CLOCK);
    setup_channel(&mut b, 0, 1, 7);
    set_freq(&mut b, 0, 1, 3, 0x269);
    key(&mut b, 1, 0xf);
    b.psg(0x86);
    b.psg(0x10);
    b.psg(0x93);
    opll_writes(&mut b, 0x51, MELODY);
    b.wait(44100);
    let data = b.finish();
    let solos = [0, 1, 7].map(|voice| play_opll(&data, 44100, !(1 << voice), 512));
    for solo in &solos {
        assert!(solo.iter().any(|s| s.unsigned_abs() > 16));
    }
    assert!(solos[0] != solos[1] && solos[0] != solos[2]);
    let all = play_opll(&data, 44100, 0, 512);
    for solo in solos {
        assert!(all != solo);
    }
}

#[test]
fn opll_legacy_clock_field_and_vrc7_presets() {
    let mut b = VgmBuilder::new(PSG_CLOCK, YM2413_CLOCK, 0);
    opll_writes(&mut b, 0x51, MELODY);
    b.wait(44100);
    let mut data = b.finish();
    let modern = play_opll(&data, 44100, 0, 512);
    data[8..12].copy_from_slice(&0x101u32.to_le_bytes());
    assert_eq!(play_opll(&data, 44100, 0, 512), modern);
    data[8..12].copy_from_slice(&0x171u32.to_le_bytes());
    data[0x10..0x14].copy_from_slice(&(0x8000_0000 | YM2413_CLOCK).to_le_bytes());
    let vrc7 = play_opll(&data, 44100, 0, 512);
    assert!(vrc7 != modern && vrc7.iter().any(|s| s.unsigned_abs() > 16));
}

#[test]
fn opll_synthesis_matches_the_vendored_c_core() {
    use gme_core::emu2413::Opll;
    use std::ffi::c_void;

    unsafe extern "C" {
        fn OPLL_new(clock: u32, rate: u32) -> *mut c_void;
        fn OPLL_delete(chip: *mut c_void);
        fn OPLL_writeReg(chip: *mut c_void, reg: u32, value: u8);
        fn OPLL_calcStereo(chip: *mut c_void, output: *mut i32);
    }
    // An integer native rate gives exactly one hardware tick per C reference sample.
    let rate = 48000;
    let clock = 72 * rate;
    let mut rust = Opll::new(clock, rate);
    let reference = unsafe { OPLL_new(clock, rate) };
    assert!(!reference.is_null());
    for registers in [MELODY, RHYTHM, &[(0x0e, 0), (0x20, 0x09)]] {
        for &(reg, value) in registers {
            rust.write_reg(u32::from(reg), value);
            unsafe { OPLL_writeReg(reference, u32::from(reg), value) };
        }
        for sample in 0..4096 {
            let mix: i32 = rust.clock_channels().iter().copied().map(i32::from).sum();
            let mut expected = [0; 2];
            unsafe { OPLL_calcStereo(reference, expected.as_mut_ptr()) };
            assert_eq!(
                mix, expected[0],
                "native PCM sample {sample} after {registers:?}"
            );
        }
    }
    unsafe { OPLL_delete(reference) };
}
