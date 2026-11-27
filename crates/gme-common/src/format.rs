// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of gme.cpp (2026-06-09 to 2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// Format type descriptor. Replaces C++ `gme_type_t_`.
#[derive(Clone, Debug)]
pub struct FormatInfo {
    pub system: &'static str,
    /// Non-zero for formats with a fixed number of tracks.
    pub track_count: i32,
    pub extension: &'static str,
    pub flags: i32,
}

/// Supported format types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
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
    Vgz,
}

impl Format {
    pub fn info(&self) -> FormatInfo {
        match self {
            Format::Ay => FormatInfo {
                system: "ZX Spectrum",
                track_count: 0,
                extension: "AY",
                flags: 1,
            },
            Format::Gbs => FormatInfo {
                system: "Game Boy",
                track_count: 0,
                extension: "GBS",
                flags: 1,
            },
            Format::Gym => FormatInfo {
                system: "Sega Genesis",
                track_count: 1,
                extension: "GYM",
                flags: 0,
            },
            Format::Hes => FormatInfo {
                system: "PC Engine",
                track_count: 0,
                extension: "HES",
                flags: 1,
            },
            Format::Kss => FormatInfo {
                system: "MSX",
                track_count: 0,
                extension: "KSS",
                flags: 3,
            },
            Format::Nsf => FormatInfo {
                system: "Nintendo NES",
                track_count: 0,
                extension: "NSF",
                flags: 1,
            },
            Format::Nsfe => FormatInfo {
                system: "Nintendo NES",
                track_count: 0,
                extension: "NSFE",
                flags: 1,
            },
            Format::Sap => FormatInfo {
                system: "Atari",
                track_count: 0,
                extension: "SAP",
                flags: 1,
            },
            Format::Spc => FormatInfo {
                system: "Super Nintendo",
                track_count: 1,
                extension: "SPC",
                flags: 0,
            },
            Format::Vgm => FormatInfo {
                system: "Sega Genesis",
                track_count: 1,
                extension: "VGM",
                flags: 1,
            },
            Format::Vgz => FormatInfo {
                system: "Sega Genesis",
                track_count: 1,
                extension: "VGZ",
                flags: 1,
            },
        }
    }

    pub fn system(&self) -> &'static str {
        self.info().system
    }

    pub fn extension(&self) -> &'static str {
        self.info().extension
    }

    pub fn is_multitrack(&self) -> bool {
        self.info().track_count == 0
    }

    /// List all available format types.
    pub fn all() -> &'static [Format] {
        &[
            Format::Ay,
            Format::Gbs,
            Format::Gym,
            Format::Hes,
            Format::Kss,
            Format::Nsf,
            Format::Nsfe,
            Format::Sap,
            Format::Spc,
            Format::Vgm,
            Format::Vgz,
        ]
    }

    /// Identify format from file header bytes.
    pub fn identify_header(header: &[u8]) -> Option<Format> {
        if header.len() < 4 {
            return None;
        }
        if &header[..2] == b"\x1f\x8b" {
            return Some(Format::Vgz); // gzip magic
        }
        match &header[..4] {
            b"NESM" => Some(Format::Nsf),
            b"NSFE" => Some(Format::Nsfe),
            b"SNES" => Some(Format::Spc), // "SNES-SPC700..."
            b"Vgm " => Some(Format::Vgm),
            b"GBS\x01" => Some(Format::Gbs),
            b"GYMX" => Some(Format::Gym),
            b"HESM" => Some(Format::Hes),
            b"KSCC" | b"KSSX" => Some(Format::Kss),
            b"ZXAY" => Some(Format::Ay),
            b"SAP\r" | b"SAP\n" => Some(Format::Sap),
            _ => None,
        }
    }

    /// Identify format from file extension.
    pub fn identify_extension(ext: &str) -> Option<Format> {
        let ext = ext.to_uppercase();
        match ext.as_str() {
            "AY" => Some(Format::Ay),
            "GBS" => Some(Format::Gbs),
            "GYM" => Some(Format::Gym),
            "HES" => Some(Format::Hes),
            "KSS" => Some(Format::Kss),
            "NSF" => Some(Format::Nsf),
            "NSFE" => Some(Format::Nsfe),
            "SAP" => Some(Format::Sap),
            "SPC" => Some(Format::Spc),
            "VGM" => Some(Format::Vgm),
            "VGZ" => Some(Format::Vgz),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_header_ay_prefix() {
        assert_eq!(Format::identify_header(b"ZXAY"), Some(Format::Ay));
    }

    #[test]
    fn test_identify_header_nsf() {
        let header = b"NESM\x1a\x01\x01";
        assert_eq!(Format::identify_header(header), Some(Format::Nsf));
    }

    #[test]
    fn test_identify_header_spc() {
        let header = b"SNES-SPC700 Sound File Data";
        assert_eq!(Format::identify_header(header), Some(Format::Spc));
    }

    #[test]
    fn test_identify_header_vgm() {
        let header = b"Vgm \x00\x00\x00\x00";
        assert_eq!(Format::identify_header(header), Some(Format::Vgm));
    }

    #[test]
    fn test_identify_extension() {
        assert_eq!(Format::identify_extension("nsf"), Some(Format::Nsf));
        assert_eq!(Format::identify_extension("SPC"), Some(Format::Spc));
        assert_eq!(Format::identify_extension("vgz"), Some(Format::Vgz));
        assert_eq!(Format::identify_extension("txt"), None);
    }

    #[test]
    fn test_all_formats() {
        assert_eq!(Format::all().len(), 11);
    }
}
