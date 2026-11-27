use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_sap::sap_emu::SapEmu;
use std::env;
use std::fs;
use std::io::Read;

fn main() {
    let path = env::args().nth(1).expect("Usage: sap_debug <file.sap>");
    let mut file_data = Vec::new();
    fs::File::open(&path)
        .unwrap()
        .read_to_end(&mut file_data)
        .unwrap();

    let mut emu = SapEmu::new();
    emu.set_sample_rate_impl(44100)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&file_data).expect("load failed");

    // Enable CPU debug tracing for first call_play
    // emu.cpu.debug_trace = true;

    emu.start_track_impl(0).expect("start_track failed");

    let buf_size = 44100 / 60 * 2; // stereo
    let mut buf = vec![0i16; buf_size];

    for frame in 0..120 {
        emu.play_impl(buf_size as i32, &mut buf)
            .expect("play failed");
        if frame >= 90 {
            let pc = emu.cpu.r.pc;
            eprintln!(
                "Frame {}: pc={:04X} A={:02X} X={:02X} Y={:02X} SP={:02X}",
                frame, pc, emu.cpu.r.a, emu.cpu.r.x, emu.cpu.r.y, emu.cpu.r.sp
            );
        }
    }
}
