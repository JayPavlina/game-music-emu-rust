// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Gme_File.cpp (2026-06-09 to 2026-06-10), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::data_reader::{DataReader, RemainingReader, StdFileReader};
use gme_core::error::{GmeError, GmeResult};
use gme_core::m3u_playlist::M3uPlaylist;

use crate::format::Format;
use crate::track_info::{TrackInfo, copy_field};

/// Trait for game music file handlers.
/// Replaces C++ `Gme_File` struct.
pub trait GmeFile {
    /// Get the format type for this emulator.
    fn format(&self) -> Format;

    /// Get raw track count (before M3U remapping).
    fn raw_track_count(&self) -> i32;

    /// Set the raw track count.
    fn set_raw_track_count(&mut self, n: i32);

    /// Load from memory data. Each format implements this.
    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult;

    /// Get track info for a specific track. Each format implements this.
    fn track_info_impl(&self, info: &mut TrackInfo, track: i32) -> GmeResult;

    /// Called before loading. Override for custom cleanup.
    fn pre_load(&mut self) {}

    /// Called after successful loading.
    fn post_load(&mut self) {}

    /// Called to clear internal playlist (e.g. NSFE).
    fn clear_playlist_impl(&mut self) {}
}

/// State that all GME file handlers share.
pub struct GmeFileState {
    pub track_count: i32,
    pub raw_track_count: i32,
    pub warning: Option<&'static str>,
    pub file_data: Vec<u8>,
    pub tracks: Vec<i64>,
    pub playlist: M3uPlaylist,
    pub playlist_warning: String,
}

impl Default for GmeFileState {
    fn default() -> Self {
        Self::new()
    }
}

impl GmeFileState {
    pub fn new() -> Self {
        Self {
            track_count: 0,
            raw_track_count: 0,
            warning: None,
            file_data: Vec::new(),
            tracks: Vec::new(),
            playlist: M3uPlaylist::new(),
            playlist_warning: String::new(),
        }
    }

    pub fn unload(&mut self) {
        self.clear_playlist_state();
        self.track_count = 0;
        self.raw_track_count = 0;
        self.file_data.clear();
        self.tracks.clear();
    }

    pub fn clear_playlist_state(&mut self) {
        self.playlist.clear();
        self.track_count = self.raw_track_count;
    }

    pub fn clear_playlist_with_emu(&mut self, emu: &mut dyn GmeFile) {
        self.playlist.clear();
        emu.clear_playlist_impl();
        self.track_count = self.raw_track_count;
    }

    pub fn set_track_count(&mut self, n: i32) {
        self.track_count = n;
        self.raw_track_count = n;
    }

    pub fn set_warning(&mut self, s: &'static str) {
        self.warning = Some(s);
    }

