/// SAP file header parser.
/// Matches C++ parse_info() in Sap_Emu.cpp.
use gme_core::error::{GmeError, GmeResult};

/// Parsed SAP file metadata.
#[derive(Clone, Debug)]
pub struct SapInfo {
    pub init_addr: i32,
    pub play_addr: i32,
    pub music_addr: i32,
    pub player_type: char,
    pub track_count: i32,
    pub fastplay: i32,
    pub stereo: bool,
    pub author: String,
    pub name: String,
    pub copyright: String,
    /// Byte offset where ROM data blocks begin (after the 0xFF 0xFF sentinel).
    pub rom_data_offset: usize,
}

impl Default for SapInfo {
    fn default() -> Self {
        Self {
            init_addr: -1,
            play_addr: -1,
            music_addr: -1,
            player_type: 'B',
            track_count: 1,
            fastplay: 312,
            stereo: false,
            author: String::new(),
            name: String::new(),
            copyright: String::new(),
            rom_data_offset: 0,
        }
    }
}

/// Parse a hex address (4 hex digits) from a byte slice.
/// Returns -1 if not valid hex.
fn from_hex(in_bytes: &[u8]) -> i32 {
    if in_bytes.len() < 4 {
        return -1;
    }
    let mut result: u32 = 0;
    for &b in in_bytes.iter().take(4) {
        let h = from_hex_char(b);
        if h > 15 {
            return -1;
        }
        result = result * 0x10 + h;
    }
    result as i32
}

/// Convert a hex character to its value (0-15). Returns >15 if not hex.
#[inline]
fn from_hex_char(h: u8) -> u32 {
    let mut h = h.wrapping_sub(0x30);
    if h > 9 {
        h = ((h.wrapping_sub(0x11)) & 0xDF).wrapping_add(10);
    }
    h as u32
}

/// Parse a decimal number from a byte slice.
fn from_dec(in_bytes: &[u8]) -> i32 {
    if in_bytes.is_empty() {
        return -1;
    }
    let mut n: i32 = 0;
    for &b in in_bytes {
        let dig = b.wrapping_sub(b'0');
        if dig > 9 {
            return -1;
        }
        n = n * 10 + dig as i32;
    }
    n
}

/// Parse a quoted string value.
fn parse_string(in_bytes: &[u8]) -> String {
    if in_bytes.is_empty() {
        return String::new();
    }
    let (start, end) = if in_bytes[0] == b'"' {
        let start = 1;
        let mut end = start;
        while end < in_bytes.len() && in_bytes[end] != b'"' {
            end += 1;
        }
        (start, end)
    } else {
        (0, in_bytes.len())
    };
    String::from_utf8_lossy(&in_bytes[start..end]).to_string()
}

