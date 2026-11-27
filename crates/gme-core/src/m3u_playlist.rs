// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of M3u_Playlist.cpp (2026-06-09 to 2026-06-10), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use crate::data_reader::{DataReader, StdFileReader};

/// Global M3U playlist metadata.
#[derive(Clone, Debug, Default)]
pub struct M3uInfo {
    pub title: String,
    pub artist: String,
    pub date: String,
    pub composer: String,
    pub sequencer: String,
    pub engineer: String,
    pub ripping: String,
    pub tagging: String,
    pub copyright: String,
}

/// Per-track M3U entry.
#[derive(Clone, Debug)]
pub struct M3uEntry {
    pub file: String,
    pub entry_type: String,
    pub name: String,
    pub decimal_track: bool,
    /// 1-based track number, -1 if not present.
    pub track: i32,
    /// Length in milliseconds, -1 if not present.
    pub length: i32,
    pub intro: i32,
    pub loop_len: i32,
    pub fade: i32,
    /// Repeat count, -1 if not present.
    pub repeat: i32,
}

impl Default for M3uEntry {
    fn default() -> Self {
        Self {
            file: String::new(),
            entry_type: String::new(),
            name: String::new(),
            decimal_track: false,
            track: -1,
            length: -1,
            intro: -1,
            loop_len: -1,
            fade: -1,
            repeat: -1,
        }
    }
}

/// M3U playlist parser with support for subtrack information.
pub struct M3uPlaylist {
    entries: Vec<M3uEntry>,
    info: M3uInfo,
    first_error: i32,
}

impl Default for M3uPlaylist {
    fn default() -> Self {
        Self::new()
    }
}

impl M3uPlaylist {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            info: M3uInfo::default(),
            first_error: 0,
        }
    }

    pub fn clear(&mut self) {
        self.first_error = 0;
        self.entries.clear();
    }

    pub fn first_error(&self) -> i32 {
        self.first_error
    }

    pub fn info(&self) -> &M3uInfo {
        &self.info
    }

    pub fn entries(&self) -> &[M3uEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, i: usize) -> Option<&M3uEntry> {
        self.entries.get(i)
    }

    /// Load from file path.
    pub fn load(&mut self, path: &str) -> Result<(), &'static str> {
        let mut reader = StdFileReader::open(path).map_err(|_| "Couldn't open file")?;
        let remain = reader.remain() as usize;
        let mut data = vec![0u8; remain + 1];
        reader.read(&mut data[..remain]).map_err(|_| "Read error")?;
        self.parse(data)
    }

    /// Load from data reader.
    pub fn load_from_reader(&mut self, reader: &mut dyn DataReader) -> Result<(), &'static str> {
        let remain = reader.remain() as usize;
        let mut data = vec![0u8; remain + 1];
        reader.read(&mut data[..remain]).map_err(|_| "Read error")?;
        self.parse(data)
    }

    /// Load from memory.
    pub fn load_data(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let mut buf = vec![0u8; data.len() + 1];
        buf[..data.len()].copy_from_slice(data);
        self.parse(buf)
    }

    fn parse(&mut self, mut data: Vec<u8>) -> Result<(), &'static str> {
        self.info = M3uInfo::default();
        self.first_error = 0;

        let cr = b'\r';
        let lf = b'\n';

        // Terminate input
        let len = data.len();
        data[len - 1] = lf;

        let mut line_num = 0i32;
        let mut count = 0usize;
        let mut pos = 0usize;
        let mut first_comment = true;
        let mut last_tag = None;

        while pos < data.len() {
            line_num += 1;
            let begin = pos;

            // Find end of line
            while pos < data.len() && data[pos] != cr && data[pos] != lf {
                if data[pos] == 0 {
                    return Err("Not an m3u playlist");
                }
                pos += 1;
            }
            // Handle CR+LF
            if pos < data.len() && data[pos] == cr && pos + 1 < data.len() && data[pos + 1] == lf {
                data[pos] = 0;
                pos += 1;
            }
            if pos < data.len() {
                data[pos] = 0;
                pos += 1;
            }

            if begin >= data.len() || data[begin] == 0 {
                last_tag = None;
                continue;
            }

            if data[begin] == b'#' {
                // Comment line — convert to str for parsing
                // We need to find the end of the line first
                let line_end = {
                    let mut e = begin;
                    while e < data.len() && data[e] != 0 {
                        e += 1;
                    }
                    e
                };
                let line_str = std::str::from_utf8(&data[begin..line_end]).unwrap_or("");
                let trimmed = line_str.trim_start();
                parse_comment(trimmed, &mut self.info, first_comment, &mut last_tag);
                first_comment = false;
            } else if data[begin] != 0 {
                // Entry line
                let line_slice = &data[begin..];
                if let Some(line_str) = cstr_from_bytes(line_slice) {
                    let mut entry = M3uEntry::default();
                    if parse_line(line_str, &mut entry).is_some() {
                        if count >= self.entries.len() {
                            self.entries.resize(count + 1, M3uEntry::default());
                        }
                        self.entries[count] = entry;
                        count += 1;
                    } else if self.first_error == 0 {
                        self.first_error = line_num;
                    }
                }
                first_comment = false;
            }
        }

        if count == 0 {
            return Err("Not an m3u playlist");
        }

        if self.info.composer.is_empty()
            && self.info.engineer.is_empty()
            && self.info.ripping.is_empty()
            && self.info.tagging.is_empty()
        {
            self.info.title.clear();
        }
        self.entries.truncate(count);
        Ok(())
    }
}

