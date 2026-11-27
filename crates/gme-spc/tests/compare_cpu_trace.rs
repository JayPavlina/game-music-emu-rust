//! Compare the first 200,000 SPC700 instructions on both emulators.

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;

const SAMPLE_RATE: u32 = 32000;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;
fn spc_file() -> String {
    std::env::var("GME_SPC_FILE").unwrap_or_else(|_| "../../assets/test.spc".to_string())
}
const LIMIT: usize = 200_000;
type Callback = extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32);
unsafe extern "C" {
    fn gme_spc_set_trace_callback(callback: Option<Callback>);
}
thread_local! {
    static TRACE: std::cell::RefCell<Vec<Insn>> = const { std::cell::RefCell::new(Vec::new()) };
}
extern "C" fn record(pc: i32, op: i32, a: i32, x: i32, y: i32, sp: i32, time: i32, psw: i32) {
    TRACE.with_borrow_mut(|trace| {
        trace.push((
            pc as u16, op as u8, a as u8, x as u8, y as u8, sp as u8, time, psw as u8,
        ));
        if trace.len() == LIMIT {
            unsafe { gme_spc_set_trace_callback(None) };
        }
    });
}

type Insn = (u16, u8, u8, u8, u8, u8, i32, u8); // pc, opcode, a, x, y, sp, rel_time, psw

fn render_cpp_traced() -> Vec<Insn> {
    unsafe { gme_spc_set_trace_callback(Some(record)) };
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

    unsafe { gme_spc_set_trace_callback(None) };
    TRACE.take()
}

fn render_rust_traced() -> Vec<Insn> {
    let data = std::fs::read(spc_file()).unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.load_mem_impl(&data).unwrap();
    emu.apu.cpu_trace = Some(Vec::new());
    emu.start_track_impl(0).unwrap();
    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(CHUNK_SIZE);
        emu.play_impl(n as i32, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    emu.apu.cpu_trace.take().unwrap()
}

#[test]
fn compare_cpu_traces() {
    let cpp = render_cpp_traced();
    let rust = render_rust_traced();
    println!("cpp insns: {}, rust insns: {}", cpp.len(), rust.len());

    let common = cpp.len().min(rust.len());
    assert_eq!(common, LIMIT);
    for i in 0..common {
        if cpp[i] != rust[i] {
            println!("First divergent instruction at index {}:", i);
            println!("  idx  | cpp: PC  OP  A  X  Y  SP  T  P | rust: PC  OP  A  X  Y  SP  T  P");
            for j in i.saturating_sub(8)..(i + 8).min(common) {
                let (cp, co, ca, cx, cy, cs, ct, cw) = cpp[j];
                let (rp, ro, ra, rx, ry, rs, rt, rw) = rust[j];
                println!(
                    "  {}{:6} | {:04X} {:02X} {:02X} {:02X} {:02X} {:02X} {:7} {:02X} | {:04X} {:02X} {:02X} {:02X} {:02X} {:02X} {:7} {:02X}",
                    if j == i { ">" } else { " " },
                    j,
                    cp,
                    co,
                    ca,
                    cx,
                    cy,
                    cs,
                    ct,
                    cw,
                    rp,
                    ro,
                    ra,
                    rx,
                    ry,
                    rs,
                    rt,
                    rw
                );
            }
            panic!("CPU traces diverge at instruction {}", i);
        }
    }
    println!("OK: first {} instructions identical", common);
}
