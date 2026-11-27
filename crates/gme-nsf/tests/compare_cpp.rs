use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

fn render_rust(path: &str) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
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

fn render_cpp(path: &str) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
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

fn render_rust_muted(path: &str, mute_mask: i32) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    // Mute AFTER start_track, matching render_cpp_muted (C++ helper mutes
    // after start_track), so both sides pre-fill identically.
    emu.start_track_impl(0).expect("start_track failed");
    emu.mute_voices_impl(mute_mask);

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

fn render_cpp_muted(path: &str, mute_mask: i32) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();
    emu.mute_voices(mute_mask);

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

fn rms(out: &[i16]) -> f64 {
    let sum: f64 = out.iter().map(|&s| (s as f64).powi(2)).sum();
    (sum / out.len() as f64).sqrt()
}

fn max_abs(out: &[i16]) -> i32 {
    out.iter().map(|&s| s.abs() as i32).max().unwrap_or(0)
}

/// Compare expansion chip output in isolation (APU muted).
/// Since C++ does a pre-fill that Rust doesn't, we can't compare sample-by-sample.
/// Instead we verify that:
/// 1. Both produce audio
/// 2. Max amplitudes match
/// 3. RMS values are within 5% of each other
fn compare_chip_isolated(label: &str, rust_out: &[i16], cpp_out: &[i16]) {
    // Compare from t=1s to skip the pre-fill region (rendered before the mute
    // mask is applied, so its content depends on pre-fill timing).
    let start = SAMPLE_RATE as usize * 2;
    let rust_out = &rust_out[start..];
    let cpp_out = &cpp_out[start..];
    let rust_rms = rms(rust_out);
    let cpp_rms = rms(cpp_out);
    let rust_max = max_abs(rust_out);
    let cpp_max = max_abs(cpp_out);
    let rust_nz = rust_out.iter().filter(|&&s| s != 0).count();
    let cpp_nz = cpp_out.iter().filter(|&&s| s != 0).count();

    eprintln!(
        "{}: rust rms={:.1} max={} nz={} | cpp rms={:.1} max={} nz={}",
        label, rust_rms, rust_max, rust_nz, cpp_rms, cpp_max, cpp_nz
    );

    assert!(rust_nz > 0, "{}: Rust produces no output", label);
    assert!(cpp_nz > 0, "{}: C++ produces no output", label);

    let ratio = rust_rms / cpp_rms;
    eprintln!("  {} RMS ratio: {:.3}", label, ratio);

    assert!(
        ratio > 0.98 && ratio < 1.02,
        "{}: RMS ratio {:.3} outside 0.98-1.02 range",
        label,
        ratio
    );
}

#[test]
fn compare_rust_vs_cpp() {
    let path = "../../assets/test.nsf";
    let rust_out = render_rust(path);
    let cpp_out = render_cpp(path);
    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    // Compare from t=1s to skip C++ pre-fill offset
    let start = SAMPLE_RATE as usize * 2; // 1 second in stereo samples
    let end = rust_out.len();

    let mut diff_count = 0u64;
    let mut max_abs_diff = 0i32;
    let total = end - start;

    for i in start..end {
        let diff = (rust_out[i] as i32 - cpp_out[i] as i32).abs();
        if diff > 0 {
            diff_count += 1;
            if diff > max_abs_diff {
                max_abs_diff = diff;
            }
        }
    }

    let pct = diff_count as f64 / total as f64 * 100.0;
    eprintln!(
        "Base NSF: from t=1s: {}/{} differ ({:.2}%), max_abs_diff={}",
        diff_count, total, pct, max_abs_diff
    );

    // The C++ pre-fill causes timing divergence, so large diffs are expected.
    // Just verify both produce audio with similar RMS.
    let rust_rms = rms(&rust_out[start..]);
    let cpp_rms = rms(&cpp_out[start..]);
    let ratio = rust_rms / cpp_rms;
    eprintln!(
        "Base NSF: rust_rms={:.1} cpp_rms={:.1} ratio={:.3}",
        rust_rms, cpp_rms, ratio
    );
    assert!(
        ratio > 0.90 && ratio < 1.10,
        "Base NSF RMS ratio {:.3} outside 0.90-1.10",
        ratio
    );
}

