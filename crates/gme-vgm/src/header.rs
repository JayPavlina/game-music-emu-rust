// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Vgm_Emu.cpp (2026-06-09), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_common::track_info::{TrackInfo, copy_field};
use gme_core::endian::get_le32;

pub const HEADER_SIZE: usize = 0x40;
pub const VGM_MIN_FILE_SIZE: usize = HEADER_SIZE + 4;
pub const GD3_HEADER_SIZE: usize = 12;

pub const VGM_RATE: i32 = 44100;

#[derive(Clone, Debug)]
pub struct VgmHeader {
    pub version: u32,
    pub psg_rate: u32,
    pub ym2413_rate: u32,
    pub gd3_offset: u32,
    pub track_duration: u32,
    pub loop_offset: u32,
    pub loop_duration: u32,
    pub noise_feedback: u16,
    pub noise_width: u8,
    pub ym2612_rate: u32,
    pub ym2151_rate: u32,
    pub data_offset: u32,
}

impl VgmHeader {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < HEADER_SIZE {
            return None;
        }
        if &data[..4] != b"Vgm " {
            return None;
        }
        Some(Self {
            version: get_le32(&data[8..]),
            psg_rate: get_le32(&data[0x0C..]),
            ym2413_rate: get_le32(&data[0x10..]),
            gd3_offset: get_le32(&data[0x14..]),
            track_duration: get_le32(&data[0x18..]),
            loop_offset: get_le32(&data[0x1C..]),
            loop_duration: get_le32(&data[0x20..]),
            noise_feedback: (data[0x28] as u16) | ((data[0x29] as u16) << 8),
            noise_width: data[0x2A],
            ym2612_rate: get_le32(&data[0x2C..]),
            ym2151_rate: get_le32(&data[0x30..]),
            data_offset: get_le32(&data[0x34..]),
        })
    }

    pub fn psg_rate(&self) -> u32 {
        self.psg_rate & 0x0FFF_FFFF
    }

    pub fn psg_dual(&self) -> bool {
        self.psg_rate & 0x4000_0000 != 0
    }

    pub fn psg_t6w28(&self) -> bool {
        self.psg_rate & 0x8000_0000 != 0
    }

    pub fn ym2612_dual(&self) -> bool {
        self.ym2612_rate & 0x4000_0000 != 0
    }

    pub fn ym2413_dual(&self) -> bool {
        self.ym2413_rate & 0x4000_0000 != 0
    }

    pub fn ym2612_rate_val(&self) -> u32 {
        self.ym2612_rate & 0x0FFF_FFFF
    }

    pub fn ym2413_rate_val(&self) -> u32 {
        self.ym2413_rate & 0x0FFF_FFFF
    }

    pub fn uses_fm(&self) -> bool {
        self.ym2612_rate_val() != 0 || self.ym2413_rate_val() != 0
    }
}

fn skip_gd3_str(mut in_pos: usize, end: usize, data: &[u8]) -> usize {
    while end - in_pos >= 2 {
        in_pos += 2;
        if data[in_pos - 2] == 0 && data[in_pos - 1] == 0 {
            break;
        }
    }
    in_pos
}

fn get_gd3_str(in_pos: usize, end: usize, data: &[u8]) -> (usize, String) {
    let mid = skip_gd3_str(in_pos, end, data);
    let len = (mid - in_pos) / 2;
    let mut result = String::new();
    if len > 0 {
        for i in 0..len {
            let lo = data[in_pos + i * 2];
            let hi = data[in_pos + i * 2 + 1];
            if hi != 0 {
                result.push('?');
            } else if lo != 0 {
                result.push(lo as char);
            }
        }
    }
    (mid, result)
}

fn get_gd3_pair(in_pos: usize, end: usize, data: &[u8]) -> (usize, String) {
    let (mid, s) = get_gd3_str(in_pos, end, data);
    let after = skip_gd3_str(mid, end, data);
    (after, s)
}

fn parse_gd3(data: &[u8], out: &mut TrackInfo) {
    let end = data.len();
    let (pos, song) = get_gd3_pair(0, end, data);
    let (pos, game) = get_gd3_pair(pos, end, data);
    let (pos, system) = get_gd3_pair(pos, end, data);
    let (pos, author) = get_gd3_pair(pos, end, data);
    let (pos, copyright) = get_gd3_str(pos, end, data);
    let (pos, dumper) = get_gd3_pair(pos, end, data);
    let (_pos, comment) = get_gd3_str(pos, end, data);

    copy_field(&mut out.song, &song);
    copy_field(&mut out.game, &game);
    copy_field(&mut out.system, &system);
    copy_field(&mut out.author, &author);
    copy_field(&mut out.copyright, &copyright);
    copy_field(&mut out.dumper, &dumper);
    copy_field(&mut out.comment, &comment);
}

fn check_gd3_header(data: &[u8], remain: usize) -> Option<usize> {
    if remain < GD3_HEADER_SIZE {
        return None;
    }
    if &data[..4] != b"Gd3 " {
        return None;
    }
    let version = get_le32(&data[4..]);
    if version >= 0x200 {
        return None;
    }
    let gd3_size = get_le32(&data[8..]) as usize;
    if gd3_size > remain - GD3_HEADER_SIZE {
        return None;
    }
    Some(gd3_size)
}

fn get_vgm_length(h: &VgmHeader, out: &mut TrackInfo) {
    let length = h.track_duration as i64 * 10 / 441;
    if length > 0 {
        let loop_dur = h.loop_duration as i64;
        if loop_dur > 0 && h.loop_offset != 0 {
            out.loop_length = loop_dur * 10 / 441;
            out.intro_length = length - out.loop_length;
        } else {
            out.length = length;
            out.intro_length = length;
            out.loop_length = 0;
        }
    }
}

pub fn get_vgm_track_info(data: &[u8], out: &mut TrackInfo) {
    let header = match VgmHeader::parse(data) {
        Some(h) => h,
        None => return,
    };

    out.system = "Sega Genesis".to_string();

    get_vgm_length(&header, out);

    let gd3_offset_raw = header.gd3_offset;
    if gd3_offset_raw > 0 {
        let gd3_offset = (gd3_offset_raw as usize).wrapping_sub(0x2C);
        if gd3_offset < data.len() {
            let gd3_start = HEADER_SIZE + gd3_offset;
            if gd3_start < data.len() {
                let remain = data.len() - gd3_start;
                let gd3_data = &data[gd3_start..];
                if let Some(gd3_size) = check_gd3_header(gd3_data, remain) {
                    let gd3_body = &gd3_data[GD3_HEADER_SIZE..GD3_HEADER_SIZE + gd3_size];
                    parse_gd3(gd3_body, out);
                }
            }
        }
    }
}

pub fn check_vgm_header(data: &[u8]) -> bool {
    data.len() >= 4 && &data[..4] == b"Vgm "
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_vgm_header() {
        assert!(check_vgm_header(b"Vgm \x00\x00\x00\x00"));
        assert!(!check_vgm_header(b"NSF\x1a"));
    }
}
