use game_music_emu_wrapper::GameMusicEmu;
use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;

fn render_apu_channel(path: &str, mask: i32) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.mute_voices_impl(mask);
    emu.start_track_impl(0).expect("start_track failed");

    let total_samples = SAMPLE_RATE as usize;
    let mut out = vec![0i16; total_samples];
    emu.play_impl(total_samples as i32, &mut out)
        .expect("play failed");
    out
}

fn render_apu_channel_cpp(path: &str, mask: i32) -> Vec<i16> {
    let emu = GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();
    emu.mute_voices(mask);

    let total_samples = SAMPLE_RATE as usize;
    let mut out = vec![0i16; total_samples];
    emu.play(total_samples, &mut out).unwrap();
    out
}

fn rms(out: &[i16]) -> f64 {
    let sum: f64 = out.iter().map(|&s| (s as f64).powi(2)).sum();
    (sum / out.len() as f64).sqrt()
}

fn test_expansion_file(path: &str, name: &str) {
    eprintln!("\n=== {} ===", name);
    for bit in 0..5 {
        let mask = !(1 << bit) & 0x1F;
        let rust_out = render_apu_channel(path, mask);
        let cpp_out = render_apu_channel_cpp(path, mask);

        let label = match bit {
            0 => "square1",
            1 => "square2",
            2 => "triangle",
            3 => "noise",
            4 => "dmc",
            _ => "unknown",
        };
        eprintln!(
            "  Rust {}: RMS={:.1}  C++: RMS={:.1}",
            label,
            rms(&rust_out),
            rms(&cpp_out)
        );
    }
}

#[test]
fn compare_vrc6_apu_channels() {
    test_expansion_file("../../assets/expansion/vrc6.nsf", "VRC6");
}

#[test]
fn compare_fds_apu_channels() {
    test_expansion_file("../../assets/expansion/fds.nsf", "FDS");
}

#[test]
fn compare_fme7_apu_channels() {
    test_expansion_file("../../assets/expansion/fme7.nsfe", "FME7");
}