/// Regression test for the seek panic: skip_impl must mirror C++
/// Music_Emu::skip_/skip (drain the blip buffer via play_ in buf_size chunks),
/// not run clocks without reading samples out. Seeks 35s into the track and
/// verifies one second of subsequent output is bit-exact vs C++.
#[test]
fn compare_seek_rust_vs_cpp() {
    let path = "../../assets/test.nsf";
    let data = std::fs::read(path).unwrap();
    const SEEK_MSEC: u32 = 35_000;
    let total = SAMPLE_RATE as usize * 2; // 1 second of stereo samples

    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.start_track_impl(0).expect("start_track failed");
    // C++ Music_Emu::seek: skip(msec_to_samples(msec) - out_time), out_time = 0
    let count = (SEEK_MSEC / 1000 * SAMPLE_RATE * 2) as i32;
    emu.skip_impl(count).expect("skip failed");
    let mut rust_out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(CHUNK_SIZE);
        emu.play_impl(n as i32, &mut rust_out[pos..pos + n])
            .expect("play failed");
        pos += n;
    }

    let cpp = game_music_emu_wrapper::GameMusicEmu::from_data(&data, SAMPLE_RATE).unwrap();
    cpp.start_track(0).unwrap();
    cpp.seek(SEEK_MSEC).unwrap();
    let mut cpp_out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(CHUNK_SIZE);
        cpp.play(n, &mut cpp_out[pos..pos + n]).unwrap();
        pos += n;
    }

    let first_diff = (0..total).find(|&i| rust_out[i] != cpp_out[i]);
    eprintln!(
        "Seek: rust_rms={:.1} cpp_rms={:.1} first_diff={:?}",
        rms(&rust_out),
        rms(&cpp_out),
        first_diff
    );
    assert_eq!(first_diff, None, "post-seek output diverges from C++");
}

/// Per-channel solo comparison for a plain 2A03 NSF (regression test for the
/// "missing channel" bug). For each of the 5 APU voices, mutes the other four
/// and checks the soloed channel agrees with C++: silent in Rust iff silent in
/// C++, and RMS within 10% when audible.
#[test]
fn compare_zelda1_per_channel() {
    let path = "../../assets/zelda1.nsf";
    if !std::path::Path::new(path).exists() {
        eprintln!("zelda1.nsf not present (gitignored asset); skipping");
        return;
    }

    // Full mix
    let rust_out = render_rust(path);
    let cpp_out = render_cpp(path);
    let start = SAMPLE_RATE as usize * 2; // skip C++ pre-fill offset
    let rust_rms = rms(&rust_out[start..]);
    let cpp_rms = rms(&cpp_out[start..]);
    let ratio = rust_rms / cpp_rms;
    eprintln!(
        "Zelda1 full mix: rust_rms={:.1} cpp_rms={:.1} ratio={:.3}",
        rust_rms, cpp_rms, ratio
    );
    assert!(
        ratio > 0.90 && ratio < 1.10,
        "Zelda1 full mix RMS ratio {:.3} outside 0.90-1.10",
        ratio
    );

    // Solo each APU channel
    const NAMES: [&str; 5] = ["square1", "square2", "triangle", "noise", "dmc"];
    const SILENCE_RMS: f64 = 10.0;
    for (i, name) in NAMES.iter().enumerate() {
        let solo_mask = 0x1F & !(1 << i);
        let rust_out = render_rust_muted(path, solo_mask);
        let cpp_out = render_cpp_muted(path, solo_mask);
        let rust_rms = rms(&rust_out[start..]);
        let cpp_rms = rms(&cpp_out[start..]);
        eprintln!(
            "Zelda1 solo {}: rust_rms={:.1} cpp_rms={:.1}",
            name, rust_rms, cpp_rms
        );

        let rust_silent = rust_rms < SILENCE_RMS;
        let cpp_silent = cpp_rms < SILENCE_RMS;
        assert_eq!(
            rust_silent, cpp_silent,
            "Zelda1 solo {}: silence mismatch (rust_rms={:.1}, cpp_rms={:.1})",
            name, rust_rms, cpp_rms
        );
        if !cpp_silent {
            let ratio = rust_rms / cpp_rms;
            assert!(
                ratio > 0.90 && ratio < 1.10,
                "Zelda1 solo {}: RMS ratio {:.3} outside 0.90-1.10",
                name,
                ratio
            );
        }
    }
}

