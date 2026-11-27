// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Music_Emu.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::error::GmeResult;

use crate::format::Format;
use crate::track_info::Equalizer;

#[allow(dead_code)]
const SILENCE_MAX: i32 = 6; // seconds
const SILENCE_THRESHOLD: i16 = 0x10;
const FADE_BLOCK_SIZE: i64 = 512;
const FADE_SHIFT: i32 = 8;
const BUF_SIZE: usize = 2048;

/// Trait that format-specific emulators implement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Ym2612Backend {
    #[default]
    Nuked,
    Gens,
    Mame,
}

pub trait MusicEmulator {
    /// Choose an FM implementation before configuring audio.
    fn select_ym2612(&mut self, backend: Ym2612Backend) -> GmeResult {
        match backend {
            Ym2612Backend::Nuked => Ok(()),
            _ => Err("This format does not use a YM2612".into()),
        }
    }

    /// Get the format type.
    fn format(&self) -> Format;

    /// Set sample rate. Called once before loading.
    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult;

    /// Start playing a track. Track is already remapped.
    fn start_track_impl(&mut self, track: i32) -> GmeResult;

    /// Generate samples. Output is stereo i16.
    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult;

    /// Skip samples (for seeking). Each emulator transliterates C++
    /// Music_Emu::skip, including the out_time bookkeeping.
    fn skip_impl(&mut self, count: i32) -> GmeResult;

    /// Select discrete voice routing before initialization.
    fn enable_multi_channel(&mut self) -> GmeResult {
        Ok(())
    }

    /// Set equalizer.
    fn set_equalizer_impl(&mut self, _eq: &Equalizer) {}

    /// Configure stereo enhancement on formats with an effects buffer.
    fn set_stereo_depth_impl(&mut self, _depth: f64) {}

    /// Enable accurate emulation.
    fn enable_accuracy_impl(&mut self, _enable: bool) {}

    /// Mute voices by mask.
    fn mute_voices_impl(&mut self, _mask: i32) {}

    /// Disable echo (SPC).
    fn disable_echo_impl(&mut self, _disable: bool) {}

    /// Set tempo.
    fn set_tempo_impl(&mut self, _tempo: f64) {}

    /// Get voice count.
    fn voice_count(&self) -> i32;

    /// Get voice names.
    fn voice_names(&self) -> &[&str];
}

/// Shared playback state for all emulators.
pub struct MusicEmuState {
    pub sample_rate: i32,
    pub current_track: i32,
    pub out_time: i32,
    pub out_time_scaled: i32,
    pub emu_time: i32,
    pub emu_track_ended: bool,
    pub track_ended: bool,
    pub equalizer: Equalizer,
    pub mute_mask: i32,
    pub tempo: f64,
    pub gain: f64,
    pub multi_channel: bool,
    pub max_initial_silence: i32,
    pub silence_lookahead: i32,
    pub ignore_silence_flag: bool,
    pub autoload_playback_limit: bool,

    // fading
    pub fade_start: i32,
    pub fade_step: i32,

    // silence detection
    pub silence_time: i64,
    pub silence_count: i64,
    pub buf_remain: i64,
    pub silence_buf: Vec<i16>,

    // voice info
    pub voice_count: i32,
    pub voice_names_static: Vec<String>,
}

impl Default for MusicEmuState {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicEmuState {
    pub fn new() -> Self {
        Self {
            sample_rate: 0,
            current_track: -1,
            out_time: 0,
            out_time_scaled: 0,
            emu_time: 0,
            emu_track_ended: true,
            track_ended: true,
            equalizer: Equalizer::default(),
            mute_mask: 0,
            tempo: 1.0,
            gain: 1.0,
            multi_channel: false,
            max_initial_silence: 2,
            silence_lookahead: 3,
            ignore_silence_flag: false,
            autoload_playback_limit: true,
            fade_start: i32::MAX / 2 + 1,
            fade_step: 1,
            silence_time: 0,
            silence_count: 0,
            buf_remain: 0,
            silence_buf: vec![0i16; BUF_SIZE],
            voice_count: 0,
            voice_names_static: Vec::new(),
        }
    }

    pub fn clear_track_vars(&mut self) {
        self.current_track = -1;
        self.out_time = 0;
        self.out_time_scaled = 0;
        self.emu_time = 0;
        self.emu_track_ended = true;
        self.track_ended = true;
        self.fade_start = i32::MAX / 2 + 1;
        self.fade_step = 1;
        self.silence_time = 0;
        self.silence_count = 0;
        self.buf_remain = 0;
    }

