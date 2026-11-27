use gme_core::endian::get_le16;

pub const GBS_HEADER_SIZE: usize = 112;

/// GBS file header (112 bytes).
/// Matches C++ Gbs_Emu::header_t layout.
#[derive(Clone, Debug, Default)]
pub struct GbsHeader {
    pub vers: u8,
    pub track_count: u8,
    pub first_track: u8,
    pub load_addr: u16,
    pub init_addr: u16,
    pub play_addr: u16,
    pub stack_ptr: u16,
    pub timer_modulo: u8,
    pub timer_mode: u8,
    pub game: [u8; 32],
    pub author: [u8; 32],
    pub copyright: [u8; 32],
}

impl GbsHeader {
    /// Parse header from raw bytes. Returns None if magic is wrong.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < GBS_HEADER_SIZE {
            return None;
        }
        if &data[..3] != b"GBS" {
            return None;
        }
        Some(Self {
            vers: data[3],
            track_count: data[4],
            first_track: data[5],
            load_addr: get_le16(&data[6..]),
            init_addr: get_le16(&data[8..]),
            play_addr: get_le16(&data[0x0A..]),
            stack_ptr: get_le16(&data[0x0C..]),
            timer_modulo: data[0x0E],
            timer_mode: data[0x0F],
            game: {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(&data[0x10..0x30]);
                buf
            },
            author: {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(&data[0x30..0x50]);
                buf
            },
            copyright: {
                let mut buf = [0u8; 32];
                buf.copy_from_slice(&data[0x50..0x70]);
                buf
            },
        })
    }

    pub fn game_name_str(&self) -> String {
        String::from_utf8_lossy(&self.game)
            .trim_end_matches('\0')
            .to_string()
    }

    pub fn author_str(&self) -> String {
        String::from_utf8_lossy(&self.author)
            .trim_end_matches('\0')
            .to_string()
    }

    pub fn copyright_str(&self) -> String {
        String::from_utf8_lossy(&self.copyright)
            .trim_end_matches('\0')
            .to_string()
    }
}
