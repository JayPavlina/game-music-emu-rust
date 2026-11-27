// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Gym_Emu.cpp (2026-06-11 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! Sega Genesis GYM music file emulator: 1:1 port of `Gym_Emu.cpp`.
//!
//! GYM is a CPU-less command log driving a YM2612 (Nuked core) + SMS PSG +
//! PCM DAC. The FM chip renders at an oversampled rate and is brought down to
//! the output rate by `Dual_Resampler`, which also mixes in the mono
//! PSG/DAC `Blip_Buffer` — the same machinery as the VGM FM path, reused from
//! `gme-vgm`. `gme_gym_type` has `flags_ == 0`, so the C++ wrapper never puts
//! GYM in an Effects_Buffer; only the plain `Music_Emu` play/silence
//! machinery applies (mirrored from the proven-bit-exact SPC port).

use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator, count_silence};
use gme_common::track_info::TrackInfo;
use gme_core::blip_buffer::{
    BLIP_MED_QUALITY, BLIP_SAMPLE_BITS, BlipBuffer, BlipEq, BlipSynth, BlipTime,
};
use gme_core::error::{GmeError, GmeResult};
use gme_vgm::dual_resampler::DualResampler;
use gme_vgm::sms_apu::SmsApu;
use gme_vgm::ym2612::Ym2612;

use crate::header::{self, GymHeader};

// C++ Gym_Emu.cpp file-level constants
const MIN_TEMPO: f64 = 0.25;
const OVERSAMPLE_FACTOR: f64 = 5.0 / 3.0;
const FM_GAIN: f64 = 3.0;

const BASE_CLOCK: i64 = 53_700_300;
const CLOCK_RATE: i64 = BASE_CLOCK / 15; // 3580020

// C++ Music_Emu constants
const BUF_SIZE: i64 = 2048;
const SILENCE_MAX_SECS: i64 = 6;

const VOICE_NAMES: [&str; 8] = ["FM 1", "FM 2", "FM 3", "FM 4", "FM 5", "FM 6", "PCM", "PSG"];

pub struct GymEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub header: GymHeader,

    // sequence data begin, loop begin, current position, end (indices into
    // state.file_data; C++ uses raw pointers)
    data_offset: usize,
    loop_begin: Option<usize>,
    pos: usize,
    data_end: usize,
    /// frames remaining until loop beginning has been located
    loop_remain: i32,

    fm_sample_rate: f64,
    clocks_per_frame: i32,

    // dac (pcm)
    dac_amp: i32,
    prev_dac_count: i32,
    dac_enabled: bool,
    dac_muted: bool,
    dac_buf: [u8; 1024],

    // sound
    blip_buf: BlipBuffer,
    fm: Ym2612,
    dac_synth: BlipSynth,
    apu: SmsApu,
    dual: DualResampler,
    // Scratch frame buffer the FM chip renders into (C++ renders directly
    // into the resampler's input buffer; a scratch keeps borrows disjoint)
    fm_buf: Vec<i16>,

    buf_initialized: bool,
}

impl Default for GymEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl GymEmu {
    pub fn new() -> Self {
        let mut emu_state = MusicEmuState::new();
        // C++ Gym_Emu ctor: tracks should already be trimmed
        emu_state.silence_lookahead = 1;
        Self {
            state: GmeFileState::new(),
            emu_state,
            header: GymHeader::new(),
            data_offset: 0,
            loop_begin: None,
            pos: 0,
            data_end: 0,
            loop_remain: 0,
            fm_sample_rate: 0.0,
            clocks_per_frame: 0,
            dac_amp: -1,
            prev_dac_count: 0,
            dac_enabled: false,
            dac_muted: false,
            dac_buf: [0; 1024],
            blip_buf: BlipBuffer::new(),
            fm: Ym2612::default(),
            dac_synth: BlipSynth::new(BLIP_MED_QUALITY),
            apu: SmsApu::new(),
            dual: DualResampler::new(),
            fm_buf: Vec::new(),
            buf_initialized: false,
        }
    }

