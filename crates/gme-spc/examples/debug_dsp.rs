use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;

const SAMPLE_RATE: u32 = 32000;

fn main() {
    let data = std::fs::read("../../assets/test.spc").unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&data).expect("load failed");
    emu.start_track_impl(0).expect("start_track failed");

    // Play in small steps and dump voice 1 state around first non-zero
    let mut out = vec![0i16; 4096];
    let mut total_played = 0;

    // First, play until we get close to first non-zero
    while total_played < 1000 {
        emu.play_impl(64, &mut out[total_played..total_played + 64])
            .expect("play failed");
        total_played += 64;
    }

    // Now play one sample at a time and dump detailed state
    for i in 0..50 {
        let mut sample_out = [0i16; 2];
        emu.play_impl(2, &mut sample_out).expect("play failed");
        let sample_idx = total_played + i * 2;

        if sample_out[0] != 0 || sample_out[1] != 0 {
            let interp_pos = emu.apu.dsp.voice_interp_pos(1);
            let buf_pos = emu.apu.dsp.voice_buf_pos(1);
            let env = emu.apu.dsp.voice_env(1);
            let _kon_delay = emu.apu.dsp.voice_kon_delay(1);
            let _env_mode = emu.apu.dsp.voice_env_mode(1);
            let _brr_addr = emu.apu.dsp.voice_brr_addr(1);
            let _brr_offset = emu.apu.dsp.voice_brr_offset(1);

            let regs = emu.apu.dsp.regs();
            let pitch = ((regs[0x10 + 2] as u16) | ((regs[0x10 + 3] as u16) << 8)) & 0x3FFF;
            let in_base = buf_pos + ((interp_pos as u32) >> 12) as usize;
            let offset = ((interp_pos as u32) >> 3 & 0x1FE) as usize;

            println!(
                "Sample {}: L={} R={} interp_pos={:#x} buf_pos={} env={:#x} ({}) pitch={:#x} in_base={} offset={}",
                sample_idx,
                sample_out[0],
                sample_out[1],
                interp_pos,
                buf_pos,
                env,
                env,
                pitch,
                in_base % 24,
                offset
            );

            // We can't access buf directly, but we can try to infer from output
            // Print some register values
            println!(
                "  ADSR0={:#x} ADSR1={:#x} GAIN={:#x} VOLL={:#x} VOLR={:#x}",
                regs[0x10],
                regs[0x10 + 1],
                regs[0x10 + 7],
                regs[0x10 + 4],
                regs[0x10 + 5]
            );
        }
    }
}
