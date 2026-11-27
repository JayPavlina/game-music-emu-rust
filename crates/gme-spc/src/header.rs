use gme_common::track_info::{TrackInfo, copy_field};
use gme_core::endian::{get_le16, get_le32};

pub const HEADER_SIZE: usize = 0x100;
pub const SPC_MIN_FILE_SIZE: usize = 0x10180;
pub const SPC_FILE_SIZE: usize = 0x10200;

pub const SIGNATURE: &[u8] = b"SNES-SPC700 Sound File Data v0.30\x1A\x1A";
pub const SIGNATURE_CHECK_LEN: usize = 27;

#[derive(Clone, Debug)]
pub struct SpcHeader {
    pub tag: [u8; 35],
    pub format: u8,
    pub version: u8,
    pub pc: [u8; 2],
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub psw: u8,
    pub sp: u8,
    pub unused: [u8; 2],
    pub song: [u8; 32],
    pub game: [u8; 32],
    pub dumper: [u8; 16],
    pub comment: [u8; 32],
    pub date: [u8; 11],
    pub len_secs: [u8; 3],
    pub fade_msec: [u8; 4],
    pub author: [u8; 32],
    pub mute_mask: u8,
    pub emulator: u8,
    pub unused2: [u8; 46],
}

impl SpcHeader {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < HEADER_SIZE {
            return None;
        }
        let h = SpcHeader {
            tag: data[0..35].try_into().unwrap(),
            format: data[35],
            version: data[36],
            pc: [data[37], data[38]],
            a: data[39],
            x: data[40],
            y: data[41],
            psw: data[42],
            sp: data[43],
            unused: [data[44], data[45]],
            song: data[46..78].try_into().unwrap(),
            game: data[78..110].try_into().unwrap(),
            dumper: data[110..126].try_into().unwrap(),
            comment: data[126..158].try_into().unwrap(),
            date: data[158..169].try_into().unwrap(),
            len_secs: data[169..172].try_into().unwrap(),
            fade_msec: data[172..176].try_into().unwrap(),
            author: data[176..208].try_into().unwrap(),
            mute_mask: data[208],
            emulator: data[209],
            unused2: data[210..256].try_into().unwrap(),
        };
        Some(h)
    }

    pub fn pc(&self) -> u16 {
        self.pc[1] as u16 * 0x100 + self.pc[0] as u16
    }
}

pub fn check_spc_header(data: &[u8]) -> bool {
    if data.len() < SIGNATURE_CHECK_LEN {
        return false;
    }
    data[..SIGNATURE_CHECK_LEN] == SIGNATURE[..SIGNATURE_CHECK_LEN]
}

fn bytes_to_str(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s = &bytes[..end];
    std::str::from_utf8(s).unwrap_or("")
}

fn decode_length(tag: &[u8], header_author: &[u8]) -> i64 {
    let mut len_secs: i64 = 0;
    for (i, &b) in tag.iter().enumerate() {
        let n = b.wrapping_sub(b'0');
        if n > 9 {
            // ignore single-digit text lengths
            // (except if author field is present and begins at offset 1)
            // C++: if ( i == 1 && (h.author [0] || !h.author [1]) ) len_secs = 0;
            if i == 1 && (header_author[0] != 0 || header_author[1] == 0) {
                len_secs = 0;
            }
            break;
        }
        len_secs *= 10;
        len_secs += n as i64;
    }
    len_secs
}

fn get_spc_xid6(begin: &[u8], out: &mut TrackInfo) {
    let size = begin.len();
    if size < 8 || &begin[..4] != b"xid6" {
        return;
    }
    let info_size = get_le32(&begin[4..8]) as usize;
    let mut in_pos = 8;
    let end = size.min(info_size + 8);

    let mut year: i32 = 0;
    let mut copyright = String::new();

    while end - in_pos >= 4 {
        let id = begin[in_pos];
        let data = begin[in_pos + 3] as usize * 0x100 + begin[in_pos + 2] as usize;
        let type_byte = begin[in_pos + 1];
        let len = if type_byte != 0 { data } else { 0 };
        in_pos += 4;
        if len > end - in_pos {
            break;
        }

        let field = match id {
            0x01 => Some(&mut out.song),
            0x02 => Some(&mut out.game),
            0x03 => Some(&mut out.author),
            0x04 => Some(&mut out.dumper),
            0x07 => Some(&mut out.comment),
            0x13 => {
                let s = bytes_to_str(&begin[in_pos..in_pos + len]);
                copyright = s.to_string();
                None
            }
            0x14 => {
                year = data as i32;
                None
            }
            _ => None,
        };

        if let Some(f) = field {
            let s = bytes_to_str(&begin[in_pos..in_pos + len]);
            copy_field(f, s);
        }

        in_pos += len;

        // blocks are supposed to be 4-byte aligned with zero-padding...
        let unaligned = in_pos;
        while in_pos & 3 != 0 && in_pos < end {
            if begin[in_pos] != 0 {
                // ...but some files have no padding
                in_pos = unaligned;
                break;
            }
            in_pos += 1;
        }
    }

    if year != 0 {
        let year_str = format!("{:04} ", year);
        copyright = format!("{}{}", year_str, copyright);
    }
    if !copyright.is_empty() {
        copy_field(&mut out.copyright, &copyright);
    }
}

fn get_spc_info_from_header(h: &SpcHeader, xid6: &[u8], out: &mut TrackInfo) {
    let len_secs = decode_length(&h.len_secs, &h.author);
    let len_secs = if len_secs == 0 || len_secs > 0x1FFF {
        get_le16(&h.len_secs) as i64
    } else {
        len_secs
    };
    if len_secs < 0x1FFF {
        out.length = len_secs * 1000;
    }

    out.fade_length = decode_length(&h.fade_msec, &h.author);

    let offset = if h.author[0] < b' ' || (h.author[0].wrapping_sub(b'0')) <= 9 {
        1
    } else {
        0
    };
    let author_str = bytes_to_str(&h.author[offset..]);
    copy_field(&mut out.author, author_str);

    copy_field(&mut out.song, bytes_to_str(&h.song));
    copy_field(&mut out.game, bytes_to_str(&h.game));
    copy_field(&mut out.dumper, bytes_to_str(&h.dumper));
    copy_field(&mut out.comment, bytes_to_str(&h.comment));

    if !xid6.is_empty() {
        get_spc_xid6(xid6, out);
    }
}

pub fn get_spc_track_info(data: &[u8], out: &mut TrackInfo) {
    let header = match SpcHeader::from_bytes(data) {
        Some(h) => h,
        None => return,
    };
    let xid6_start = SPC_FILE_SIZE.min(data.len());
    let xid6 = if data.len() > xid6_start {
        &data[xid6_start..]
    } else {
        &[]
    };
    get_spc_info_from_header(&header, xid6, out);
}