    /// C++ `Gym_Emu::track_length`
    pub fn track_length(&self) -> i64 {
        header::gym_track_length(&self.state.file_data[self.data_offset..self.data_end])
    }

    /// C++ `Gym_Emu::set_tempo_` body (tempo is pre-clamped by the caller)
    fn set_tempo_internal(&mut self) {
        if self.blip_buf.sample_rate() != 0 {
            // C++: long (clock_rate / 60 / tempo()) — integer division first
            self.clocks_per_frame = ((CLOCK_RATE / 60) as f64 / self.emu_state.tempo) as i32;
            self.dual.resize(
                (self.emu_state.sample_rate as f64 / (60.0 * self.emu_state.tempo)) as usize,
            );
        }
    }

    fn byte_at(&self, pos: usize) -> u8 {
        // C++ reads through raw pointers and can run past the end of the
        // file buffer on malformed data; clamp to 0 (frame terminator) here.
        self.state.file_data.get(pos).copied().unwrap_or(0)
    }

    /// C++ `Gym_Emu::run_dac`
    fn run_dac(&mut self, dac_count: i32) {
        // Guess beginning and end of sample and adjust rate and buffer
        // position accordingly.

        // count dac samples in next frame
        let mut next_dac_count: i32 = 0;
        let mut p = self.pos;
        loop {
            let cmd = self.byte_at(p);
            p += 1;
            if cmd == 0 || p > self.data_end {
                break;
            }
            let data = self.byte_at(p);
            p += 1;
            if cmd <= 2 {
                p += 1;
            }
            if cmd == 1 && data == 0x2A {
                next_dac_count += 1;
            }
        }

        // detect beginning and end of sample
        let mut rate_count = dac_count;
        let mut start: i32 = 0;
        if self.prev_dac_count == 0 && next_dac_count != 0 && dac_count < next_dac_count {
            rate_count = next_dac_count;
            start = next_dac_count - dac_count;
        } else if self.prev_dac_count != 0 && next_dac_count == 0 && dac_count < self.prev_dac_count
        {
            rate_count = self.prev_dac_count;
        }

        // Evenly space samples within buffer section being used
        let period = self
            .blip_buf
            .resampled_duration(self.clocks_per_frame)
            .wrapping_div(rate_count as u32);

        let mut time = self
            .blip_buf
            .resampled_time(0)
            .wrapping_add(period.wrapping_mul(start as u32))
            .wrapping_add(period >> 1);

        let mut dac_amp = self.dac_amp;
        if dac_amp < 0 {
            dac_amp = self.dac_buf[0] as i32;
        }

        for i in 0..dac_count as usize {
            let delta = self.dac_buf[i] as i32 - dac_amp;
            dac_amp += delta;
            self.dac_synth
                .offset_resampled(time, delta, &mut self.blip_buf);
            time = time.wrapping_add(period);
        }
        self.dac_amp = dac_amp;
    }

