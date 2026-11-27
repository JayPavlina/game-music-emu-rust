use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;

#[test]
fn dump_vrc6_first_samples() {
    let path = "../../assets/expansion/vrc6.nsf";
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.mute_voices_impl(0x1F); // mute APU
    emu.start_track_impl(0).expect("start_track failed");

    // Render 30 calls of 2048 samples (~1.4 sec of audio)
    let mut out = vec![0i16; 2048 * 30];
    emu.play_impl(2048 * 30, &mut out).expect("play failed");

    // Find first non-zero sample
    let mut first_nonzero = out.len();
    for (i, &s) in out.iter().enumerate() {
        if s != 0 {
            first_nonzero = i;
            break;
        }
    }
    eprintln!(
        "VRC6 first non-zero sample: {} ({} ms)",
        first_nonzero,
        first_nonzero as f32 / 88.2
    );
    eprintln!("First 20 samples: {:?}", &out[..20]);
    eprintln!(
        "Around first_nonzero ({}-{}):",
        first_nonzero.saturating_sub(2),
        (first_nonzero + 20).min(out.len())
    );
    #[allow(clippy::needless_range_loop)]
    for i in first_nonzero.saturating_sub(2)..(first_nonzero + 20).min(out.len()) {
        eprintln!("  [{}] = {}", i, out[i]);
    }
    eprintln!("Last 5 samples: {:?}", &out[out.len() - 5..]);
}
