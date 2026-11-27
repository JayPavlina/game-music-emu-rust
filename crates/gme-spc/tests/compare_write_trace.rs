//! Diffs the sequence of CPU-issued DSP register writes (time, addr, data)
//! between the C++ emulator (via SPC_DSP_WRITE_HOOK) and the Rust port.
//! With the DSP layer proven bit-exact in isolation, the first divergent
//! write localizes any remaining bug to the CPU/timer/sync layer.

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;

const SAMPLE_RATE: u32 = 32000;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;
fn spc_file() -> String {
    std::env::var("GME_SPC_FILE").unwrap_or_else(|_| "../../assets/test.spc".to_string())
}

fn render_cpp_traced() -> Vec<(i32, u16, u8)> {
    game_music_emu_wrapper::spc_dsp_shim::dsp_write_trace_reset();
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(spc_file(), SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(CHUNK_SIZE);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    game_music_emu_wrapper::spc_dsp_shim::dsp_write_trace()
}

fn render_rust_traced() -> Vec<(i32, u16, u8)> {
    let data = std::fs::read(spc_file()).unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.load_mem_impl(&data).unwrap();
    emu.apu.dsp_write_trace = Some(Vec::new());
    emu.start_track_impl(0).unwrap();

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(CHUNK_SIZE);
        emu.play_impl(n as i32, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    emu.apu.dsp_write_trace.take().unwrap()
}

#[test]
fn compare_dsp_write_traces() {
    let cpp = render_cpp_traced();
    let rust = render_rust_traced();

    println!("cpp writes: {}, rust writes: {}", cpp.len(), rust.len());

    let common = cpp.len().min(rust.len());

    // Addr/data sequence must match; report how the time offset evolves
    let mut last_delta = i64::MIN;
    let mut transitions = 0;
    for i in 0..common {
        if (cpp[i].1, cpp[i].2) == (rust[i].1, rust[i].2) {
            let delta = rust[i].0 as i64 - cpp[i].0 as i64;
            if delta != last_delta {
                println!(
                    "  time delta change at write {:6}: rust - cpp = {:6} (addr={:#04x} data={:#04x} cpp_t={})",
                    i, delta, cpp[i].1, cpp[i].2, cpp[i].0
                );
                last_delta = delta;
                transitions += 1;
                if transitions > 60 {
                    println!("  ... (more transitions suppressed)");
                    break;
                }
            }
        }
    }

    for i in 0..common {
        if (cpp[i].1, cpp[i].2) != (rust[i].1, rust[i].2) {
            let lo = i.saturating_sub(5);
            println!("First divergent DSP write at index {}:", i);
            println!("  idx | cpp (time, addr, data) | rust (time, addr, data)");
            for j in lo..(i + 6).min(common) {
                println!(
                    "  {}{:6} | t={:8} r={:#04x} d={:#04x} | t={:8} r={:#04x} d={:#04x}",
                    if j == i { ">" } else { " " },
                    j,
                    cpp[j].0,
                    cpp[j].1,
                    cpp[j].2,
                    rust[j].0,
                    rust[j].1,
                    rust[j].2
                );
            }
            panic!("DSP write traces diverge at index {}", i);
        }
    }

    assert_eq!(
        cpp.len(),
        rust.len(),
        "traces match for {} writes but lengths differ",
        common
    );
    println!("OK: {} DSP register writes identical", common);
}