    /// C++ `Gym_Emu::parse_frame`
    fn parse_frame(&mut self) {
        let mut dac_count: i32 = 0;
        let mut pos = self.pos;

        if self.loop_remain != 0 {
            self.loop_remain -= 1;
            if self.loop_remain == 0 {
                self.loop_begin = Some(pos); // find loop on first time through sequence
            }
        }

        loop {
            let cmd = self.byte_at(pos);
            pos += 1;
            if cmd == 0 || pos > self.data_end {
                break;
            }
            let data = self.byte_at(pos);
            pos += 1;
            if cmd == 1 {
                let data2 = self.byte_at(pos);
                pos += 1;
                if data != 0x2A {
                    if data == 0x2B {
                        self.dac_enabled = (data2 & 0x80) != 0;
                    }
                    self.fm.write0(data as i32, data2 as i32);
                } else if (dac_count as usize) < self.dac_buf.len() {
                    self.dac_buf[dac_count as usize] = data2;
                    dac_count += self.dac_enabled as i32;
                }
            } else if cmd == 2 {
                let data2 = self.byte_at(pos);
                pos += 1;
                self.fm.write1(data as i32, data2 as i32);
            } else if cmd == 3 {
                self.apu.write_data(0, data as i32);
            } else {
                // to do (from C++): many GYM streams are full of errors, and
                // error count should reflect cases where music is really
                // having problems
                pos -= 1; // put data back
            }
        }

        // loop
        if pos >= self.data_end {
            debug_assert_eq!(pos, self.data_end);
            if let Some(loop_begin) = self.loop_begin {
                pos = loop_begin;
            } else {
                // C++ set_track_ended()
                self.emu_state.emu_track_ended = true;
            }
        }
        self.pos = pos;

        // dac
        if dac_count != 0 && !self.dac_muted {
            self.run_dac(dac_count);
        }
        self.prev_dac_count = dac_count;
    }

    /// C++ `Gym_Emu::play_frame` (Dual_Resampler's frame callback)
    fn play_frame(&mut self, blip_time: BlipTime, sample_count: i32) -> i32 {
        // C++ checks track_ended() — the outer Music_Emu flag, which lags
        // emu_track_ended_ until the current play() call returns
        if !self.emu_state.track_ended {
            self.parse_frame();
        }

        self.apu.end_frame(blip_time);

        self.fm_buf[..sample_count as usize].fill(0);
        let pairs = sample_count >> 1;
        self.fm.run(pairs, &mut self.fm_buf[..pairs as usize * 2]);

        sample_count
    }

    // C++ Dual_Resampler::play_frame_. Mirrors gme-vgm's VgmEmu version:
    // mixes in place into dual.sample_buf; callers copy out of it.
    fn play_frame_dual(&mut self) {
        let pair_count = (self.dual.sample_buf_size >> 1) as i64;
        let blip_time = self.blip_buf.count_clocks(pair_count);
        let sample_count = self.dual.oversamples_per_frame - self.dual.resampler.written() as i32;

        let new_count = self.play_frame(blip_time, sample_count) as usize;
        debug_assert!(new_count < self.dual.resampler_size);

        self.blip_buf.end_frame(blip_time);
        debug_assert_eq!(self.blip_buf.samples_avail(), pair_count);

        // C++ play_frame renders directly into resampler.buffer(); copy the
        // scratch buffer over to it
        let dst = self.dual.resampler.buffer_ptr();
        dst[..new_count].copy_from_slice(&self.fm_buf[..new_count]);
        self.dual.resampler.write(new_count);

        let n = self.dual.sample_buf_size;
        let dual = &mut self.dual;
        let count = dual.resampler.read(&mut dual.sample_buf[..n]);
        debug_assert_eq!(count as usize, n);

        self.mix_samples_in_place();
        self.blip_buf.remove_samples(pair_count);
    }

    // C++ Dual_Resampler::mix_samples (identical to gme-vgm's port): mixes
    // the mono PSG/DAC blip buffer into the resampled FM stereo stream.
    fn mix_samples_in_place(&mut self) {
        let bass = self.blip_buf.bass_shift;
        let mut accum = self.blip_buf.reader_accum;
        let n = self.dual.sample_buf_size >> 1;
        let buf = &mut self.dual.sample_buf;

        for i in 0..n {
            // C++ does NOT clamp s itself, only the mixed l/r results
            let s = accum >> (BLIP_SAMPLE_BITS - 16);
            let mut l = (buf[i * 2] as i32).wrapping_mul(2).wrapping_add(s);
            if l as i16 as i32 != l {
                l = 0x7FFF - (l >> 24);
            }
            accum = accum
                .wrapping_add(self.blip_buf.buffer[i])
                .wrapping_sub(accum >> bass);
            let mut r = (buf[i * 2 + 1] as i32).wrapping_mul(2).wrapping_add(s);
            if r as i16 as i32 != r {
                r = 0x7FFF - (r >> 24);
            }
            buf[i * 2] = l as i16;
            buf[i * 2 + 1] = r as i16;
        }

        self.blip_buf.reader_accum = accum;
    }

