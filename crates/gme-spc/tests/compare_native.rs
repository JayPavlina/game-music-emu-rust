use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;

const SAMPLE_RATE: u32 = 32000;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

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

fn render_rust(path: &str) -> (Vec<i16>, gme_spc::spc_apu::SpcApu) {
    let data = std::fs::read(path).unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.load_mem_impl(&data).expect("load failed");
    emu.start_track_impl(0).expect("start_track failed");

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2; // stereo
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(CHUNK_SIZE);
        emu.play_impl(n as i32, &mut out[pos..pos + n])
            .expect("play failed");
        pos += n;
    }
    (out, emu.apu)
}

#[test]
fn dump_rust_voice_state() {
    let path = "../../assets/test.spc";
    let (rust_out, apu) = render_rust(path);

    eprintln!("Rust output at first non-zero samples:");
    #[allow(clippy::needless_range_loop)]
    for i in 0..1100.min(rust_out.len()) {
        if rust_out[i] != 0 || (1020..1100).contains(&i) {
            eprintln!("  [{}] = {}", i, rust_out[i]);
        }
    }
    eprintln!();
    eprintln!("Rust voice state at end of render:");
    for v in 0..8 {
        let env = apu.dsp.voice_env(v);
        let kon_delay = apu.dsp.voice_kon_delay(v);
        let env_mode = apu.dsp.voice_env_mode(v);
        let brr_addr = apu.dsp.voice_brr_addr(v);
        let brr_offset = apu.dsp.voice_brr_offset(v);
        let interp_pos = apu.dsp.voice_interp_pos(v);
        if env != 0 || kon_delay != 0 || env_mode != 0 {
            eprintln!(
                "  v{}: env={:#x} kon_delay={} env_mode={} brr_addr={:#x} brr_offset={} interp_pos={:#x}",
                v, env, kon_delay, env_mode, brr_addr, brr_offset, interp_pos
            );
        }
    }
    eprintln!(
        "  kon={:#x} new_kon={:#x} t_koff={:#x}",
        apu.dsp.kon(),
        apu.dsp.new_kon(),
        apu.dsp.t_koff()
    );
}

#[test]
fn compare_first_samples() {
    let path = "../../assets/test.spc";
    let cpp_out = render_cpp(path);
    let (rust_out, _apu) = render_rust(path);

    eprintln!("First non-zero samples comparison (32000Hz):");
    eprintln!("  idx | cpp_L | cpp_R | rust_L | rust_R");
    let mut first_cpp = None;
    let mut first_rust = None;
    for i in (0..cpp_out.len()).step_by(2) {
        if cpp_out[i] != 0 || cpp_out[i + 1] != 0 {
            first_cpp = Some(i);
            break;
        }
    }
    for i in (0..rust_out.len()).step_by(2) {
        if rust_out[i] != 0 || rust_out[i + 1] != 0 {
            first_rust = Some(i);
            break;
        }
    }
    eprintln!("  First non-zero cpp: {:?}", first_cpp);
    eprintln!("  First non-zero rust: {:?}", first_rust);

    let start = first_cpp.unwrap_or(0).min(first_rust.unwrap_or(0));
    let end = (start + 40).min(cpp_out.len());
    for i in (start..end).step_by(2) {
        eprintln!(
            "  [{:4}] | {:6} | {:6} | {:6} | {:6}",
            i,
            cpp_out[i],
            cpp_out[i + 1],
            rust_out[i],
            rust_out[i + 1]
        );
    }
}
