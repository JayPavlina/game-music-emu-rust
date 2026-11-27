// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Kss_Emu.cpp (2026-06-09), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! KSS file header parsing.
//! Matches C++ Kss_Emu::header_t and Kss_Emu::ext_header_t.

pub const KSS_HEADER_SIZE: usize = 0x10;
pub const KSS_EXT_HEADER_SIZE: usize = 0x10;

/// Basic KSS header (0x10 bytes).
#[derive(Clone, Debug, Default)]
pub struct KssHeader {
    pub tag: [u8; 4],
    pub load_addr: u16,
    pub load_size: u16,
    pub init_addr: u16,
    pub play_addr: u16,
    pub first_bank: u8,
    pub bank_mode: u8,
    pub extra_header: u8,
    pub device_flags: u8,
}

/// Extended KSS header (0x10 bytes, present when tag[3] == 'X').
#[derive(Clone, Debug, Default)]
pub struct KssExtHeader {
    pub data_size: u32,
    pub unused: [u8; 4],
    pub first_track: u16,
    pub last_track: u16,
    pub psg_vol: u8,
    pub scc_vol: u8,
    pub msx_music_vol: u8,
    pub msx_audio_vol: u8,
}

/// Combined header.
#[derive(Clone, Debug, Default)]
pub struct KssCompositeHeader {
    pub base: KssHeader,
    pub ext: KssExtHeader,
}

impl KssHeader {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < KSS_HEADER_SIZE {
            return None;
        }
        if &data[..4] != b"KSCC" && &data[..4] != b"KSSX" {
            return None;
        }
        Some(Self {
            tag: [data[0], data[1], data[2], data[3]],
            load_addr: u16::from_le_bytes([data[4], data[5]]),
            load_size: u16::from_le_bytes([data[6], data[7]]),
            init_addr: u16::from_le_bytes([data[8], data[9]]),
            play_addr: u16::from_le_bytes([data[10], data[11]]),
            first_bank: data[12],
            bank_mode: data[13],
            extra_header: data[14],
            device_flags: data[15],
        })
    }

    pub fn is_kssx(&self) -> bool {
        self.tag[3] == b'X'
    }

    /// Number of extra header bytes at the beginning of file data.
    pub fn extra_header_size(&self) -> usize {
        if self.is_kssx() {
            self.extra_header as usize
        } else {
            0
        }
    }

    /// Determine the system name from device_flags.
    pub fn system_name(&self) -> &'static str {
        if self.device_flags & 0x02 != 0 {
            if self.device_flags & 0x04 != 0 {
                "Game Gear"
            } else if self.device_flags & 0x01 != 0 {
                "Sega Mega Drive"
            } else {
                "Sega Master System"
            }
        } else {
            "MSX"
        }
    }

    /// Whether this file uses SCC sound chip.
    pub fn uses_scc(&self) -> bool {
        self.device_flags & 0x04 == 0
    }

    /// Whether this file uses SN76489 (SMS PSG).
    pub fn uses_sn76489(&self) -> bool {
        self.device_flags & 0x02 != 0
    }

    /// Whether this file uses FM sound (unsupported).
    pub fn uses_fm(&self) -> bool {
        self.device_flags & 0x09 != 0
    }

    /// Whether this is a PAL file (50 Hz).
    pub fn is_pal(&self) -> bool {
        self.device_flags & 0x40 != 0
    }
}

impl KssExtHeader {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < KSS_EXT_HEADER_SIZE {
            return None;
        }
        Some(Self {
            data_size: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            unused: [data[4], data[5], data[6], data[7]],
            first_track: u16::from_le_bytes([data[8], data[9]]),
            last_track: u16::from_le_bytes([data[10], data[11]]),
            psg_vol: data[12],
            scc_vol: data[13],
            msx_music_vol: data[14],
            msx_audio_vol: data[15],
        })
    }
}

impl KssCompositeHeader {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let base = KssHeader::parse(data)?;
        let ext = if base.is_kssx() && data.len() >= KSS_HEADER_SIZE + KSS_EXT_HEADER_SIZE {
            KssExtHeader::parse(&data[KSS_HEADER_SIZE..]).unwrap_or_default()
        } else {
            KssExtHeader::default()
        };
        Some(Self { base, ext })
    }
}
