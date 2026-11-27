use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

fn render_rust(path: &str) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&data).expect("load failed");
    emu.start_track_impl(0).expect("start_track failed");

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2; // stereo
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

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2; // stereo
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
fn compare_spc_rust_vs_cpp() {
    let path = "../../assets/test.spc";
    let rust_out = render_rust(path);
    let cpp_out = render_cpp(path);

    assert_eq!(rust_out.len(), cpp_out.len(), "output lengths differ");

    let mut rust_nonzero = 0u64;
    let mut cpp_nonzero = 0u64;
    let mut first_diff = None;
    let mut diff_count = 0u64;
    for i in 0..rust_out.len() {
        if rust_out[i] != 0 {
            rust_nonzero += 1;
        }
        if cpp_out[i] != 0 {
            cpp_nonzero += 1;
        }
        if rust_out[i] != cpp_out[i] {
            diff_count += 1;
            if first_diff.is_none() {
                first_diff = Some(i);
            }
        }
    }

    eprintln!("Rust nonzero: {} / {}", rust_nonzero, rust_out.len());
    eprintln!("C++ nonzero: {} / {}", cpp_nonzero, cpp_out.len());
    eprintln!("Differing: {} / {}", diff_count, rust_out.len());

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
    }

    if diff_count == 0 {
        eprintln!("Outputs are identical!");
    } else {
        panic!(
            "SPC Rust output differs from C++ at index {:?} ({:.1}% differ)",
            first_diff,
            diff_count as f64 / rust_out.len() as f64 * 100.0
        );
    }
}
