pub mod apu;
pub mod chips;
pub mod cpu;
pub mod header;
pub mod nsf_emu;
pub mod nsfe;
pub mod oscs;
pub mod rom;

/// Differential-trace hook for APU register writes, mirroring the C++
/// NSF_APU_TRACE hook in Nes_Apu.cpp. Enabled via NSF_APU_TRACE_RS=<path>.
/// Compiled out unless the `trace` cargo feature is on; diagnostic runs use
/// `cargo test -p gme-nsf --features trace` with the env var set.
#[cfg(feature = "trace")]
pub fn apu_trace(time: i32, addr: u16, data: u8) {
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    static TRACE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let f = TRACE.get_or_init(|| {
        std::env::var("NSF_APU_TRACE_RS")
            .ok()
            .and_then(|p| std::fs::File::create(p).ok())
            .map(Mutex::new)
    });
    if let Some(f) = f {
        let _ = writeln!(f.lock().unwrap(), "W {} {:04X} {:02X}", time, addr, data);
    }
}
