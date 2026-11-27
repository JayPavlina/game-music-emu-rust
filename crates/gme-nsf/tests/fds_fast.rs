use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

fn render_rust(path: &str) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(44100).unwrap();
    emu.start_track_impl(0).unwrap();
    let total = 44100 * 2 * 3;
    let mut out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(4096);
        emu.play_impl(n as i32, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn render_cpp(path: &str) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, 44100).unwrap();
    emu.start_track(0).unwrap();
    let total = 44100 * 2 * 3;
    let mut out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(4096);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

#[test]
fn compare_fds_fast_path() {
    for (name, path) in &[("fds.nsf", "../../assets/expansion/fds.nsf")] {
        let rust = render_rust(path);
        let cpp = render_cpp(path);
        let mut diff_count = 0u64;
        for i in 0..rust.len() {
            if rust[i] != cpp[i] {
                diff_count += 1;
            }
        }
        let pct = diff_count as f64 / rust.len() as f64 * 100.0;
        eprintln!(
            "{}: {}/{} differ ({:.1}%)",
            name,
            diff_count,
            rust.len(),
            pct
        );
    }
}
