pub mod ay_apu;
pub mod header;
pub mod kss_cpu;
pub mod kss_emu;
pub mod kss_scc_apu;

/// Differential-trace hook for CPU OUT events, mirroring the C++
/// KSS_OUT_TRACE hook in Kss_Emu.cpp. Enabled via KSS_OUT_TRACE_RS=<path>.
/// Compiled out unless the `trace` cargo feature is on; diagnostic runs use
/// `cargo test -p gme-kss --features trace` with the env var set.
#[cfg(feature = "trace")]
pub fn kss_out_trace(time: i32, addr: u16, data: u8) {
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    static TRACE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let f = TRACE.get_or_init(|| {
        std::env::var("KSS_OUT_TRACE_RS")
            .ok()
            .and_then(|p| std::fs::File::create(p).ok())
            .map(Mutex::new)
    });
    if let Some(f) = f {
        let _ = writeln!(
            f.lock().unwrap(),
            "O {} {:02X} {:02X}",
            time,
            addr & 0xFF,
            data
        );
    }
}

/// Differential-trace hook for CPU instructions, mirroring the C++
/// KSS_CPU_TRACE hook in Kss_Cpu.cpp. Enabled via KSS_CPU_TRACE_RS=<path>.
/// Compiled out unless the `trace` cargo feature is on (it costs an
/// indirect call per instruction); diagnostic runs use
/// `cargo test -p gme-kss --features trace` with the env var set.
#[cfg(feature = "trace")]
pub fn kss_cpu_trace(pc: u16, opcode: u8, time: i32) {
    use std::io::Write;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Mutex, OnceLock};
    static TRACE: OnceLock<Option<Mutex<std::io::BufWriter<std::fs::File>>>> = OnceLock::new();
    static COUNT: AtomicI64 = AtomicI64::new(0);
    let f = TRACE.get_or_init(|| {
        std::env::var("KSS_CPU_TRACE_RS")
            .ok()
            .and_then(|p| std::fs::File::create(p).ok())
            .map(|f| Mutex::new(std::io::BufWriter::new(f)))
    });
    if let Some(f) = f
        && COUNT.fetch_add(1, Ordering::Relaxed) < 2_000_000
    {
        let _ = writeln!(f.lock().unwrap(), "{:04X} {:02X} t={}", pc, opcode, time);
    }
}
