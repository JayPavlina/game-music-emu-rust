use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;

#[test]
fn trace_vrc6_writes() {
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
    for i in 0..30 {
        emu.play_impl(2048, &mut out).expect("play failed");
        let sum: f64 = out.iter().map(|&s| (s as f64).powi(2)).sum();
        let rms = (sum / out.len() as f64).sqrt();
        eprintln!(
            "Frame {}: t≈{}ms out RMS={:.3} max_abs={}",
            i,
            i * 2048 * 1000 / 44100,
            rms,
            out.iter().map(|s| s.abs() as i32).max().unwrap_or(0)
        );
    }
}
