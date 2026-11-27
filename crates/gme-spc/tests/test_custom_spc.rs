use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;
use std::fs;

const SAMPLE_RATE: u32 = 32000;

#[test]
fn test_custom_spc() {
    fn render_cpp(path: &str) -> Vec<i16> {
        let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
        emu.start_track(0).unwrap();
        let mut out = vec![0i16; SAMPLE_RATE as usize * 2 * 2];
        emu.play(out.len(), &mut out).unwrap();
        out
    }

    fn render_rust(path: &str) -> Vec<i16> {
        let data = fs::read(path).unwrap();
        let mut emu = SpcEmu::new();
        emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
        emu.load_mem_impl(&data).unwrap();
        emu.start_track_impl(0).unwrap();
        let mut out = vec![0i16; SAMPLE_RATE as usize * 2 * 2];
        emu.play_impl(out.len() as i32, &mut out).unwrap();
        out
    }

    let path = "../../assets/eight_voices.spc";

    println!("Rendering C++...");
    let cpp_out = render_cpp(path);

    println!("Rendering Rust...");
    let rust_out = render_rust(path);

    let cpp_first = cpp_out.iter().position(|&x| x != 0);
    let rust_first = rust_out.iter().position(|&x| x != 0);

    println!("First non-zero C++: {:?}", cpp_first);
    println!("First non-zero Rust: {:?}", rust_first);

    if let (Some(c), Some(_r)) = (cpp_first, rust_first) {
        println!("At first non-zero:");
        for i in c..c + 10.min(cpp_out.len()).min(rust_out.len()) {
            println!(
                "[{}] cpp={} rust={} diff={}",
                i,
                cpp_out[i],
                rust_out[i],
                (cpp_out[i] as i32 - rust_out[i] as i32).abs()
            );
        }
    }

    let mut diff_count = 0;
    let mut total_diff = 0i64;
    for i in 0..cpp_out.len().min(rust_out.len()) {
        let diff = (cpp_out[i] as i32 - rust_out[i] as i32).abs();
        if diff > 0 {
            diff_count += 1;
            total_diff += diff as i64;
        }
    }
    println!("Total samples: {}", cpp_out.len());
    println!(
        "Different samples: {} ({:.2}%)",
        diff_count,
        diff_count as f64 / cpp_out.len() as f64 * 100.0
    );
    println!("Avg diff: {}", total_diff / diff_count.max(1) as i64);
}
