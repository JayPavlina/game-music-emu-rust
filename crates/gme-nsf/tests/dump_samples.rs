use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;
const NUM_SAMPLES: usize = 5000;

fn render_rust(path: &str) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.start_track_impl(0).expect("start_track failed");

    let total_samples = NUM_SAMPLES * 2; // stereo
    let mut out = vec![0i16; total_samples];
    emu.play_impl(total_samples as i32, &mut out)
        .expect("play failed");
    out
}

fn render_cpp(path: &str) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();

    let total_samples = NUM_SAMPLES * 2; // stereo
    let mut out = vec![0i16; total_samples];
    emu.play(total_samples, &mut out).unwrap();
    out
}

#[test]
fn dump_samples() {
    let path = "../../assets/test.nsf";
    let rust_out = render_rust(path);
    let cpp_out = render_cpp(path);

    // Write to the system temporary directory
    let mut rust_file =
        std::fs::File::create(std::env::temp_dir().join("rust_samples.txt")).unwrap();
    let mut cpp_file = std::fs::File::create(std::env::temp_dir().join("cpp_samples.txt")).unwrap();

    use std::io::Write;
    writeln!(rust_file, "idx,left,right").unwrap();
    writeln!(cpp_file, "idx,left,right").unwrap();

    for i in 0..NUM_SAMPLES {
        let rust_l = rust_out[i * 2];
        let rust_r = rust_out[i * 2 + 1];
        let cpp_l = cpp_out[i * 2];
        let cpp_r = cpp_out[i * 2 + 1];
        writeln!(rust_file, "{},{},{}", i, rust_l, rust_r).unwrap();
        writeln!(cpp_file, "{},{},{}", i, cpp_l, cpp_r).unwrap();
    }

    eprintln!(
        "Wrote {} samples to rust_samples.txt and cpp_samples.txt in the temporary directory",
        NUM_SAMPLES
    );
}