    // C++ Dual_Resampler::dual_play (called from Gym_Emu::play_)
    fn dual_play(&mut self, mut count: i64, out: &mut [i16]) {
        let mut out_pos = 0usize;

        // empty extra buffer
        let mut remain = self.dual.sample_buf_size as i64 - self.dual.buf_pos as i64;
        if remain != 0 {
            if remain > count {
                remain = count;
            }
            count -= remain;
            let src = self.dual.buf_pos as usize;
            out[out_pos..out_pos + remain as usize]
                .copy_from_slice(&self.dual.sample_buf[src..src + remain as usize]);
            out_pos += remain as usize;
            self.dual.buf_pos += remain as i32;
        }

        // entire frames
        while count >= self.dual.sample_buf_size as i64 {
            self.play_frame_dual();
            let n = self.dual.sample_buf_size;
            out[out_pos..out_pos + n].copy_from_slice(&self.dual.sample_buf[..n]);
            out_pos += n;
            count -= n as i64;
        }

        // extra
        if count != 0 {
            self.play_frame_dual();
            self.dual.buf_pos = count as i32;
            out[out_pos..out_pos + count as usize]
                .copy_from_slice(&self.dual.sample_buf[..count as usize]);
        }
    }

    /// C++ `Music_Emu::emu_play` (Gym_Emu::play_ is just dual_play)
    fn emu_play(&mut self, count: i32, out: &mut [i16]) {
        self.emu_state.emu_time += count;
        if self.emu_state.current_track >= 0 && !self.emu_state.emu_track_ended {
            self.dual_play(count as i64, out);
        } else {
            out[..count as usize].fill(0);
        }
    }

    /// C++ `Music_Emu::fill_buf`: fill internal buffer and check for silence
    fn fill_buf(&mut self) {
        debug_assert!(self.emu_state.buf_remain == 0);
        if !self.emu_state.emu_track_ended {
            let mut buf = std::mem::take(&mut self.emu_state.silence_buf);
            self.emu_play(BUF_SIZE as i32, &mut buf);
            let silence = count_silence(&mut buf);
            self.emu_state.silence_buf = buf;
            if silence < BUF_SIZE {
                self.emu_state.silence_time = self.emu_state.emu_time as i64 - silence;
                self.emu_state.buf_remain = BUF_SIZE;
                return;
            }
        }
        self.emu_state.silence_count += BUF_SIZE;
    }
}

impl GmeFile for GymEmu {
    fn format(&self) -> Format {
        Format::Gym
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.raw_track_count = n;
    }

    /// C++ `Gym_Emu::load_mem_`
    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        let offset = header::check_header(data)?;

        self.emu_state.voice_count = 8;
        self.emu_state.voice_names_static = VOICE_NAMES.iter().map(|s| s.to_string()).collect();

        self.data_offset = offset;
        self.data_end = data.len();
        self.loop_begin = None;

        if offset != 0 {
            self.header = GymHeader::from_bytes(data);
        } else {
            self.header = GymHeader::new();
        }

        self.state.file_data = data.to_vec();
        self.state.set_track_count(1); // GYM files hold a single track

        // C++ Music_Emu::post_load_: set_tempo (clocks_per_frame, resampler
        // resize) + remute_voices (re-applied per start_track here)
        if self.emu_state.sample_rate != 0 {
            self.set_tempo_internal();
        }

        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, _track: i32) -> GmeResult {
        header::get_gym_info(&self.header, self.track_length(), info);
        Ok(())
    }
}

impl MusicEmulator for GymEmu {
    fn format(&self) -> Format {
        Format::Gym
    }