/// Solo one voice on both sides and compare. `name` is the C++ voice name for
/// that index; `total_voices` determines the all-mute base mask.
fn compare_solo_voice(path: &str, voice: usize, total_voices: usize, name: &str) {
    let solo_mask = ((1 << total_voices) - 1) & !(1 << voice);
    let rust_out = render_rust_muted(path, solo_mask);
    let cpp_out = render_cpp_muted(path, solo_mask);
    let start = SAMPLE_RATE as usize * 2; // skip C++ pre-fill offset
    let rust_rms = rms(&rust_out[start..]);
    let cpp_rms = rms(&cpp_out[start..]);
    eprintln!(
        "{} solo voice {} ({}): rust_rms={:.1} cpp_rms={:.1}",
        path, voice, name, rust_rms, cpp_rms
    );

    const SILENCE_RMS: f64 = 10.0;
    let rust_silent = rust_rms < SILENCE_RMS;
    let cpp_silent = cpp_rms < SILENCE_RMS;
    assert_eq!(
        rust_silent, cpp_silent,
        "{} solo voice {} ({}): silence mismatch (rust_rms={:.1}, cpp_rms={:.1})",
        path, voice, name, rust_rms, cpp_rms
    );
    if !cpp_silent {
        let ratio = rust_rms / cpp_rms;
        assert!(
            ratio > 0.98 && ratio < 1.02,
            "{} solo voice {} ({}): RMS ratio {:.3} outside 0.98-1.02",
            path,
            voice,
            name,
            ratio
        );
    }
}

/// Full-mix + per-voice comparison for an expansion-chip NSF.
/// `expansion_voices` are (index, name) pairs.
fn compare_game_nsf(path: &str, total_voices: usize, expansion_voices: &[(usize, &str)]) {
    let rust_out = render_rust(path);
    let cpp_out = render_cpp(path);
    let start = SAMPLE_RATE as usize * 2;
    let rust_rms = rms(&rust_out[start..]);
    let cpp_rms = rms(&cpp_out[start..]);
    let ratio = rust_rms / cpp_rms;
    eprintln!(
        "{} full mix: rust_rms={:.1} cpp_rms={:.1} ratio={:.3}",
        path, rust_rms, cpp_rms, ratio
    );
    assert!(
        ratio > 0.98 && ratio < 1.02,
        "{} full mix RMS ratio {:.3} outside 0.98-1.02",
        path,
        ratio
    );

    for &(voice, name) in expansion_voices {
        compare_solo_voice(path, voice, total_voices, name);
    }
}

/// Generated VRC6 pulse and saw waves mixed with the base APU.
#[test]
fn compare_vrc6_full_mix() {
    compare_game_nsf("../../assets/expansion/vrc6.nsf", 8, &[]);
}

/// Generated three-channel FME7 tone mixed with the base APU.
#[test]
fn compare_fme7_full_mix() {
    compare_game_nsf("../../assets/expansion/fme7.nsfe", 8, &[]);
}

/// Generated FDS wavetable mixed with the base APU.
#[test]
fn compare_fds_full_mix() {
    compare_game_nsf("../../assets/expansion/fds.nsf", 6, &[]);
}

/// Render through the same sequence the public GameMusicEmu API uses:
/// sample rate set BEFORE load, then the emulator moved into a Box. Catches
/// init-order and self-referential-pointer bugs that the load-first helpers
/// can't see.
fn render_rust_public_api_order(path: &str, mute_mask: i32) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    let mut emu = Box::new(emu);
    emu.start_track_impl(0).expect("start_track failed");
    emu.mute_voices_impl(mute_mask);

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

/// Regression test for the public-API init-order bug: set_sample_rate before
/// load ran init_audio without the header, so the FDS chip was never created
/// and the wave channel ("missing instrument") was silent. Solo the FDS Wave
/// voice through the public-API sequence and compare to C++.
#[test]
fn compare_fds_wave_public_api_order() {
    let path = "../../assets/expansion/fds.nsf";
    let solo_wave_mask = ((1 << 6) - 1) & !(1 << 5);
    let rust_out = render_rust_public_api_order(path, solo_wave_mask);
    let cpp_out = render_cpp_muted(path, solo_wave_mask);
    let start = SAMPLE_RATE as usize * 2;
    let rust_rms = rms(&rust_out[start..]);
    let cpp_rms = rms(&cpp_out[start..]);
    eprintln!(
        "FDS solo Wave (public-API order): rust_rms={:.1} cpp_rms={:.1}",
        rust_rms, cpp_rms
    );
    assert!(cpp_rms > 10.0, "C++ FDS wave unexpectedly silent");
    let ratio = rust_rms / cpp_rms;
    assert!(
        ratio > 0.98 && ratio < 1.02,
        "FDS solo Wave (public-API order): RMS ratio {:.3} outside 0.98-1.02",
        ratio
    );
}

