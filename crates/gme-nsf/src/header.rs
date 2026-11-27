use gme_core::endian::get_le16;

/// NSF file header (128 bytes).
#[derive(Clone, Debug)]
pub struct NsfHeader {
    pub load_addr: u16,
    pub init_addr: u16,
    pub play_addr: u16,
    pub game_name: [u8; 32],
    pub artist_name: [u8; 32],
    pub copyright: [u8; 32],
    pub ntsc_speed: u16,
    pub banks: [u8; 8],
    pub pal_speed: u16,
    pub region: u8,
    pub expansion: u8,
    pub extra_soundchips: u8,
    pub program_length: u32,
}

impl Default for NsfHeader {
    fn default() -> Self {
        Self {
            load_addr: 0x8000,
            init_addr: 0x8000,
            play_addr: 0x8000,
            game_name: [0; 32],
            artist_name: [0; 32],
            copyright: [0; 32],
            ntsc_speed: 0,
            banks: [0; 8],
            pal_speed: 0,
            region: 0,
            expansion: 0,
            extra_soundchips: 0,
            program_length: 0,
        }
    }
}

impl NsfHeader {
    pub const SIZE: usize = 128;
    pub const MAGIC: [u8; 5] = [b'N', b'E', b'S', b'M', 0x1a];

    /// Parse header from raw bytes. Returns None if magic is wrong.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        if data[..5] != Self::MAGIC {
            return None;
        }
        Some(Self {
            load_addr: get_le16(&data[0x08..]),
            init_addr: get_le16(&data[0x0A..]),
            play_addr: get_le16(&data[0x0C..]),
            game_name: {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(&data[0x0E..0x2E]);
                buf
            },
            artist_name: {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(&data[0x2E..0x4E]);
                buf
            },
            copyright: {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(&data[0x4E..0x6E]);
                buf
            },
            ntsc_speed: get_le16(&data[0x6E..]),
            banks: {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&data[0x70..0x78]);
                buf
            },
            pal_speed: get_le16(&data[0x78..]),
            region: data[0x7A],
            expansion: data[0x7B],
            extra_soundchips: data[0x7C],
            program_length: {
                // Actually 2 bytes at 0x7D, but stored as u32 for convenience
                get_le16(&data[0x7D..]) as u32
            },
        })
    }

    /// Whether this NSF uses PAL timing.
    /// Matches C++ logic: pal_only = (speed_flags & 3) == 1
    /// Only PAL-only (bit 0 set, bit 1 clear) returns true.
    /// Dual PAL/NTSC (both bits set) returns false (defaults to NTSC).
    pub fn is_pal(&self) -> bool {
        (self.region & 3) == 1
    }

    /// Chip flags byte. Matches the C++ Nsf_Emu header layout, which reads
    /// only byte 0x7B (`chip_flags`); bytes 0x7C-0x7F are `unused [4]` and
    /// must not influence chip creation (some files carry garbage there,
    /// e.g. assets/test.nsf has 0x82 at offset 0x7C).
    pub fn chip_flags(&self) -> u8 {
        self.expansion
    }

    /// Serialize this header back to 128 bytes (NSF format).
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[..5].copy_from_slice(&Self::MAGIC);
        buf[5] = 1; // version
        buf[0x08..0x0A].copy_from_slice(&self.load_addr.to_le_bytes());
        buf[0x0A..0x0C].copy_from_slice(&self.init_addr.to_le_bytes());
        buf[0x0C..0x0E].copy_from_slice(&self.play_addr.to_le_bytes());
        buf[0x0E..0x2E].copy_from_slice(&self.game_name);
        buf[0x2E..0x4E].copy_from_slice(&self.artist_name);
        buf[0x4E..0x6E].copy_from_slice(&self.copyright);
        buf[0x6E..0x70].copy_from_slice(&self.ntsc_speed.to_le_bytes());
        buf[0x70..0x78].copy_from_slice(&self.banks);
        buf[0x78..0x7A].copy_from_slice(&self.pal_speed.to_le_bytes());
        buf[0x7A] = self.region;
        buf[0x7B] = self.expansion;
        buf[0x7C] = self.extra_soundchips;
        buf[0x7D..0x7F].copy_from_slice(&(self.program_length as u16).to_le_bytes());
        buf
    }

    /// Speed/frame flags (upper bits of the region byte).
    /// Bit 4: 5-frame mode, Bit 5: skip APU init.
    pub fn speed_flags(&self) -> u8 {
        self.region
    }

    /// Get string fields as UTF-8 (lossy).
    pub fn game_name_str(&self) -> String {
        String::from_utf8_lossy(&self.game_name)
            .trim_end_matches('\0')
            .to_string()
    }

    pub fn artist_name_str(&self) -> String {
        String::from_utf8_lossy(&self.artist_name)
            .trim_end_matches('\0')
            .to_string()
    }

    pub fn copyright_str(&self) -> String {
        String::from_utf8_lossy(&self.copyright)
            .trim_end_matches('\0')
            .to_string()
    }
}
