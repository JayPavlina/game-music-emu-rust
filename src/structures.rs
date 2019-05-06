use crate::native::MusicEmu;
use core::borrow::Borrow;
use std::mem::transmute_copy;
use std::intrinsics::transmute;
use std::sync::Arc;
use crate::native;
use crate::experimental::GmeResult;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum GmeType {
    Ay,
    Gbs,
    Gym,
    Hes,
    Kss,
    Nsf,
    Nsfe,
    Sap,
    Spc,
    Vgm,
    /// Vgz comes with Vgm
    Vgz
}

mod extensions {
    pub const AY: &'static str = "AY";
    pub const GBS: &'static str = "GBS";
    pub const GYM: &'static str = "GYM";
    pub const HES: &'static str = "HES";
    pub const KSS: &'static str = "KSS";
    pub const NSF: &'static str = "NSF";
    pub const NSFE: &'static str = "NSFE";
    pub const SAP: &'static str = "SAP";
    pub const SPC: &'static str = "SPC";
    pub const VGM: &'static str = "VGM";
    pub const VGZ: &'static str = "VGZ";
}

impl GmeType {
    pub fn from_extension(value: &str) -> GmeType {
        match value {
            extensions::AY => GmeType::Ay,
            extensions::GBS => GmeType::Gbs,
            extensions::GYM => GmeType::Gym,
            extensions::HES => GmeType::Hes,
            extensions::KSS => GmeType::Kss,
            extensions::NSF => GmeType::Nsf,
            extensions::NSFE => GmeType::Nsfe,
            extensions::SAP => GmeType::Sap,
            extensions::SPC => GmeType::Spc,
            extensions::VGM => GmeType::Vgm,
            extensions::VGZ => GmeType::Vgz,
            _ => panic!()
        }
    }

    pub fn to_extension(&self) -> &'static str {
        match self {
            GmeType::Ay => extensions::AY,
            GmeType::Gbs => extensions::GBS,
            GmeType::Gym => extensions::GYM,
            GmeType::Hes => extensions::HES,
            GmeType::Kss => extensions::KSS,
            GmeType::Nsf => extensions::NSF,
            GmeType::Nsfe => extensions::NSFE,
            GmeType::Sap => extensions::SAP,
            GmeType::Spc => extensions::SPC,
            GmeType::Vgm => extensions::VGM,
            GmeType::Vgz => extensions::VGZ
        }
    }
}

#[derive(Clone)]
pub struct EmuHandle {
    pub(crate) emu: Arc<MusicEmu>
}

impl EmuHandle {
    pub fn new(emu: *const MusicEmu) -> Self {
        unsafe { Self { emu: Arc::new(transmute(emu)) } }
    }

    pub(crate) fn to_raw(&self) -> *const MusicEmu { unsafe { transmute_copy(&*self.emu) } }
}

impl Drop for EmuHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.emu) == 1 {
            native::delete(self);
        }
    }
}

pub struct GmeError(String);

impl GmeError {
    pub fn new(message: String) -> Self { Self(message) }

    pub fn message(&self) -> &str { &self.0 }
}