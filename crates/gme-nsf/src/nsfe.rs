// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2005-2006 Shay Green
// Rust adaptation of Nsfe_Emu.cpp (2026-06-09), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::endian::get_le32;
use gme_core::error::{GmeError, GmeResult};

use crate::header::NsfHeader;

/// Parsed NSFE metadata.
pub struct NsfeInfo {
    pub header: NsfHeader,
    pub actual_track_count: i32,
    pub first_track: i32,
    pub game: String,
    pub author: String,
    pub copyright: String,
    pub dumper: String,
    pub track_times: Vec<i32>,
    pub track_names: Vec<String>,
    pub playlist: Vec<u8>,
    pub data_offset: usize,
    pub data_size: usize,
}

impl NsfeInfo {
    pub fn parse(data: &[u8]) -> GmeResult<Self> {
        if data.len() < 4 || &data[..4] != b"NSFE" {
            return Err(GmeError::WrongFileType);
        }

        let mut header = NsfHeader {
            ntsc_speed: 0x411A,
            pal_speed: 0x4E20,
            ..Default::default()
        };

        let mut actual_track_count: i32 = 1;
        let mut first_track: i32 = 0;
        let mut game = String::new();
        let mut author = String::new();
        let mut copyright = String::new();
        let mut dumper = String::new();
        let mut track_times = Vec::new();
        let mut track_names = Vec::new();
        let mut playlist = Vec::new();
        let mut data_offset: usize = 0;
        let mut data_size: usize = 0;

        let mut phase: i32 = 0;
        let mut offset: usize = 4;

        while offset < data.len() {
            if data.len() - offset < 8 {
                return Err(GmeError::CorruptFile);
            }

            let size = get_le32(&data[offset..]) as usize;
            let tag_bytes: [u8; 4] = [
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ];
            let tag = u32::from_le_bytes(tag_bytes);
            offset += 8;

            if data.len() - offset < size {
                return Err(GmeError::CorruptFile);
            }

            let chunk_data = &data[offset..offset + size];

            match tag {
                // INFO
                0x4F464E49 => {
                    if phase != 0 {
                        return Err(GmeError::CorruptFile);
                    }
                    if size < 8 {
                        return Err(GmeError::CorruptFile);
                    }
                    header.load_addr = u16::from_le_bytes([chunk_data[0], chunk_data[1]]);
                    header.init_addr = u16::from_le_bytes([chunk_data[2], chunk_data[3]]);
                    header.play_addr = u16::from_le_bytes([chunk_data[4], chunk_data[5]]);
                    header.region = chunk_data[6];
                    header.expansion = chunk_data[7];
                    if size > 8 {
                        actual_track_count = chunk_data[8] as i32;
                    }
                    if size > 9 {
                        first_track = chunk_data[9] as i32;
                    }
                    phase = 1;
                }
                // BANK
                0x4B4E4142 => {
                    let len = size.min(8);
                    header.banks[..len].copy_from_slice(&chunk_data[..len]);
                }
                // auth
                0x68747561 => {
                    let strs = split_cstrs(chunk_data);
                    if !strs.is_empty() {
                        game = strs[0].clone();
                    }
                    if strs.len() > 1 {
                        author = strs[1].clone();
                    }
                    if strs.len() > 2 {
                        copyright = strs[2].clone();
                    }
                    if strs.len() > 3 {
                        dumper = strs[3].clone();
                    }
                }
                // time
                0x656D6974 => {
                    track_times.clear();
                    for i in 0..size / 4 {
                        let val = get_le32(&chunk_data[i * 4..]) as i32;
                        track_times.push(val);
                    }
                }
                // tlbl
                0x6C626C74 => {
                    track_names = split_cstrs(chunk_data);
                }
                // plst
                0x74736C70 => {
                    playlist = chunk_data.to_vec();
                }
                // DATA
                0x41544144 => {
                    phase = 2;
                    data_offset = offset;
                    data_size = size;
                }
                // NEND
                0x444E454E => {
                    phase = 3;
                    break;
                }
                _ => {
                    // Lowercase-first tags are silently skipped
                    let first_byte = tag_bytes[0];
                    if first_byte.is_ascii_lowercase() {
                        // skip
                    } else {
                        return Err(GmeError::CorruptFile);
                    }
                }
            }

            offset += size;
        }

        if phase < 2 {
            return Err(GmeError::CorruptFile);
        }

        Ok(Self {
            header,
            actual_track_count,
            first_track,
            game,
            author,
            copyright,
            dumper,
            track_times,
            track_names,
            playlist,
            data_offset,
            data_size,
        })
    }

    /// Remap a 0-based user track index to the actual NSF subtrack.
    pub fn remap_track(&self, track: i32) -> i32 {
        if !self.playlist.is_empty() && (track as usize) < self.playlist.len() {
            self.playlist[track as usize] as i32
        } else {
            track
        }
    }

    /// Number of tracks to expose to the user.
    pub fn track_count(&self) -> i32 {
        if !self.playlist.is_empty() {
            self.playlist.len() as i32
        } else {
            self.actual_track_count
        }
    }
}

fn read_cstr(data: &[u8]) -> &str {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    std::str::from_utf8(&data[..end]).unwrap_or("")
}

fn split_cstrs(data: &[u8]) -> Vec<String> {
    let mut result = Vec::new();
    let mut start = 0;
    for i in 0..data.len() {
        if data[i] == 0 {
            if i > start {
                result.push(read_cstr(&data[start..i]).to_string());
            } else {
                result.push(String::new());
            }
            start = i + 1;
        }
    }
    if start < data.len() {
        result.push(read_cstr(&data[start..]).to_string());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_test_nsfe() {
        let data = std::fs::read("../../assets/test.nsfe")
            .or_else(|_| std::fs::read("assets/test.nsfe"))
            .unwrap();
        let info = NsfeInfo::parse(&data).unwrap();
        assert_eq!(info.actual_track_count, 1);
        assert_eq!(info.first_track, 0);
        assert!(info.data_size > 0);
        assert_eq!(info.header.load_addr, 0x8000);
        assert_eq!(info.header.init_addr, 0x8000);
    }

    #[test]
    fn parse_fme7_nsfe() {
        let data = std::fs::read("../../assets/expansion/fme7.nsfe")
            .or_else(|_| std::fs::read("assets/expansion/fme7.nsfe"))
            .unwrap();
        let info = NsfeInfo::parse(&data).unwrap();
        assert!(info.actual_track_count > 0);
        assert!(info.data_size > 0);
        assert_eq!(info.track_names, ["Sweep"]);
    }
}