/// Parse a SAP file header and extract metadata.
/// Returns the parsed info, or an error if the file is invalid.
pub fn parse_info(data: &[u8]) -> GmeResult<SapInfo> {
    let mut info = SapInfo::default();

    // Accept "SAP\r\n" or "SAP\n"
    let header_end = if data.len() >= 5 && data.starts_with(b"SAP\r\n") {
        5
    } else if data.len() >= 4 && data.starts_with(b"SAP\n") {
        4
    } else {
        return Err(GmeError::WrongFileType);
    };

    // Find the end of the header (0xFF 0xFF sentinel)
    let mut pos = header_end;

    while pos + 1 < data.len() {
        // Check for 0xFF 0xFF sentinel
        if data[pos] == 0xFF && data[pos + 1] == 0xFF {
            info.rom_data_offset = pos + 2;
            return Ok(info);
        }

        // Find end of line (0x0D or 0x0A)
        let line_start = pos;
        let mut line_end = pos;
        while line_end < data.len() && data[line_end] != 0x0D && data[line_end] != 0x0A {
            line_end += 1;
        }

        let line = &data[line_start..line_end];

        // Parse tag and value
        let mut tag_end = 0;
        while tag_end < line.len() && line[tag_end] > b' ' {
            tag_end += 1;
        }
        let tag = &line[..tag_end];

        // Skip whitespace between tag and value
        let mut val_start = tag_end;
        while val_start < line.len() && line[val_start] <= b' ' {
            val_start += 1;
        }
        let value = &line[val_start..];

        if tag.is_empty() {
            // skip
        } else if tag == b"INIT" {
            let addr = from_hex(value);
            if !(0..=0xFFFF).contains(&addr) {
                return Err(GmeError::Internal("Invalid init address"));
            }
            info.init_addr = addr;
        } else if tag == b"PLAYER" {
            let addr = from_hex(value);
            if !(0..=0xFFFF).contains(&addr) {
                return Err(GmeError::Internal("Invalid play address"));
            }
            info.play_addr = addr;
        } else if tag == b"MUSIC" {
            let addr = from_hex(value);
            if !(0..=0xFFFF).contains(&addr) {
                return Err(GmeError::Internal("Invalid music address"));
            }
            info.music_addr = addr;
        } else if tag == b"SONGS" {
            let count = from_dec(value);
            if count <= 0 {
                return Err(GmeError::Internal("Invalid track count"));
            }
            info.track_count = count;
        } else if tag == b"TYPE" {
            if !value.is_empty() {
                match value[0] as char {
                    'B' | 'C' => info.player_type = value[0] as char,
                    'D' => return Err(GmeError::Internal("Digimusic not supported")),
                    _ => return Err(GmeError::Internal("Unsupported player type")),
                }
            }
        } else if tag == b"STEREO" {
            info.stereo = true;
        } else if tag == b"FASTPLAY" {
            let fp = from_dec(value);
            if fp <= 0 {
                return Err(GmeError::Internal("Invalid fastplay value"));
            }
            info.fastplay = fp;
        } else if tag == b"AUTHOR" {
            info.author = parse_string(value);
        } else if tag == b"NAME" {
            info.name = parse_string(value);
        } else if tag == b"DATE" {
            info.copyright = parse_string(value);
        }

        // Advance past line ending (CRLF or LF)
        pos = line_end;
        if pos < data.len() && data[pos] == 0x0D {
            pos += 1;
        }
        if pos < data.len() && data[pos] == 0x0A {
            pos += 1;
        }
    }

    // If we get here, no sentinel was found
    Err(GmeError::Internal("ROM data missing"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_sap() {
        let mut data = b"SAP\r\n".to_vec();
        data.extend_from_slice(b"TYPE B\r\n");
        data.extend_from_slice(b"INIT 1000\r\n");
        data.extend_from_slice(b"PLAYER 2000\r\n");
        data.extend_from_slice(b"SONGS 3\r\n");
        data.extend_from_slice(b"FASTPLAY 312\r\n");
        data.extend_from_slice(&[0xFF, 0xFF]);
        // Some dummy ROM data: start=$4000, end=$400F
        data.extend_from_slice(&[0x00, 0x40, 0x0F, 0x40]);
        data.extend_from_slice(&[0u8; 16]);

        let info = parse_info(&data).unwrap();
        assert_eq!(info.player_type, 'B');
        assert_eq!(info.init_addr, 0x1000);
        assert_eq!(info.play_addr, 0x2000);
        assert_eq!(info.track_count, 3);
        assert_eq!(info.fastplay, 312);
        assert!(!info.stereo);
    }

    #[test]
    fn test_parse_stereo() {
        let mut data = b"SAP\r\n".to_vec();
        data.extend_from_slice(b"TYPE B\r\n");
        data.extend_from_slice(b"INIT 1000\r\n");
        data.extend_from_slice(b"STEREO\r\n");
        data.extend_from_slice(&[0xFF, 0xFF]);

        let info = parse_info(&data).unwrap();
        assert!(info.stereo);
    }

    #[test]
    fn test_parse_type_c() {
        let mut data = b"SAP\r\n".to_vec();
        data.extend_from_slice(b"TYPE C\r\n");
        data.extend_from_slice(b"INIT 1000\r\n");
        data.extend_from_slice(b"PLAYER 2000\r\n");
        data.extend_from_slice(b"MUSIC 3000\r\n");
        data.extend_from_slice(&[0xFF, 0xFF]);

        let info = parse_info(&data).unwrap();
        assert_eq!(info.player_type, 'C');
        assert_eq!(info.music_addr, 0x3000);
    }

    #[test]
    fn test_parse_metadata() {
        let mut data = b"SAP\r\n".to_vec();
        data.extend_from_slice(b"AUTHOR \"Test Author\"\r\n");
        data.extend_from_slice(b"NAME \"Test Song\"\r\n");
        data.extend_from_slice(b"DATE \"2024\"\r\n");
        data.extend_from_slice(b"TYPE B\r\n");
        data.extend_from_slice(b"INIT 1000\r\n");
        data.extend_from_slice(&[0xFF, 0xFF]);

        let info = parse_info(&data).unwrap();
        assert_eq!(info.author, "Test Author");
        assert_eq!(info.name, "Test Song");
        assert_eq!(info.copyright, "2024");
    }
}
