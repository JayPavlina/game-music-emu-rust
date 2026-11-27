pub mod dual_resampler;
pub mod header;
pub mod sms_apu;
pub mod vgm_emu;
mod ym2413;
pub mod ym2612;
#[cfg(feature = "ym2612_emu_gens")]
mod ym2612_gens;
pub mod ym2612_nuked;