    pub fn warning(&mut self) -> Option<&'static str> {
        self.warning.take()
    }

    pub fn track_pos(&self, i: usize) -> &[u8] {
        let start = self.tracks[i] as usize;
        let end = self.tracks[i + 1] as usize;
        &self.file_data[start..end]
    }

    pub fn track_size(&self, i: usize) -> i64 {
        self.tracks[i + 1] - self.tracks[i]
    }

    /// Remap a track index through the M3U playlist.
    pub fn remap_track(&self, track_io: &mut i32, flags: i32) -> GmeResult {
        if *track_io < 0 || *track_io >= self.track_count {
            return Err(GmeError::Internal("Invalid track"));
        }

        if !self.playlist.is_empty()
            && (*track_io as usize) < self.playlist.len()
            && let Some(entry) = self.playlist.entry(*track_io as usize)
        {
            *track_io = 0;
            if entry.track >= 0 {
                *track_io = entry.track;
                if flags & 0x02 == 0 && entry.decimal_track {
                    *track_io -= 1;
                }
            }
            if !(0..self.raw_track_count).contains(track_io) {
                return Err(GmeError::Internal("Invalid track in m3u playlist"));
            }
        }
        Ok(())
    }

    /// Get track info with M3U overlay.
    pub fn track_info_with_m3u<F>(
        &self,
        out: &mut TrackInfo,
        track: i32,
        format: Format,
        get_format_info: F,
    ) -> GmeResult
    where
        F: FnOnce(&mut TrackInfo, i32) -> GmeResult,
    {
        out.track_count = self.track_count as i64;
        out.length = -1;
        out.loop_length = -1;
        out.intro_length = -1;
        out.fade_length = -1;
        out.play_length = -1;
        out.repeat_count = -1;
        out.song.clear();
        out.game.clear();
        out.author.clear();
        out.composer.clear();
        out.engineer.clear();
        out.sequencer.clear();
        out.tagger.clear();
        out.copyright.clear();
        out.date.clear();
        out.comment.clear();
        out.dumper.clear();
        out.system.clear();
        out.disc.clear();
        out.track.clear();
        out.ost.clear();

        copy_field(&mut out.system, format.system());

        let mut remapped = track;
        self.remap_track(&mut remapped, format.info().flags)?;
        get_format_info(out, remapped)?;

        // Override with M3U info
        if !self.playlist.is_empty() {
            if let Some(info) = Some(self.playlist.info()) {
                copy_field(&mut out.game, &info.title);
                copy_field(&mut out.author, &info.artist);
                copy_field(&mut out.engineer, &info.engineer);
                copy_field(&mut out.composer, &info.composer);
                copy_field(&mut out.sequencer, &info.sequencer);
                copy_field(&mut out.copyright, &info.copyright);
                copy_field(&mut out.dumper, &info.ripping);
                copy_field(&mut out.tagger, &info.tagging);
                copy_field(&mut out.date, &info.date);
            }

            if let Some(entry) = self.playlist.entry(track as usize) {
                copy_field(&mut out.song, &entry.name);
                if entry.length >= 0 {
                    out.length = entry.length as i64;
                }
                if entry.intro >= 0 {
                    out.intro_length = entry.intro as i64;
                }
                if entry.loop_len >= 0 {
                    out.loop_length = entry.loop_len as i64;
                }
                if entry.fade >= 0 {
                    out.fade_length = entry.fade as i64;
                }
                if entry.repeat >= 0 {
                    out.repeat_count = entry.repeat as i64;
                }
            }
        }

        out.compute_play_length();
        Ok(())
    }

    /// Load file from disk.
    pub fn load_file(&mut self, emu: &mut dyn GmeFile, path: &str) -> GmeResult {
        let mut reader = StdFileReader::open(path)?;
        self.load_reader(emu, &mut reader)
    }

    /// Load from data in memory.
    pub fn load_data(&mut self, emu: &mut dyn GmeFile, data: &[u8]) -> GmeResult {
        self.pre_load_common();
        emu.pre_load();
        let result = emu.load_mem_impl(data);
        self.post_load_common(emu, result)
    }

    /// Load from a DataReader.
    pub fn load_reader(&mut self, emu: &mut dyn GmeFile, reader: &mut dyn DataReader) -> GmeResult {
        self.pre_load_common();
        emu.pre_load();

        // Read entire file into memory
        let remain = reader.remain() as usize;
        self.file_data.resize(remain, 0);
        reader.read(&mut self.file_data)?;

        let format = emu.format();
        if format.info().track_count == 1 {
            self.tracks.clear();
            self.tracks.push(0);
            self.tracks.push(self.file_data.len() as i64);
        }

        let result = emu.load_mem_impl(&self.file_data);
        self.post_load_common(emu, result)
    }

    /// Load remaining data after a header has already been read.
    pub fn load_remaining(
        &mut self,
        emu: &mut dyn GmeFile,
        header: &[u8],
        reader: &mut dyn DataReader,
    ) -> GmeResult {
        let mut remaining = RemainingReader::new(header, reader);
        self.load_reader(emu, &mut remaining)
    }

    fn pre_load_common(&mut self) {
        self.unload();
    }

    fn post_load_common(&mut self, emu: &mut dyn GmeFile, result: GmeResult) -> GmeResult {
        if self.track_count == 0 {
            self.set_track_count(emu.format().info().track_count);
        }
        match result {
            Ok(()) => {
                emu.post_load();
                Ok(())
            }
            Err(e) => {
                self.unload();
                Err(e)
            }
        }
    }

    /// Load M3U playlist from file.
    pub fn load_m3u_file(&mut self, path: &str) -> GmeResult {
        if self.raw_track_count == 0 {
            return Err(GmeError::Internal("File must be loaded first"));
        }
        let result = self.playlist.load(path).map_err(GmeError::Internal);
        self.process_m3u_result(result)
    }

    /// Load M3U playlist from data.
    pub fn load_m3u_data(&mut self, data: &[u8]) -> GmeResult {
        if self.raw_track_count == 0 {
            return Err(GmeError::Internal("File must be loaded first"));
        }
        let mut playlist = M3uPlaylist::new();
        playlist
            .load_data(data)
            .map_err(|_| GmeError::NotAnM3uPlaylist)?;
        self.playlist = playlist;
        self.process_m3u_result(Ok(()))
    }

    fn process_m3u_result(&mut self, result: GmeResult) -> GmeResult {
        match result {
            Ok(()) => {
                if !self.playlist.is_empty() {
                    self.track_count = self.playlist.len() as i32;
                }
                let line = self.playlist.first_error();
                if line > 0 {
                    self.playlist_warning = format!("Problem in m3u at line {}", line);
                    self.warning = Some("Problem in m3u playlist");
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn clear_playlist(&mut self, emu: &mut dyn GmeFile) {
        self.playlist.clear();
        emu.clear_playlist_impl();
        self.track_count = self.raw_track_count;
    }
}
