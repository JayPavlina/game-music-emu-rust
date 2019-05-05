use crate::native::MusicEmu;
use core::borrow::Borrow;
use std::mem::transmute_copy;
use std::intrinsics::transmute;
use std::sync::Arc;
use crate::native;
use crate::wrapper::GmeResult;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum GmeType {
    Nsf,
    Nsfe,
    Kss,
}

mod extensions {
    pub const NSF: &'static str = "NSF";
    pub const NSFE: &'static str = "NSFE";
    pub const KSS: &'static str = "KSS";
}

impl GmeType {
    pub fn from_extension(value: &str) -> GmeType {
        match value {
            extensions::NSF => GmeType::Nsf,
            extensions::NSFE => GmeType::Nsfe,
            extensions::KSS => GmeType::Kss,
            _ => { panic!() }
        }
    }

    pub fn to_extension(&self) -> &'static str {
        match self {
            GmeType::Nsf => extensions::NSF,
            GmeType::Nsfe => extensions::NSFE,
            GmeType::Kss => extensions::KSS,
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