    /// C++ `Gym_Emu::set_sample_rate_`
    fn select_ym2612(&mut self, backend: gme_common::music_emu::Ym2612Backend) -> GmeResult {
        self.fm = Ym2612::select(backend)?;
        Ok(())
    }
    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        let gain = self.emu_state.gain;
        let eq = BlipEq::with_params(-32.0, 8000, sample_rate as i64, 0);
        self.apu.treble_eq(&eq);
        self.dac_synth.treble_eq(&eq);
        self.apu.volume(0.135 * FM_GAIN * gain);
        // C++ Blip_Synth<blip_med_quality,1>::volume
        self.dac_synth.volume(0.125 / 256.0 * FM_GAIN * gain, 1);
        let factor = self.dual.setup(OVERSAMPLE_FACTOR, 0.990, FM_GAIN * gain);
        self.fm_sample_rate = sample_rate as f64 * factor;

        self.blip_buf
            .set_sample_rate(sample_rate, (1000.0 / 60.0 / MIN_TEMPO) as i32)
            .map_err(|_| GmeError::Internal("Failed to set sample rate"))?;
        self.blip_buf.clock_rate(CLOCK_RATE as u32);

        self.fm
            .set_rate(self.fm_sample_rate, BASE_CLOCK as f64 / 7.0)?;
        self.dual
            .reset((1.0 / 60.0 / MIN_TEMPO * sample_rate as f64) as usize);
        self.fm_buf = vec![0; self.dual.resampler_size];

        self.emu_state.sample_rate = sample_rate;
        self.buf_initialized = true;

