// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2004-2006 Shay Green
// Rust adaptation of Spc_Emu.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_common::format::Format;
use gme_common::gme_file::GmeFile;
use gme_common::gme_file::GmeFileState;
use gme_common::music_emu::{MusicEmuState, MusicEmulator, count_silence};
use gme_common::track_info::TrackInfo;
use gme_core::error::{GmeError, GmeResult};
use gme_core::fir_resampler::FirResampler;

use crate::header::{self, SPC_MIN_FILE_SIZE};
use crate::spc_apu::{self, SAMPLE_RATE, SpcApu, VOICE_COUNT};
use crate::spc_filter::{self, SpcFilter};

const NATIVE_SAMPLE_RATE: i32 = SAMPLE_RATE;
const RESAMPLE_BUF_SIZE: usize = NATIVE_SAMPLE_RATE as usize / 20 * 2;
const SILENCE_BUF_SIZE: i64 = 2048;
const SILENCE_MAX_SECS: i64 = 6;
const VOICE_NAMES: [&str; VOICE_COUNT as usize] = [
    "DSP 1", "DSP 2", "DSP 3", "DSP 4", "DSP 5", "DSP 6", "DSP 7", "DSP 8",
];

pub struct SpcEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub apu: SpcApu,
    pub filter: SpcFilter,
    pub resampler: FirResampler<24>,
    pub buf_initialized: bool,
    pub file_size: usize,
    gain: f64,
}

impl Default for SpcEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl SpcEmu {
    pub fn new() -> Self {
        Self {
            state: GmeFileState::new(),
            emu_state: MusicEmuState::new(),
            apu: SpcApu::new(),
            filter: SpcFilter::new(),
            resampler: FirResampler::new(),
            buf_initialized: false,
            file_size: 0,
            gain: 1.4,
        }
    }

    fn fill_buf(&mut self) -> GmeResult {
        debug_assert!(self.emu_state.buf_remain == 0);
        if !self.emu_state.emu_track_ended {
            // C++ emu_play: emulator time advances by what was generated
            self.emu_state.emu_time += SILENCE_BUF_SIZE as i32;
            let out = &mut self.emu_state.silence_buf;
            if self.emu_state.sample_rate == NATIVE_SAMPLE_RATE {
                let err = self.apu.play(SILENCE_BUF_SIZE as usize, out);
                if let Some(e) = err {
                    eprintln!("SPC CPU error: {}", e);
                }
                self.filter.run(out);
            } else {
                let len = SILENCE_BUF_SIZE as usize;
                let mut remain = len;
                while remain > 0 {
                    let read = self.resampler.read(&mut out[len - remain..]) as usize;
                    remain -= read;
                    if remain > 0 {
                        let n = self.resampler.max_write();
                        if n > 0 {
                            let buf = self.resampler.buffer_ptr();
                            self.apu.play(n, buf);
                            self.filter.run(&mut buf[..n]);
                            self.resampler.write(n);
                        } else {
                            break;
                        }
                    }
                }
            }
            let silence = count_silence(out);
            if silence < SILENCE_BUF_SIZE {
                self.emu_state.silence_time = (self.emu_state.emu_time as i64) - silence;
                self.emu_state.buf_remain = SILENCE_BUF_SIZE;
                return Ok(());
            }
        }
        self.emu_state.silence_count += SILENCE_BUF_SIZE;
        Ok(())
    }

