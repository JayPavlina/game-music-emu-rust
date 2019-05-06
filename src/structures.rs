use crate::native::MusicEmu;
use core::borrow::Borrow;
use std::mem::transmute_copy;
use std::intrinsics::transmute;
use std::sync::Arc;
use crate::native;

pub(crate) type GmeVoidResult = Result<(), GmeError>;
pub(crate) type GmeHandleResult = Result<EmuHandle, GmeError>;

/// All supported emulator types
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum EmuType {
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

/// File extension for each EmuType
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

impl EmuType {
    /// Get an `EmuType` from a file extension
    pub fn from_extension(value: &str) -> EmuType {
        match value {
            extensions::AY => EmuType::Ay,
            extensions::GBS => EmuType::Gbs,
            extensions::GYM => EmuType::Gym,
            extensions::HES => EmuType::Hes,
            extensions::KSS => EmuType::Kss,
            extensions::NSF => EmuType::Nsf,
            extensions::NSFE => EmuType::Nsfe,
            extensions::SAP => EmuType::Sap,
            extensions::SPC => EmuType::Spc,
            extensions::VGM => EmuType::Vgm,
            extensions::VGZ => EmuType::Vgz,
            _ => panic!()
        }
    }

    /// Get a file extension from an `EmuType`
    pub fn to_extension(&self) -> &'static str {
        match self {
            EmuType::Ay => extensions::AY,
            EmuType::Gbs => extensions::GBS,
            EmuType::Gym => extensions::GYM,
            EmuType::Hes => extensions::HES,
            EmuType::Kss => extensions::KSS,
            EmuType::Nsf => extensions::NSF,
            EmuType::Nsfe => extensions::NSFE,
            EmuType::Sap => extensions::SAP,
            EmuType::Spc => extensions::SPC,
            EmuType::Vgm => extensions::VGM,
            EmuType::Vgz => extensions::VGZ
        }
    }
}

/// Holds a pointer to a `MusicEmu` instance in the C++ code. It automatically frees the instance
/// when dropped.
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