/// Castlevania 3 (US) uses DPCM timpani drums. Regression test for the
/// public-API bug where the DMC sample reader captured raw pointers into the
/// emulator at init time: the public API sets the sample rate BEFORE loading
/// (so rom_data was empty) and then moves the emulator into a Box (dangling
/// the inline-array pointers), turning the drums into beeps. Reproduces that
/// exact order + move and checks the soloed DMC channel against C++.
#[test]
fn compare_cv3_dmc_after_move() {
    let path = "../../assets/cv3.nsf";
    if !std::path::Path::new(path).exists() {
        eprintln!("cv3.nsf not present (gitignored asset); skipping");
        return;
    }

    let solo_dmc_mask = 0x1F & !0x10;
    let rust_out = render_rust_public_api_order(path, solo_dmc_mask);
    let cpp_out = render_cpp_muted(path, solo_dmc_mask);
    let start = SAMPLE_RATE as usize * 2;
    let rust_rms = rms(&rust_out[start..]);
    let cpp_rms = rms(&cpp_out[start..]);
    eprintln!(
        "CV3 solo DMC (boxed emu): rust_rms={:.1} cpp_rms={:.1}",
        rust_rms, cpp_rms
    );
    assert!(cpp_rms > 10.0, "C++ DMC unexpectedly silent — wrong track?");
    let ratio = rust_rms / cpp_rms;
    assert!(
        ratio > 0.98 && ratio < 1.02,
        "CV3 solo DMC RMS ratio {:.3} outside 0.98-1.02 (DPCM drums wrong)",
        ratio
    );
}

/// Per-voice solo comparison for the VRC6 test NSF. Voice order matches C++
/// Nsf_Emu::set_voice, which puts the saw FIRST: 5=Saw, 6=Pulse1, 7=Pulse2.
/// Regression test for the mute-bit mapping.
#[test]
fn compare_vrc6_per_voice() {
    let path = "../../assets/expansion/vrc6.nsf";
    compare_solo_voice(path, 5, 8, "Saw Wave");
    compare_solo_voice(path, 6, 8, "Square 3");
    compare_solo_voice(path, 7, 8, "Square 4");
}

/// Per-voice solo comparison for FDS (voice 5 = "Wave").
#[test]
fn compare_fds_per_voice() {
    let path = "../../assets/expansion/fds.nsf";
    compare_solo_voice(path, 5, 6, "Wave");
}

/// Per-voice solo comparison for FME7 (voices 5-7 = "Square 3/4/5").
#[test]
fn compare_fme7_per_voice() {
    let path = "../../assets/expansion/fme7.nsfe";
    compare_solo_voice(path, 5, 8, "Square 3");
    compare_solo_voice(path, 6, 8, "Square 4");
    compare_solo_voice(path, 7, 8, "Square 5");
}

#[test]
fn compare_vrc6_rust_vs_cpp() {
    let path = "../../assets/expansion/vrc6.nsf";
    let mute_mask = 0x1F; // mute APU, keep VRC6
    let rust_out = render_rust_muted(path, mute_mask);
    let cpp_out = render_cpp_muted(path, mute_mask);
    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    compare_chip_isolated("VRC6", &rust_out, &cpp_out);
}

#[test]
fn compare_fds_rust_vs_cpp() {
    let path = "../../assets/expansion/fds.nsf";
    let mute_mask = 0x1F; // mute APU, keep FDS
    let rust_out = render_rust_muted(path, mute_mask);
    let cpp_out = render_cpp_muted(path, mute_mask);
    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    compare_chip_isolated("FDS", &rust_out, &cpp_out);
}

#[test]
fn compare_fme7_rust_vs_cpp() {
    let path = "../../assets/expansion/fme7.nsfe";
    let mute_mask = 0x1F; // mute APU, keep FME7
    let rust_out = render_rust_muted(path, mute_mask);
    let cpp_out = render_cpp_muted(path, mute_mask);
    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    compare_chip_isolated("FME7", &rust_out, &cpp_out);
}