    pub fn out_channels(&self) -> i32 {
        if self.multi_channel { 2 * 8 } else { 2 }
    }

    pub fn msec_to_samples(&self, msec: i32) -> i32 {
        let channels = i64::from(self.out_channels());
        let frames = i64::from(msec.max(0)) * i64::from(self.sample_rate) / 1000;
        (frames.min(i64::from(i32::MAX) / channels) * channels) as i32
    }

    pub fn tell(&self) -> i32 {
        let rate = i64::from(self.sample_rate) * i64::from(self.out_channels());
        if rate == 0 {
            return 0;
        }
        (i64::from(self.out_time) * 1000 / rate).min(i64::from(i32::MAX)) as i32
    }

    pub fn advance_output(&mut self, samples: i32) {
        self.out_time = self.out_time.saturating_add(samples);
        let frames = f64::from(samples) * self.tempo / f64::from(self.out_channels());
        self.out_time_scaled = self.out_time_scaled.saturating_add(frames as i32);
    }

    pub fn tell_samples(&self) -> i32 {
        self.out_time
    }

    pub fn tell_scaled(&self) -> i64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (self.out_time_scaled as f64 / (self.sample_rate as f64 / 1000.0)) as i64
    }

    pub fn set_fade(&mut self, start_msec: i32, length_msec: i32) {
        let length_msec = length_msec.max(1);
        self.fade_step = (1i32).max(
            (i64::from(self.sample_rate) * i64::from(length_msec)
                / (FADE_BLOCK_SIZE * i64::from(FADE_SHIFT) * 1000 / i64::from(self.out_channels())))
            .min(i64::from(i32::MAX)) as i32,
        );
        self.fade_start = self.msec_to_samples(start_msec);
    }

    pub fn set_tempo_range(&mut self, t: f64) {
        let t = t.clamp(0.02, 4.0);
        self.tempo = t;
    }

    /// Process fade on output samples. Returns true if the fade is complete.
    pub fn handle_fade(&mut self, out_time: i32, out: &mut [i16]) -> bool {
        let out_count = out.len() as i64;
        let mut faded = false;
        for i in (0..out_count).step_by(FADE_BLOCK_SIZE as usize) {
            let shift = 14;
            let unit = 1i32 << shift;
            let gain = int_log(
                ((out_time as i64 + i - self.fade_start as i64) / FADE_BLOCK_SIZE) as i32,
                self.fade_step,
                unit,
            );
            if gain < (unit >> FADE_SHIFT) {
                faded = true;
            }

            let end = (i + FADE_BLOCK_SIZE).min(out_count) as usize;
            for sample in &mut out[i as usize..end] {
                *sample = ((*sample as i32 * gain) >> shift) as i16;
            }
        }
        faded
    }
}

/// Count consecutive silent samples at end of buffer.
pub fn count_silence(buf: &mut [i16]) -> i64 {
    let size = buf.len();
    if size == 0 {
        return 0;
    }
    // Check from end backward: return number of samples from p to end
    // where p is the first non-silent sample from the end.
    // Uses a sentinel at buf[0] (matching C++ behavior) so an all-silent
    // buffer returns buf.len() instead of buf.len() - 1.
    let saved = buf[0];
    buf[0] = SILENCE_THRESHOLD;

    let mut p = size;
    while p > 0 {
        p -= 1;
        // C++: (unsigned)(*p + silence_threshold / 2) <= (unsigned)silence_threshold
        // — the sample is sign-extended, so "silent" means s in [-thr/2, thr/2].
        let val = (buf[p] as i32 + (SILENCE_THRESHOLD as i32 / 2)) as u32;
        if val <= SILENCE_THRESHOLD as u32 {
            continue;
        }
        break;
    }

    buf[0] = saved;
    (size - p) as i64
}

/// int_log: unit / pow(2.0, x / step)
fn int_log(x: i32, step: i32, unit: i32) -> i32 {
    let shift = x / step;
    if shift >= i32::BITS as i32 - 1 {
        return 0;
    }
    let fraction = ((x % step) as i64 * unit as i64 / step as i64) as i32;
    ((unit - fraction) + (fraction >> 1)) >> shift
}
