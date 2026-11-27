//! Differential test: render assets/test.gym through the Rust GymEmu and the
//! vendored C++ implementation; outputs must be bit-exact.

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_gym::GymEmu;

const GYM_PATH: &str = "../../assets/test.gym";
const SAMPLE_RATE: i32 = 44100;
const RENDER_SECS: usize = 3;
const CHUNK: usize = 4096;

fn render_rust(data: &[u8]) -> Vec<i16> {
    let mut emu = GymEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(data).expect("rust load failed");
    emu.start_track_impl(0).expect("start_track failed");
    let total = SAMPLE_RATE as usize * RENDER_SECS * 2;
    let mut out = vec![0i16; total];
    for chunk in out.chunks_mut(CHUNK) {
        emu.play_impl(chunk.len() as i32, chunk).unwrap();
    }
    out
}

fn render_cpp() -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(GYM_PATH, SAMPLE_RATE as u32)
        .expect("cpp load failed");
    emu.start_track(0).unwrap();
    let total = SAMPLE_RATE as usize * RENDER_SECS * 2;
    let mut out = vec![0i16; total];
    for chunk in out.chunks_mut(CHUNK) {
        emu.play(chunk.len(), chunk).unwrap();
    }
    out
}

#[test]
fn compare_cpp_gym() {
    let data = std::fs::read(GYM_PATH).expect("missing assets/test.gym");
    let rust_out = render_rust(&data);
    let cpp_out = render_cpp();
    assert_eq!(rust_out.len(), cpp_out.len());

    let nonzero = cpp_out.iter().filter(|&&s| s != 0).count();
    assert!(
        nonzero > 1000,
        "C++ reference produced only {nonzero} nonzero samples; test is not meaningful"
    );

    let mut diff_count = 0usize;
    let mut first_diff = None;
    for (i, (&c, &r)) in cpp_out.iter().zip(rust_out.iter()).enumerate() {
        if c != r {
            if first_diff.is_none() {
                first_diff = Some(i);
            }
            diff_count += 1;
        }
    }
    if let Some(i) = first_diff {
        let lo = i.saturating_sub(4);
        let hi = (i + 4).min(cpp_out.len());
        panic!(
            "{diff_count} samples differ; first at index {i} (sample {}, {}): cpp={} rust={}\n\
             context cpp:  {:?}\n\
             context rust: {:?}",
            i / 2,
            if i % 2 == 0 { "L" } else { "R" },
            cpp_out[i],
            rust_out[i],
            &cpp_out[lo..hi],
            &rust_out[lo..hi],
        );
    }
}

#[test]
fn packed_gym_rejected() {
    let mut data = std::fs::read(GYM_PATH).expect("missing assets/test.gym");
    data[424] = 1; // set the packed flag

    // Same error as the vendored C++ build (compiled without zlib)
    let mut emu = GymEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE).unwrap();
    let err = emu.load_mem_impl(&data).expect_err("packed GYM must fail");
    assert_eq!(err.as_str(), "Packed GYM file not supported");

    let cpp = game_music_emu_wrapper::GameMusicEmu::from_data(&data, SAMPLE_RATE as u32);
    assert!(cpp.is_err(), "C++ should reject a packed GYM");
}
