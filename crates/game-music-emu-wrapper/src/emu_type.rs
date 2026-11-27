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
    pub const AY: &str = "AY";
    pub const GBS: &str = "GBS";
    pub const GYM: &str = "GYM";
    pub const HES: &str = "HES";
    pub const KSS: &str = "KSS";
    pub const NSF: &str = "NSF";
    pub const NSFE: &str = "NSFE";
    pub const SAP: &str = "SAP";
    pub const SPC: &str = "SPC";
    pub const VGM: &str = "VGM";
    pub const VGZ: &str = "VGZ";
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
            _ => panic!(),
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
            EmuType::Vgz => extensions::VGZ,
        }
    }
}
