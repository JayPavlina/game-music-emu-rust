#[cfg(feature = "ym2612_emu_gens")]
use crate::ym2612_gens::Gens;
use crate::ym2612_nuked::Ym2612NukedEmu;
use gme_common::music_emu::Ym2612Backend;
use gme_core::error::{GmeError, GmeResult};
#[cfg(feature = "ym2612_emu_mame")]
use gme_ym2612_mame::Mame;

pub enum Ym2612 {
    Nuked(Box<Ym2612NukedEmu>),
    #[cfg(feature = "ym2612_emu_gens")]
    Gens(Gens),
    #[cfg(feature = "ym2612_emu_mame")]
    Mame(Mame),
}
impl Default for Ym2612 {
    fn default() -> Self {
        Self::Nuked(Box::default())
    }
}
impl Ym2612 {
    pub fn select(backend: Ym2612Backend) -> GmeResult<Self> {
        match backend {
            Ym2612Backend::Nuked => Ok(Self::default()),
            #[cfg(feature = "ym2612_emu_gens")]
            Ym2612Backend::Gens => Ok(Self::Gens(Gens::default())),
            #[cfg(feature = "ym2612_emu_mame")]
            Ym2612Backend::Mame => Ok(Self::Mame(Mame::default())),
            #[allow(unreachable_patterns)]
            _ => Err(GmeError::Internal("YM2612 backend feature is disabled")),
        }
    }
    pub fn set_rate(&mut self, rate: f64, clock: f64) -> GmeResult {
        match self {
            Self::Nuked(chip) => {
                chip.set_rate(rate, clock);
                Ok(())
            }
            #[cfg(feature = "ym2612_emu_gens")]
            Self::Gens(chip) => chip
                .set_rate(rate, clock)
                .then_some(())
                .ok_or(GmeError::Internal("Unsupported YM2612 rate")),
            #[cfg(feature = "ym2612_emu_mame")]
            Self::Mame(chip) => chip
                .set_rate(rate, clock)
                .then_some(())
                .ok_or(GmeError::Internal("Unsupported YM2612 rate")),
        }
    }
    pub fn reset(&mut self) {
        match self {
            Self::Nuked(chip) => chip.reset(),
            #[cfg(feature = "ym2612_emu_gens")]
            Self::Gens(chip) => chip.reset(),
            #[cfg(feature = "ym2612_emu_mame")]
            Self::Mame(chip) => chip.reset(),
        }
    }
    pub fn mute_voices(&mut self, mask: i32) {
        match self {
            Self::Nuked(chip) => chip.mute_voices(mask),
            #[cfg(feature = "ym2612_emu_gens")]
            Self::Gens(chip) => chip.mute_voices(mask),
            #[cfg(feature = "ym2612_emu_mame")]
            Self::Mame(chip) => chip.mute_voices(mask),
        }
    }
    pub fn write0(&mut self, addr: i32, data: i32) {
        match self {
            Self::Nuked(chip) => chip.write0(addr, data),
            #[cfg(feature = "ym2612_emu_gens")]
            Self::Gens(chip) => chip.write(false, addr as u8, data as u8),
            #[cfg(feature = "ym2612_emu_mame")]
            Self::Mame(chip) => chip.write(false, addr as u8, data as u8),
        }
    }
    pub fn write1(&mut self, addr: i32, data: i32) {
        match self {
            Self::Nuked(chip) => chip.write1(addr, data),
            #[cfg(feature = "ym2612_emu_gens")]
            Self::Gens(chip) => chip.write(true, addr as u8, data as u8),
            #[cfg(feature = "ym2612_emu_mame")]
            Self::Mame(chip) => chip.write(true, addr as u8, data as u8),
        }
    }
    pub fn run(&mut self, frames: i32, out: &mut [i16]) {
        let out = &mut out[..usize::try_from(frames).expect("negative frame count") * 2];
        match self {
            Self::Nuked(chip) => chip.run(frames, out),
            #[cfg(feature = "ym2612_emu_gens")]
            Self::Gens(chip) => chip.render(out),
            #[cfg(feature = "ym2612_emu_mame")]
            Self::Mame(chip) => chip.render(out),
        }
    }
}
