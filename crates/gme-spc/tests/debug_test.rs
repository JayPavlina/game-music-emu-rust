use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;

const SAMPLE_RATE: u32 = 32000;
const RENDER_SECS: u32 = 3;
#[allow(dead_code)]
const CHUNK_SIZE: usize = 4096;

#[allow(dead_code)]
fn render_cpp(path: &str) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2; // stereo
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(CHUNK_SIZE);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn render_rust_step_by_step(path: &str) {
    let data = std::fs::read(path).unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&data).expect("load failed");
    emu.start_track_impl(0).expect("start_track failed");

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2; // stereo
    let mut out = vec![0i16; total_samples];

    // Play in small chunks and check voice state
    let chunk = 64;
    for pos in (0..total_samples).step_by(chunk) {
        let n = (total_samples - pos).min(chunk);
        emu.play_impl(n as i32, &mut out[pos..pos + n])
            .expect("play failed");

        // Check if we have non-zero output
        let has_nonzero = out[pos..pos + n].iter().any(|&x| x != 0);
        if has_nonzero {
            eprintln!("\nFirst non-zero at pos {}:", pos);
            #[allow(clippy::needless_range_loop)]
            for i in pos..(pos + n).min(pos + 20) {
                if out[i] != 0 {
                    eprintln!("  [{}] = {}", i, out[i]);
                }
            }

            // Dump voice 0 state
            let env = emu.apu.dsp.voice_env(0);
            let kon_delay = emu.apu.dsp.voice_kon_delay(0);
            let env_mode = emu.apu.dsp.voice_env_mode(0);
            let brr_addr = emu.apu.dsp.voice_brr_addr(0);
            let brr_offset = emu.apu.dsp.voice_brr_offset(0);
            let interp_pos = emu.apu.dsp.voice_interp_pos(0);
            let buf_pos = emu.apu.dsp.voice_buf_pos(0);
            let hidden_env = emu.apu.dsp.voice_hidden_env(0);

            eprintln!(
                "  v0: env={:#x} ({}) kon_delay={} env_mode={} brr_addr={:#x} brr_offset={} interp_pos={:#x} buf_pos={} hidden_env={:#x}",
                env,
                env,
                kon_delay,
                env_mode,
                brr_addr,
                brr_offset,
                interp_pos,
                buf_pos,
                hidden_env
            );

            // Also dump other voices
            for v in 1..8 {
                let env = emu.apu.dsp.voice_env(v);
                let kon_delay = emu.apu.dsp.voice_kon_delay(v);
                let env_mode = emu.apu.dsp.voice_env_mode(v);
                if env != 0 || kon_delay != 0 || env_mode != 0 {
                    let brr_addr = emu.apu.dsp.voice_brr_addr(v);
                    let brr_offset = emu.apu.dsp.voice_brr_offset(v);
                    let interp_pos = emu.apu.dsp.voice_interp_pos(v);
                    let hidden_env = emu.apu.dsp.voice_hidden_env(v);
                    eprintln!(
                        "  v{}: env={:#x} ({}) kon_delay={} env_mode={} brr_addr={:#x} brr_offset={} interp_pos={:#x} hidden_env={:#x}",
                        v,
                        env,
                        env,
                        kon_delay,
                        env_mode,
                        brr_addr,
                        brr_offset,
                        interp_pos,
                        hidden_env
                    );
                }
            }

            eprintln!(
                "  kon={:#x} new_kon={:#x} t_koff={:#x}",
                emu.apu.dsp.kon(),
                emu.apu.dsp.new_kon(),
                emu.apu.dsp.t_koff()
            );

            break;
        }
    }
}

#[test]
fn debug_first_nonzero() {
    let path = "../../assets/test.spc";
    render_rust_step_by_step(path);
}
