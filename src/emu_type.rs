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
    Vgz,
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