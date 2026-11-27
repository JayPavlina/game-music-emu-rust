//! HES (TurboGrafx-16 / PC Engine) music file emulator.
//! Rust port of Game_Music_Emu's Hes_Emu / Hes_Cpu / Hes_Apu / Hes_Apu_Adpcm.

pub mod adpcm;
pub mod apu;
pub mod cpu;
mod cpu_run;
pub mod hes_emu;
pub mod rom;

pub use hes_emu::HesEmu;
