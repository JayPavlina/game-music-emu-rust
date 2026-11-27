use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_core::multi_buffer::MultiBuffer;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;

#[test]
fn debug_buffer_growth() {
    let path = "../../assets/expansion/vrc6.nsf";
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.mute_voices_impl(0x1F); // mute APU
    emu.start_track_impl(0).expect("start_track failed");

    let mut out = vec![0i16; 4096];
    for frame in 0..5 {
        emu.play_impl(2048, &mut out).expect("play failed");
        let buf = unsafe { &mut *emu.stereo_buf.channel(0, 0).center };
        let avail = buf.samples_avail();
        eprintln!(
            "Frame {}: buffer.len()={} samples_avail()={} offset={}",
            frame,
            buf.buffer.len(),
            avail,
            buf.offset
        );
    }
}
