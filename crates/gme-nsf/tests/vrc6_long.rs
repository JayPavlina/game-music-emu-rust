use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;

#[test]
fn trace_vrc6_long() {
    let path = "../../assets/expansion/vrc6.nsf";
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.mute_voices_impl(0x1F);
    emu.start_track_impl(0).expect("start_track failed");

    let mut out = vec![0i16; 4096];
    let mut last_print_sec = 0;
    for i in 0..600 {
        emu.play_impl(2048, &mut out).expect("play failed");
        let sec = (i * 2048 * 1000) / (SAMPLE_RATE as usize * 1000);
        if sec != last_print_sec {
            let sum: f64 = out.iter().map(|&s| (s as f64).powi(2)).sum();
            let rms = (sum / out.len() as f64).sqrt();
            eprintln!(
                "Sec {}: out RMS={:.3} max_abs={}",
                sec,
                rms,
                out.iter().map(|s| s.abs() as i32).max().unwrap_or(0)
            );
            last_print_sec = sec;
        }
    }
}