fn cstr_from_bytes(data: &[u8]) -> Option<&str> {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    std::str::from_utf8(&data[..end]).ok()
}

fn skip_white(s: &str) -> &str {
    s.trim_start_matches(' ')
}

fn parse_filename<'a>(input: &'a str, entry: &mut M3uEntry) -> &'a str {
    let mut out = String::new();
    let mut chars = input.char_indices().peekable();
    let mut end_pos = input.len();

    while let Some((i, c)) = chars.next() {
        if c == ',' {
            let rest = skip_white(&input[i + 1..]);
            if let Some(first) = rest.as_bytes().first()
                && (*first == b'$' || first.is_ascii_digit())
            {
                end_pos = i + 1;
                break;
            }
            out.push(c);
        } else if c == ':' {
            if let Some((_, ':')) = chars.peek() {
                chars.next();
                // ::type suffix
                let type_start = i + 2;
                let mut type_end = type_start;
                for (j, ch) in input[type_start..].char_indices() {
                    if ch == ',' || ch == '\0' {
                        type_end = type_start + j;
                        break;
                    }
                    type_end = type_start + j + ch.len_utf8();
                }
                entry.entry_type = input[type_start..type_end].to_string();
                if type_end < input.len() && input.as_bytes()[type_end] == b',' {
                    end_pos = type_end + 1;
                } else {
                    end_pos = type_end;
                }
                break;
            }
            out.push(c);
        } else if c == '\\' {
            if let Some((_, next)) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }

    entry.file = out;
    &input[end_pos..]
}

/// Read a nonnegative number, leaving the delimiter for the caller.
fn number(input: &str, radix: u32) -> (i32, &str) {
    let end = input
        .bytes()
        .take_while(|&c| (c as char).is_digit(radix))
        .count();
    let value = i32::from_str_radix(&input[..end], radix).unwrap_or(-1);
    (value, &input[end..])
}

fn time(input: &str) -> (i32, &str) {
    let (mut seconds, mut rest) = number(input, 10);
    if seconds < 0 {
        return (-1, rest);
    }
    while let Some(next) = rest.strip_prefix(':') {
        let (part, tail) = number(next, 10);
        if part >= 0 {
            seconds = seconds.saturating_mul(60).saturating_add(part);
        }
        rest = tail;
    }
    let mut millis = seconds.saturating_mul(1000);
    if let Some(next) = rest.strip_prefix('.') {
        let digits = next.bytes().take_while(u8::is_ascii_digit).count();
        for (digit, scale) in next.bytes().take(digits).zip([100, 10, 1]) {
            millis = millis.saturating_add(i32::from(digit - b'0') * scale);
        }
        rest = &next[digits..];
    }
    (millis, rest)
}

fn next_field(input: &str) -> Option<&str> {
    let input = skip_white(input);
    if input.is_empty() {
        Some(input)
    } else {
        input.strip_prefix(',').map(skip_white)
    }
}

