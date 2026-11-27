use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_kss::kss_emu::KssEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

fn render_rust(path: &str) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = KssEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&data).expect("load failed");
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

#[test]
fn compare_kss_rust_vs_cpp() {
    let path = "../../assets/test.kss";
    let rust_out = render_rust(path);
    let cpp_out = render_cpp(path);

    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    let nonzero = cpp_out.iter().filter(|&&s| s != 0).count();
    assert!(
        nonzero > 0,
        "C++ reference produced silence; comparison is vacuous"
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
        panic!(
            "Rust KSS output is not bit-exact vs C++: first difference at sample index {idx} \
             (frame {sample_idx}, ~{time_ms:.1}ms, cpp={} rust={}), {diff_count} of {} samples \
             differ, max abs diff {max_diff}",
            cpp_out[idx],
            rust_out[idx],
            rust_out.len(),
        );
    }
}
