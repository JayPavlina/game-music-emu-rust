//! Drives the C++ Spc_Dsp (via game-music-emu-wrapper's test shim) and the
//! Rust SpcDsp standalone — no SPC700 CPU — on the same SPC file's RAM and
//! DSP registers, and asserts bit-exact samples plus identical voice state
//! every sample. Pinpoints DSP divergences without CPU noise.

use game_music_emu_wrapper::spc_dsp_shim::SpcDspShim;
use gme_spc::spc_dsp::SpcDsp;

fn spc_file() -> String {
    std::env::var("GME_SPC_FILE").unwrap_or_else(|_| "../../assets/test.spc".to_string())
}
const SECONDS: usize = 3;
const NATIVE_RATE: usize = 32000;
const CLOCKS_PER_SAMPLE: i32 = 32;

struct SpcImage {
    ram: Box<[u8; 0x10000]>,
    dsp_regs: [u8; 128],
}

fn load_spc_image() -> SpcImage {
    let data = std::fs::read(spc_file()).expect("assets/test.spc missing");
    assert!(data.len() >= 0x10180, "SPC file too small");
    let mut ram: Box<[u8; 0x10000]> = vec![0u8; 0x10000].into_boxed_slice().try_into().unwrap();
    ram.copy_from_slice(&data[0x100..0x10100]);
    let mut dsp_regs = [0u8; 128];
    dsp_regs.copy_from_slice(&data[0x10100..0x10180]);
    SpcImage { ram, dsp_regs }
}

#[test]
fn compare_dsp_isolated() {
    let image = load_spc_image();

    // C++ DSP (owns a copy of RAM)
    let mut cpp = SpcDspShim::new(&image.ram[..]);
    cpp.load(&image.dsp_regs);

    // Rust DSP (operates on our copy of RAM)
    let mut rust_ram = image.ram.clone();
    let mut rust = SpcDsp::new();
    rust.init();
    rust.load(&image.dsp_regs);

    let total_samples = SECONDS * NATIVE_RATE;
    let mut diverged = false;

    for sample_idx in 0..total_samples {
        let mut cpp_pair = [0i16; 2];
        let mut rust_pair = [0i16; 2];

        cpp.run(CLOCKS_PER_SAMPLE, &mut cpp_pair);
        rust.set_output(Some(&mut rust_pair));
        rust.run(CLOCKS_PER_SAMPLE, &mut rust_ram);

        if cpp_pair != rust_pair {
            println!(
                "SAMPLE DIVERGENCE at sample {}: cpp={:?} rust={:?}",
                sample_idx, cpp_pair, rust_pair
            );
            diverged = true;
        }

        // Global state
        let globals_cpp = (
            cpp.kon(),
            cpp.new_kon(),
            cpp.t_koff(),
            cpp.noise(),
            cpp.every_other_sample(),
            cpp.echo_offset(),
            cpp.phase(),
        );
        let globals_rust = (
            rust.kon(),
            rust.new_kon(),
            rust.t_koff(),
            rust.noise(),
            rust.every_other_sample(),
            rust.echo_offset(),
            rust.phase(),
        );
        if globals_cpp != globals_rust {
            println!(
                "GLOBAL STATE DIVERGENCE at sample {}:\n  (kon, new_kon, t_koff, noise, eos, echo_off, phase)\n  cpp ={:?}\n  rust={:?}",
                sample_idx, globals_cpp, globals_rust
            );
            diverged = true;
        }
        for i in 0..4 {
            if cpp.counter(i) != rust.counter(i) {
                println!(
                    "COUNTER {} DIVERGENCE at sample {}: cpp={:#x} rust={:#x}",
                    i,
                    sample_idx,
                    cpp.counter(i),
                    rust.counter(i)
                );
                diverged = true;
            }
        }

        // Per-voice state
        for v in 0..8 {
            let vc = (
                cpp.voice_env(v),
                cpp.voice_env_mode(v),
                cpp.voice_kon_delay(v),
                cpp.voice_interp_pos(v),
                cpp.voice_brr_addr(v),
                cpp.voice_brr_offset(v),
                cpp.voice_hidden_env(v),
                cpp.voice_buf_pos(v),
            );
            let vr = (
                rust.voice_env(v),
                rust.voice_env_mode(v) as i32,
                rust.voice_kon_delay(v),
                rust.voice_interp_pos(v),
                rust.voice_brr_addr(v),
                rust.voice_brr_offset(v),
                rust.voice_hidden_env(v),
                rust.voice_buf_pos(v) as i32,
            );
            if vc != vr {
                println!(
                    "VOICE {} STATE DIVERGENCE at sample {}:\n  (env, env_mode, kon_delay, interp_pos, brr_addr, brr_offset, hidden_env, buf_pos)\n  cpp ={:?}\n  rust={:?}",
                    v, sample_idx, vc, vr
                );
                diverged = true;
            }
        }

        // RAM must match too (echo buffer writes)
        if sample_idx % 1024 == 0 && cpp.ram()[..] != rust_ram[..] {
            let first = cpp
                .ram()
                .iter()
                .zip(rust_ram.iter())
                .position(|(a, b)| a != b)
                .unwrap();
            println!(
                "RAM DIVERGENCE at sample {}: first differing byte at {:#06x}: cpp={:#04x} rust={:#04x}",
                sample_idx,
                first,
                cpp.ram()[first],
                rust_ram[first]
            );
            diverged = true;
        }

        assert!(
            !diverged,
            "DSP state diverged at sample {} (see output above)",
            sample_idx
        );
    }

    println!(
        "OK: {} samples bit-exact, all voice/global state identical",
        total_samples
    );
}
