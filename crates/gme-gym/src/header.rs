//! GYM file header parsing and track info, ported 1:1 from `Gym_Emu.cpp`
//! (header layout, `check_header`, `gym_track_length`, `get_gym_info`).

use gme_common::track_info::{TrackInfo, copy_field};
use gme_core::endian::get_le32;
use gme_core::error::{GmeError, GmeResult};

/// C++ `Gym_Emu::header_size`
pub const HEADER_SIZE: usize = 428;

/// C++ `gym_rate`: frames per second
pub const GYM_RATE: i64 = 60;

// Field offsets within the 428-byte header (C++ `Gym_Emu::header_t`)
const TAG: usize = 0; // char [4]
const SONG: usize = 4; // char [32]
const GAME: usize = 36; // char [32]
const COPYRIGHT: usize = 68; // char [32]
#[allow(dead_code)]
const EMULATOR: usize = 100; // char [32]
const DUMPER: usize = 132; // char [32]
const COMMENT: usize = 164; // char [256]
const LOOP_START: usize = 420; // byte [4], in 1/60 seconds, 0 if not looped
const PACKED: usize = 424; // byte [4]

/// Raw GYM header. Zeroed when the file has no GYMX header (C++ memsets
/// `header_` in that case).
#[derive(Clone)]
pub struct GymHeader {
    pub raw: [u8; HEADER_SIZE],
}

impl Default for GymHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl GymHeader {
    pub fn new() -> Self {
        Self {
            raw: [0; HEADER_SIZE],
        }
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        let mut raw = [0; HEADER_SIZE];
        raw.copy_from_slice(&data[..HEADER_SIZE]);
        Self { raw }
    }

    pub fn tag(&self) -> &[u8] {
        &self.raw[TAG..TAG + 4]
    }

    pub fn loop_start(&self) -> u32 {
        get_le32(&self.raw[LOOP_START..])
    }
}

/// NUL-terminated C string from a fixed-size field.
fn field_str(bytes: &[u8]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).into_owned()
}

/// C++ `check_header`: validates the file and returns the command data offset
/// (HEADER_SIZE for GYMX files, 0 for raw logs).
pub fn check_header(data: &[u8]) -> GmeResult<usize> {
    if data.len() < 4 {
        return Err(GmeError::WrongFileType);
    }

    if &data[..4] == b"GYMX" {
        if data.len() < HEADER_SIZE + 1 {
            return Err(GmeError::WrongFileType);
        }
        // The vendored C++ build has no zlib (HAVE_ZLIB_H disabled), so
        // packed GYM files are always rejected with this exact error.
        if data[PACKED..PACKED + 4] != [0, 0, 0, 0] {
            return Err(GmeError::Internal("Packed GYM file not supported"));
        }
        Ok(HEADER_SIZE)
    } else if data[0] > 3 {
        Err(GmeError::WrongFileType)
    } else {
        Ok(0)
    }
}

/// C++ `gym_track_length`: number of 1/60-second frames in the command data.
pub fn gym_track_length(data: &[u8]) -> i64 {
    let mut time: i64 = 0;
    let mut p = 0usize;
    while p < data.len() {
        let cmd = data[p];
        p += 1;
        match cmd {
            0 => time += 1,
            1 | 2 => p += 2,
            3 => p += 1,
            _ => {}
        }
    }
    time
}

/// C++ `get_gym_info`
pub fn get_gym_info(h: &GymHeader, length_frames: i64, out: &mut TrackInfo) {
    if h.tag() != b"GYMX" {
        return;
    }

    let length = length_frames * 50 / 3; // 1000 / 60
    let loop_start = h.loop_start() as i64;
    if loop_start != 0 {
        out.intro_length = loop_start * 50 / 3;
        out.loop_length = length - out.intro_length;
    } else {
        out.length = length;
        out.intro_length = length; // make it clear that track is no longer than length
        out.loop_length = 0;
    }

    // more stupidity where the field should have been left blank (from C++)
    let song = field_str(&h.raw[SONG..SONG + 32]);
    if song != "Unknown Song" {
        copy_field(&mut out.song, &song);
    }
    let game = field_str(&h.raw[GAME..GAME + 32]);
    if game != "Unknown Game" {
        copy_field(&mut out.game, &game);
    }
    let copyright = field_str(&h.raw[COPYRIGHT..COPYRIGHT + 32]);
    if copyright != "Unknown Publisher" {
        copy_field(&mut out.copyright, &copyright);
    }
    let dumper = field_str(&h.raw[DUMPER..DUMPER + 32]);
    if dumper != "Unknown Person" {
        copy_field(&mut out.dumper, &dumper);
    }
    let comment = field_str(&h.raw[COMMENT..COMMENT + 256]);
    if comment != "Header added by YMAMP" {
        copy_field(&mut out.comment, &comment);
    }
}