    /// Raw DSP + filter + resampler path (no silence detection, no out_time update)
    fn raw_play(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        if self.emu_state.sample_rate == NATIVE_SAMPLE_RATE {
            let err = self.apu.play(count as usize, out);
            if let Some(e) = err {
                eprintln!("SPC CPU error: {}", e);
            }
            self.filter.run(out);
        } else {
            let count = count as usize;
            let mut remain = count;
            while remain > 0 {
                let read = self.resampler.read(&mut out[count - remain..]) as usize;
                remain -= read;
                if remain > 0 {
                    let n = self.resampler.max_write();
                    if n > 0 {
                        let buf = self.resampler.buffer_ptr();
                        self.apu.play(n, buf);
                        self.filter.run(&mut buf[..n]);
                        self.resampler.write(n);
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl GmeFile for SpcEmu {
    fn format(&self) -> Format {
        Format::Spc
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.raw_track_count = n;
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        if data.len() < SPC_MIN_FILE_SIZE {
            return Err(GmeError::WrongFileType);
        }
        if !header::check_spc_header(data) {
            return Err(GmeError::WrongFileType);
        }
        self.file_size = data.len();
        self.state.file_data = data.to_vec();
        self.state.set_track_count(1); // SPC files hold a single track
        self.emu_state.voice_count = VOICE_COUNT;
        self.emu_state.voice_names_static = VOICE_NAMES.iter().map(|s| s.to_string()).collect();
        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, _track: i32) -> GmeResult {
        header::get_spc_track_info(&self.state.file_data, info);
        Ok(())
    }
}

impl MusicEmulator for SpcEmu {
    fn format(&self) -> Format {
        Format::Spc
    }

    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        self.apu.init();
        // C++ Spc_Emu::set_sample_rate_ calls enable_accuracy(false), which
        // disables the SPC_Filter by default (gain still applies).
        self.filter.enable(false);
        self.emu_state.sample_rate = sample_rate;
        if sample_rate != NATIVE_SAMPLE_RATE {
            self.resampler.buffer_size(RESAMPLE_BUF_SIZE);
            self.resampler
                .time_ratio(NATIVE_SAMPLE_RATE as f64 / sample_rate as f64, 0.9965, 1.0);
        }
        self.buf_initialized = true;
        Ok(())
    }

    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;
        self.resampler.clear();
        self.filter.clear();

        self.apu
            .load_spc(&self.state.file_data)
            .map_err(|_| GmeError::CorruptFile)?;

        self.filter
            .set_gain((self.gain * spc_filter::GAIN_UNIT as f64) as i32);
        self.apu.clear_echo();

        // Set default fade from track info
        let mut spc_info = TrackInfo::default();
        let _ = self.track_info_impl(&mut spc_info, track);
        if self.emu_state.autoload_playback_limit && spc_info.length > 0 {
            self.emu_state.set_fade(spc_info.length as i32, 50);
        }

        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;

        // Pre-fill: run emulator until non-silent audio found (C++ Music_Emu::start_track)
        if !self.emu_state.ignore_silence_flag {
            let end =
                self.emu_state.max_initial_silence as i64 * 2 * self.emu_state.sample_rate as i64;
            while (self.emu_state.emu_time as i64) < end {
                self.fill_buf()?;
                if self.emu_state.buf_remain > 0 || self.emu_state.emu_track_ended {
                    break;
                }
            }
            // C++: emu_time = buf_remain — initial silence is discarded entirely
            self.emu_state.emu_time = self.emu_state.buf_remain as i32;
            self.emu_state.out_time = 0;
            self.emu_state.out_time_scaled = 0;
            self.emu_state.silence_time = 0;
            self.emu_state.silence_count = 0;
        }

        Ok(())
    }

    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let out_count = count as usize;

        // C++ Music_Emu::play: a finished track outputs zeros
        if self.emu_state.track_ended {
            out[..out_count].fill(0);
            self.emu_state.out_time += count;
            self.emu_state.out_time_scaled += (count as f64 * self.emu_state.tempo / 2.0) as i32;
            return Ok(());
        }

        let mut pos = 0usize;

        // Silence detection (all comparisons against emu_time, like C++)
        if self.emu_state.silence_count > 0 {
            // during a run of silence, run emulator at >=2x speed so it gets ahead
            let ahead_time = self.emu_state.silence_lookahead as i64
                * (self.emu_state.out_time as i64 + out_count as i64 - self.emu_state.silence_time)
                + self.emu_state.silence_time;
            while (self.emu_state.emu_time as i64) < ahead_time
                && self.emu_state.buf_remain == 0
                && !self.emu_state.emu_track_ended
            {
                self.fill_buf()?;
            }

            let n = (self.emu_state.silence_count as usize).min(out_count);
            out[..n].fill(0);
            self.emu_state.silence_count -= n as i64;
            pos = n;

            if !self.emu_state.ignore_silence_flag
                && self.emu_state.emu_time as i64 - self.emu_state.silence_time
                    > SILENCE_MAX_SECS * 2 * self.emu_state.sample_rate as i64
            {
                self.emu_state.track_ended = true;
                self.emu_state.emu_track_ended = true;
                self.emu_state.silence_count = 0;
                self.emu_state.buf_remain = 0;
            }
        }

        // Return held-back buffer first
        if self.emu_state.buf_remain > 0 && pos < out_count {
            let n = (self.emu_state.buf_remain as usize).min(out_count - pos);
            let buf_start = (SILENCE_BUF_SIZE - self.emu_state.buf_remain) as usize;
            let end = buf_start + n;
            out[pos..pos + n].copy_from_slice(&self.emu_state.silence_buf[buf_start..end]);
            self.emu_state.buf_remain -= n as i64;
            pos += n;
        }

        // Generate remaining samples
        if pos < out_count {
            let remain = (out_count - pos) as i32;
            // C++ emu_play
            self.emu_state.emu_time += remain;
            // C++ Music_Emu::emu_play: once the track has ended, output
            // hard zeros instead of running the emulator
            if !self.emu_state.emu_track_ended {
                self.raw_play(remain, &mut out[pos..])?;
            } else {
                out[pos..pos + remain as usize].fill(0);
            }
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;

            if !self.emu_state.ignore_silence_flag
                || self.emu_state.out_time > self.emu_state.fade_start
            {
                // check end for a new run of silence
                let silence = count_silence(&mut out[pos..pos + remain as usize]);
                if silence < remain as i64 {
                    self.emu_state.silence_time = self.emu_state.emu_time as i64 - silence;
                }
                if self.emu_state.emu_time as i64 - self.emu_state.silence_time > SILENCE_BUF_SIZE {
                    self.fill_buf()?; // cause silence detection on next play()
                }
            }
        }

        // Handle fading
        if self.emu_state.fade_start >= 0
            && self.emu_state.out_time > self.emu_state.fade_start
            && self.emu_state.handle_fade(self.emu_state.out_time, out)
        {
            self.emu_state.track_ended = true;
            self.emu_state.emu_track_ended = true;
        }

        self.emu_state.out_time += count;
        self.emu_state.out_time_scaled += (count as f64 * self.emu_state.tempo / 2.0) as i32;
        Ok(())
    }

    /// Transliteration of C++ Music_Emu::skip / Spc_Emu::skip_.
    fn skip_impl(&mut self, count: i32) -> GmeResult {
        self.emu_state.out_time += count;
        self.emu_state.out_time_scaled += (count as f64 * self.emu_state.tempo / 2.0) as i32;

        let mut count = count;

        // Remove from silence and buf first
        {
            let n = (count as i64).min(self.emu_state.silence_count);
            self.emu_state.silence_count -= n;
            count -= n as i32;

            let n = (count as i64).min(self.emu_state.buf_remain);
            self.emu_state.buf_remain -= n;
            count -= n as i32;
        }

        if count > 0 && !self.emu_state.emu_track_ended {
            self.emu_state.emu_time += count;

            // C++ Spc_Emu::skip_
            if self.emu_state.sample_rate != NATIVE_SAMPLE_RATE {
                let native_count = (count as f64 * self.resampler.ratio()) as i32 & !1;
                let skipped = self.resampler.skip_input(native_count as i64) as i32;
                let remaining = native_count - skipped;
                if remaining > 0 {
                    self.apu.skip(remaining as usize);
                    self.filter.clear();
                }
            } else if count > 0 {
                self.apu.skip(count as usize);
                self.filter.clear();
            }

            // Eliminate pop due to resampler (C++ uses raw play_: no silence
            // detection, no out_time bookkeeping)
            const RESAMPLER_LATENCY: usize = 64;
            let mut buf = [0i16; RESAMPLER_LATENCY];
            self.raw_play(RESAMPLER_LATENCY as i32, &mut buf)?;
        }

        if self.emu_state.silence_count == 0 && self.emu_state.buf_remain == 0 {
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    fn enable_accuracy_impl(&mut self, enable: bool) {
        self.filter.enable(enable);
    }

    fn mute_voices_impl(&mut self, mask: i32) {
        self.apu.mute_voices(mask);
    }

    fn disable_echo_impl(&mut self, disable: bool) {
        self.apu.disable_echo(disable);
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.apu
            .set_tempo((tempo * spc_apu::TEMPO_UNIT as f64) as i32);
    }

    fn voice_count(&self) -> i32 {
        VOICE_COUNT
    }

    fn voice_names(&self) -> &[&str] {
        &VOICE_NAMES
    }
}