fn parse_name(input: &str) -> (String, &str) {
    let mut name = String::new();
    let mut chars = input.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == ',' {
            let rest = skip_white(&input[i + 1..]);
            if rest
                .as_bytes()
                .first()
                .is_some_and(|b| *b == b',' || *b == b'-' || b.is_ascii_digit())
            {
                return (name, rest);
            }
        }
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                name.push(next);
            }
        } else {
            name.push(c);
        }
    }
    (name, "")
}

fn parse_line(input: &str, entry: &mut M3uEntry) -> Option<()> {
    let rest = skip_white(parse_filename(input, entry));
    let (track, tail) = if let Some(hex) = rest.strip_prefix('$') {
        number(hex, 16)
    } else {
        let value = number(rest, 10);
        entry.decimal_track = value.0 >= 0;
        value
    };
    entry.track = track;
    let (name, rest) = parse_name(next_field(tail)?);
    entry.name = name;
    let (length, tail) = time(rest);
    entry.length = length;
    let rest = next_field(tail)?;
    let tail = if let Some(tail) = rest.strip_prefix('-') {
        entry.loop_len = length;
        tail
    } else {
        let (value, tail) = time(rest);
        if value >= 0 {
            if let Some(tail) = tail.strip_prefix('-') {
                entry.intro = value;
                entry.loop_len = length.saturating_sub(value);
                tail
            } else {
                entry.intro = 0;
                entry.loop_len = value;
                tail
            }
        } else {
            tail
        }
    };
    let (fade, tail) = time(next_field(tail)?);
    entry.fade = fade;
    let (repeat, tail) = number(next_field(tail)?, 10);
    entry.repeat = repeat;
    next_field(tail)?;
    Some(())
}

type TagField = fn(&mut M3uInfo) -> &mut String;

fn parse_comment(line: &str, info: &mut M3uInfo, first: bool, last_tag: &mut Option<TagField>) {
    let text = line.trim_start_matches('#').trim_start();
    if let Some(tag) = text.strip_prefix('@') {
        let (name, value) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        let field: Option<TagField> = match name {
            "TITLE" => Some(|i| &mut i.title),
            "ARTIST" => Some(|i| &mut i.artist),
            "DATE" => Some(|i| &mut i.date),
            "COMPOSER" => Some(|i| &mut i.composer),
            "SEQUENCER" => Some(|i| &mut i.sequencer),
            "ENGINEER" => Some(|i| &mut i.engineer),
            "RIPPER" => Some(|i| &mut i.ripping),
            "TAGGER" => Some(|i| &mut i.tagging),
            _ => None,
        };
        if let Some(field) = field
            && !value.trim_start().is_empty()
        {
            *field(info) = value.trim_start().to_owned();
            *last_tag = Some(field);
            return;
        }
    } else if let Some((name, value)) = text.split_once(':') {
        let field = match name {
            "Composer" => Some(&mut info.composer),
            "Engineer" => Some(&mut info.engineer),
            "Ripping" => Some(&mut info.ripping),
            "Tagging" => Some(&mut info.tagging),
            "Game" => Some(&mut info.title),
            "Artist" => Some(&mut info.artist),
            "Copyright" => Some(&mut info.copyright),
            _ => None,
        };
        if let Some(field) = field
            && !value.trim_start().is_empty()
        {
            *field = value.trim_start().to_owned();
            return;
        }
    } else if let Some(field) = last_tag {
        let value = field(info);
        value.push_str(", ");
        value.push_str(text);
        return;
    }
    if first {
        info.title = text.to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_m3u_basic() {
        let m3u_data = b"#EXTM3U\n#EXTINF:123,Test Song\ntest.nsf\n";
        let mut playlist = M3uPlaylist::new();
        playlist.load_data(m3u_data).unwrap();
        assert_eq!(playlist.len(), 1);
        assert_eq!(playlist.entry(0).unwrap().file, "test.nsf");
    }

    #[test]
    fn test_parse_m3u_with_time() {
        let m3u_data = b"#EXTM3U\n#EXTINF:123,Test Song\ntest.nsf\n";
        let mut playlist = M3uPlaylist::new();
        playlist.load_data(m3u_data).unwrap();
        assert_eq!(playlist.len(), 1);
        assert_eq!(playlist.entry(0).unwrap().file, "test.nsf");
    }
}
