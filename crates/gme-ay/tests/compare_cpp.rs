use gme_ay::ay_emu::AyEmu;
use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

fn render_rust(path: &str, track: i32) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = AyEmu::new();
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

fn compare_track(path: &str, track: i32) {
    let rust_out = render_rust(path, track);
    let cpp_out = render_cpp(path, track);

    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    let nonzero = cpp_out.iter().filter(|&&s| s != 0).count();
    assert!(
        nonzero > 1000,
        "C++ reference produced (near-)silence ({nonzero} nonzero samples); comparison is vacuous"
    );

    let mut first_diff = None;
    let mut diff_count = 0u64;
    let mut max_diff = 0i32;
    for i in 0..rust_out.len() {
        let d = (rust_out[i] as i32 - cpp_out[i] as i32).abs();
        if d != 0 {
            diff_count += 1;
            max_diff = max_diff.max(d);
            if first_diff.is_none() {
                first_diff = Some(i);
            }
        }
    }

    if let Some(idx) = first_diff {
        let sample_idx = idx / 2;
        let time_ms = sample_idx as f64 / SAMPLE_RATE as f64 * 1000.0;
        let ctx_start = idx.saturating_sub(4);
        let ctx_end = (idx + 8).min(rust_out.len());
        panic!(
            "Rust AY output is not bit-exact vs C++ (track {track}): first difference at sample \
             index {idx} (frame {sample_idx}, ~{time_ms:.1}ms, cpp={} rust={}), {diff_count} of \
             {} samples differ, max abs diff {max_diff}\ncontext cpp:  {:?}\ncontext rust: {:?}",
            cpp_out[idx],
            rust_out[idx],
            rust_out.len(),
            &cpp_out[ctx_start..ctx_end],
            &rust_out[ctx_start..ctx_end],
        );
    }
}

#[test]
fn compare_ay_rust_vs_cpp_track_0() {
    compare_track("../../assets/test.ay", 0);
}

/// Same comparison with an odd chunk size over a longer render, to exercise
/// frame-boundary and silence-machinery timing differently.
#[test]
fn compare_ay_rust_vs_cpp_small_chunks() {
    const SECS: u32 = 10;
    const CHUNK: usize = 1498; // even (stereo), not a divisor of the buffer size
    let path = "../../assets/test.ay";

    let data = std::fs::read(path).unwrap();
    let mut emu = AyEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.load_mem_impl(&data).unwrap();
    emu.start_track_impl(0).unwrap();

    let cemu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    cemu.start_track(0).unwrap();

    let total = (SECS * SAMPLE_RATE) as usize * 2;
    let mut rust_out = vec![0i16; total];
    let mut cpp_out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(CHUNK);
        emu.play_impl(n as i32, &mut rust_out[pos..pos + n])
            .unwrap();
        cemu.play(n, &mut cpp_out[pos..pos + n]).unwrap();
        pos += n;
    }

    let nonzero = cpp_out.iter().filter(|&&s| s != 0).count();
    assert!(nonzero > 1000, "C++ reference produced (near-)silence");
    let first_diff = (0..total).find(|&i| rust_out[i] != cpp_out[i]);
    assert_eq!(
        first_diff,
        None,
        "first diff at {:?}: cpp={:?} rust={:?}",
        first_diff,
        first_diff.map(|i| cpp_out[i]),
        first_diff.map(|i| rust_out[i]),
    );
}
