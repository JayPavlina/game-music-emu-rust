use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_vgm::vgm_emu::VgmEmu;

fn load_test_vgm() -> VgmEmu {
    let data = std::fs::read("../../assets/test.vgm")
        .or_else(|_| std::fs::read("assets/test.vgm"))
        .expect("test.vgm not found");
    let mut emu = VgmEmu::new();
    emu.load_mem_impl(&data).expect("Failed to load test.vgm");
    emu
}

#[test]
fn test_load_vgm() {
    let emu = load_test_vgm();
    assert_eq!(emu.state.track_count, 1); // VGM files hold a single track
    assert!(emu.header.psg_rate() > 0, "Should have PSG rate");
}

#[test]
fn test_set_sample_rate() {
    let mut emu = load_test_vgm();
    emu.set_sample_rate_impl(44100).unwrap();
    assert_eq!(emu.emu_state.sample_rate, 44100);
}

#[test]
fn test_start_track() {
    let mut emu = load_test_vgm();
    emu.set_sample_rate_impl(44100).unwrap();
    emu.start_track_impl(0).unwrap();
    assert!(!emu.emu_state.track_ended);
    assert!(!emu.emu_state.emu_track_ended);
}

#[test]
fn test_play_produces_nonzero_output() {
    let mut emu = load_test_vgm();
    emu.set_sample_rate_impl(44100).unwrap();
    emu.start_track_impl(0).unwrap();

    let count = 4410 * 2; // 0.1 seconds stereo
    let mut out = vec![0i16; count];
    emu.play_impl(count as i32, &mut out).unwrap();

    let nonzero = out.iter().filter(|&&s| s != 0).count();
    assert!(
        nonzero > 0,
        "Expected non-zero audio output, got {} / {} nonzero",
        nonzero,
        count
    );
}

#[test]
fn test_track_info() {
    let emu = load_test_vgm();
    let mut info = gme_common::track_info::TrackInfo::default();
    emu.track_info_impl(&mut info, 0).unwrap();
    // VGM files should have system info
    assert!(!info.system.is_empty(), "System should be set");
}

#[test]
fn test_play_multiple_frames() {
    let mut emu = load_test_vgm();
    emu.set_sample_rate_impl(44100).unwrap();
    emu.start_track_impl(0).unwrap();

    // Play 1 second of audio
    let total = 44100 * 2; // stereo
    let mut out = vec![0i16; total];
    let chunk_size = 4410 * 2; // 100ms chunks

    for i in (0..total).step_by(chunk_size) {
        let end = (i + chunk_size).min(total);
        emu.play_impl((end - i) as i32, &mut out[i..end]).unwrap();
    }

    let nonzero = out.iter().filter(|&&s| s != 0).count();
    assert!(
        nonzero > 100,
        "Expected significant non-zero audio, got {} / {} nonzero",
        nonzero,
        total
    );
}
