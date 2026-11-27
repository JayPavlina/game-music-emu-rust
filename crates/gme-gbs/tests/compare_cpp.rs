use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_gbs::gbs_emu::GbsEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

fn render_rust(path: &str) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = GbsEmu::new();
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
fn compare_gbs_rust_vs_cpp() {
    let path = "../../assets/test.gbs";
    let rust_out = render_rust(path);
    let cpp_out = render_cpp(path);

    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    let rust_nz = rust_out.iter().filter(|&&s| s != 0).count();
    let cpp_nz = cpp_out.iter().filter(|&&s| s != 0).count();
    let rust_max = rust_out.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let cpp_max = cpp_out.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    eprintln!(
        "Rust: {rust_nz}/{} nonzero, max abs {rust_max}",
        rust_out.len()
    );
    eprintln!(
        "C++:  {cpp_nz}/{} nonzero, max abs {cpp_max}",
        cpp_out.len()
    );

    let rust_first_nz = rust_out.iter().position(|&s| s != 0);
    let cpp_first_nz = cpp_out.iter().position(|&s| s != 0);
    eprintln!("Rust first non-zero: {rust_first_nz:?}");
    eprintln!("C++  first non-zero: {cpp_first_nz:?}");

    let mut first_diff = None;
    let mut diff_count = 0u64;
    for i in 0..rust_out.len() {
        if rust_out[i] != cpp_out[i] {
            diff_count += 1;
            if first_diff.is_none() {
                first_diff = Some(i);
            }
        }
    }

    if let Some(idx) = first_diff {
        let sample_idx = idx / 2;
        let time_ms = sample_idx as f64 / SAMPLE_RATE as f64 * 1000.0;
        eprintln!("First difference at sample index {idx} (frame {sample_idx}, ~{time_ms:.1}ms)");
        eprintln!(
            "Total differing samples: {diff_count} / {} ({:.2}%)",
            rust_out.len(),
            diff_count as f64 / rust_out.len() as f64 * 100.0
        );
        eprintln!();
        eprintln!("First 60 samples (index, cpp, rust):");
        for i in 0..60.min(rust_out.len()) {
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
        eprintln!();
        eprintln!("Samples around first diff:");
        let start = first_diff.unwrap().saturating_sub(5);
        let end = (start + 40).min(rust_out.len());
        for i in start..end {
            let marker = if rust_out[i] != cpp_out[i] {
                ">>>"
            } else {
                "   "
            };
            eprintln!(
                "  {marker} [{i}] cpp={:>6} rust={:>6} diff={}",
                cpp_out[i],
                rust_out[i],
                rust_out[i] as i32 - cpp_out[i] as i32
            );
        }
        let max_diff = rust_out
            .iter()
            .zip(cpp_out.iter())
            .map(|(r, c)| (*r as i32 - *c as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        let avg_diff: f64 = rust_out
            .iter()
            .zip(cpp_out.iter())
            .map(|(r, c)| (*r as i32 - *c as i32).unsigned_abs() as f64)
            .sum::<f64>()
            / rust_out.len() as f64;
        eprintln!("Max sample diff: {max_diff}, avg diff: {avg_diff:.1}");
        eprintln!(
            "Volume ratio: cpp_max/rust_max = {:.2}",
            cpp_max as f64 / rust_max.max(1) as f64
        );
    }

    // Hard gate: the Rust port must be bit-exact against the C++ reference.
    let max_diff = rust_out
        .iter()
        .zip(cpp_out.iter())
        .map(|(r, c)| (*r as i32 - *c as i32).unsigned_abs())
        .max()
        .unwrap_or(0);
    assert_eq!(
        max_diff, 0,
        "GBS output differs from C++ reference (first diff at {first_diff:?})"
    );
}