        // If the file was loaded before the sample rate was set, finish now
        if !self.state.file_data.is_empty() {
            self.set_tempo_internal();
        }
        Ok(())
    }

    /// C++ `Music_Emu::start_track` + `Gym_Emu::start_track_`
    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;

        // Gym_Emu::start_track_
        self.pos = self.data_offset;
        self.loop_remain = self.header.loop_start() as i32;

        self.prev_dac_count = 0;
        self.dac_enabled = false;
        self.dac_amp = -1;

        // Reinit internal pointers (struct may have been moved since
        // creation), then rewire outputs as C++ post_load_/remute_voices did
        self.apu.reinit_pointers();
        self.mute_voices_impl(self.emu_state.mute_mask);

        self.fm.reset();
        self.apu.reset(0, 0);
        self.blip_buf.clear(true);
        self.dual.clear();

        // Music_Emu::start_track tail
        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;

        if !self.emu_state.ignore_silence_flag {
            // play until non-silence or end of track
            let end =
                self.emu_state.max_initial_silence as i64 * 2 * self.emu_state.sample_rate as i64;
            while (self.emu_state.emu_time as i64) < end {
                self.fill_buf();
                if self.emu_state.buf_remain > 0 || self.emu_state.emu_track_ended {
                    break;
                }
            }

            self.emu_state.emu_time = self.emu_state.buf_remain as i32;
            self.emu_state.out_time = 0;
            self.emu_state.out_time_scaled = 0;
            self.emu_state.silence_time = 0;
            self.emu_state.silence_count = 0;
        }

        Ok(())
    }

    /// C++ `Music_Emu::play`
    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let out_count = count as usize;

        if self.emu_state.track_ended {
            out[..out_count].fill(0);
        } else {
            debug_assert!(self.emu_state.emu_time >= self.emu_state.out_time);

            let mut pos = 0usize;
            if self.emu_state.silence_count > 0 {
                // during a run of silence, run emulator at >=2x speed so it
                // gets ahead
                let ahead_time = self.emu_state.silence_lookahead as i64
                    * (self.emu_state.out_time as i64 + count as i64 - self.emu_state.silence_time)
                    + self.emu_state.silence_time;
                while (self.emu_state.emu_time as i64) < ahead_time
                    && self.emu_state.buf_remain == 0
                    && !self.emu_state.emu_track_ended
                {
                    self.fill_buf();
                }

                // fill with silence
                pos = self.emu_state.silence_count.min(out_count as i64) as usize;
                out[..pos].fill(0);
                self.emu_state.silence_count -= pos as i64;

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

            if self.emu_state.buf_remain > 0 {
                // empty silence buf
                let n = (self.emu_state.buf_remain as usize).min(out_count - pos);
                let start = (BUF_SIZE - self.emu_state.buf_remain) as usize;
                out[pos..pos + n].copy_from_slice(&self.emu_state.silence_buf[start..start + n]);
                self.emu_state.buf_remain -= n as i64;
                pos += n;
            }

            // generate remaining samples normally
            let remain = out_count - pos;
            if remain > 0 {
                self.emu_play(remain as i32, &mut out[pos..]);
                self.emu_state.track_ended |= self.emu_state.emu_track_ended;

                if !self.emu_state.ignore_silence_flag
                    || self.emu_state.out_time > self.emu_state.fade_start
                {
                    // check end for a new run of silence
                    let silence = count_silence(&mut out[pos..pos + remain]);
                    if silence < remain as i64 {
                        self.emu_state.silence_time = self.emu_state.emu_time as i64 - silence;
                    }

                    if self.emu_state.emu_time as i64 - self.emu_state.silence_time >= BUF_SIZE {
                        self.fill_buf(); // cause silence detection on next play()
                    }
                }
            }

            if self.emu_state.fade_start >= 0
                && self.emu_state.out_time > self.emu_state.fade_start
                && self
                    .emu_state
                    .handle_fade(self.emu_state.out_time, &mut out[..out_count])
            {
                self.emu_state.track_ended = true;
                self.emu_state.emu_track_ended = true;
            }
        }
        self.emu_state.advance_output(count);
        Ok(())
    }

    /// Transliteration of C++ Music_Emu::skip / Music_Emu::skip_. Note that
    /// emu_play advances emu_time internally, so unlike the C++ original this
    /// must not add count to emu_time itself.
    fn skip_impl(&mut self, count: i32) -> GmeResult {
        self.emu_state.advance_output(count);

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
            // C++ Music_Emu::skip_: for long skips, mute sound
            let threshold = 30000;
            let mut scratch = [0i16; BUF_SIZE as usize];
            if count > threshold {
                let saved_mute = self.emu_state.mute_mask;
                self.mute_voices_impl(!0);

                while count > threshold / 2 && !self.emu_state.emu_track_ended {
                    self.emu_play(BUF_SIZE as i32, &mut scratch);
                    count -= BUF_SIZE as i32;
                }

                self.mute_voices_impl(saved_mute);
            }

            while count > 0 && !self.emu_state.emu_track_ended {
                let n = (count as i64).min(BUF_SIZE) as i32;
                count -= n;
                self.emu_play(n, &mut scratch);
            }
        }

        if self.emu_state.silence_count == 0 && self.emu_state.buf_remain == 0 {
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    /// C++ `Gym_Emu::set_tempo_`
    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.set_tempo_range(tempo);
        if self.emu_state.tempo < MIN_TEMPO {
            self.emu_state.tempo = MIN_TEMPO;
        }
        self.set_tempo_internal();
    }

    /// C++ `Gym_Emu::mute_voices_`
    fn mute_voices_impl(&mut self, mask: i32) {
        self.emu_state.mute_mask = mask;
        self.fm.mute_voices(mask);
        self.dac_muted = (mask & 0x40) != 0;
        if mask & 0x80 != 0 {
            self.apu.output_mono(std::ptr::null_mut());
        } else {
            self.apu.output_mono(&mut self.blip_buf as *mut BlipBuffer);
        }
    }

    fn voice_count(&self) -> i32 {
        self.emu_state.voice_count
    }

    fn voice_names(&self) -> &[&str] {
        &VOICE_NAMES
    }
}

// SmsApu stores raw pointers into blip_buf; they are re-derived in
// start_track_impl, never dereferenced across moves (same as VgmEmu)
unsafe impl Send for GymEmu {}
