// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Sap_Emu.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// SAP format emulator.
/// Matches C++ Sap_Emu.h / Sap_Emu.cpp.
use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator};
use gme_common::track_info::Equalizer;
use gme_common::track_info::{TrackInfo, copy_field};
use gme_core::blip_buffer::{BlipEq, BlipTime};
use gme_core::effects_buffer::EffectsBuffer;
use gme_core::endian::get_le16;
use gme_core::error::{GmeError, GmeResult};
use gme_core::multi_buffer::{CHANNEL_WAVE_TYPE, MultiBuffer};

/// C++ Sap_Emu ctor voice types (Sap_Emu.cpp:37).
const VOICE_TYPES: [i32; 8] = [
    CHANNEL_WAVE_TYPE | 1,
    CHANNEL_WAVE_TYPE | 2,
    CHANNEL_WAVE_TYPE | 3,
    CHANNEL_WAVE_TYPE,
    CHANNEL_WAVE_TYPE | 5,
    CHANNEL_WAVE_TYPE | 6,
    CHANNEL_WAVE_TYPE | 7,
    CHANNEL_WAVE_TYPE | 4,
];

use crate::header::{SapInfo, parse_info};
use crate::sap_apu::{END_ADDR, OSC_COUNT, START_ADDR, SapApu, SapApuImpl};
use crate::sap_cpu::{CpuResult, IDLE_ADDR, SapCpu, SapTime};

const BASE_SCANLINE_PERIOD: SapTime = 114;
const ATARI_CLOCK_RATE: u32 = 1_773_447;

/// SAP emulator.
pub struct SapEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub cpu: SapCpu,
    pub apu: SapApu,
    pub apu2: SapApu,
    pub apu_impl: Box<SapApuImpl>,
    pub info: SapInfo,
    /// 64KB RAM + 256 bytes padding.
    pub ram: Box<[u8; 0x10000 + 0x100]>,
    /// Stereo buffer for audio output.
    pub stereo_buf: EffectsBuffer,
    pub buf_initialized: bool,
    /// Period between play routine calls.
    scanline_period: SapTime,
    /// Time of next play routine call.
    pub next_play: SapTime,
    /// Mutes sound during init.
    time_mask: SapTime,
    /// Copy of the file data for ROM block extraction.
    file_data: Vec<u8>,
}

// SAFETY: SapEmu's raw pointers (via APU/CPU) all point to data owned by
// the struct itself. No shared references or thread-unsafe interior mutability.
unsafe impl Send for SapEmu {}

impl Default for SapEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl SapEmu {
    pub fn new() -> Self {
        let mut emu = Self {
            state: GmeFileState::new(),
            emu_state: MusicEmuState::new(),
            cpu: SapCpu::new(),
            apu: SapApu::new(),
            apu2: SapApu::new(),
            apu_impl: Box::new(SapApuImpl::new()),
            info: SapInfo::default(),
            ram: Box::new([0u8; 0x10000 + 0x100]),
            stereo_buf: EffectsBuffer::new(1, false),
            buf_initialized: false,
            scanline_period: BASE_SCANLINE_PERIOD,
            next_play: 0,
            time_mask: 0,
            file_data: Vec::new(),
        };
        emu.emu_state.voice_count = OSC_COUNT as i32;
        emu
    }

    /// Initialize audio buffer and wire oscillators.
    fn init_audio(&mut self, sample_rate: i32) -> GmeResult {
        self.stereo_buf
            .set_sample_rate(sample_rate, 1000 / 20)
            .map_err(|_| GmeError::Internal("Failed to set sample rate"))?;
        self.stereo_buf.clock_rate(ATARI_CLOCK_RATE);
        // Matches C++ Music_Emu default equalizer (treble -1.0, bass 60)
        // applied via Classic_Emu::set_equalizer_ -> buf->bass_freq(60).

        // Wire oscillators
        self.wire_oscillators();

        // Apply volume matching C++: gain / osc_count / 30
        self.apu_impl.volume(self.emu_state.gain);

        // Apply default equalizer
        self.set_equalizer_impl(&Equalizer::default());

        self.buf_initialized = true;
        Ok(())
    }

    /// Wire oscillator outputs per the mute mask.
    /// Transliteration of C++ Classic_Emu::mute_voices_ + Sap_Emu::set_voice:
    /// each voice gets its Effects_Buffer channel; pokey 1 plays on the left
    /// (center when mono), pokey 2 on the right.
    fn wire_voices(&mut self, mask: i32) {
        let voice_count = if self.info.stereo {
            OSC_COUNT * 2
        } else {
            OSC_COUNT
        };
        for i in (0..voice_count).rev() {
            if mask & (1 << i) != 0 {
                if i >= OSC_COUNT {
                    self.apu2.osc_output(i - OSC_COUNT, None);
                } else {
                    self.apu.osc_output(i, None);
                }
            } else {
                let ch = self.stereo_buf.channel(i as i32, VOICE_TYPES[i]);
                if i >= OSC_COUNT {
                    self.apu2.osc_output(i - OSC_COUNT, Some(ch.right));
                } else {
                    let buf = if self.info.stereo { ch.left } else { ch.center };
                    self.apu.osc_output(i, Some(buf));
                }
            }
        }
    }

    fn wire_oscillators(&mut self) {
        self.wire_voices(self.emu_state.mute_mask);
    }

    /// Push return address onto stack and set PC (JSR).
    /// Matches C++ Sap_Emu::cpu_jsr.
    fn cpu_jsr(&mut self, addr: u32) {
        self.cpu.r.pc = addr as u16;
        let high_byte = ((IDLE_ADDR - 1) >> 8) as u8;
        if self.cpu.r.sp == 0xFE && self.ram[0x1FF] == high_byte {
            self.cpu.r.sp = self.cpu.r.sp.wrapping_add(1); // pop extra byte off
        }
        // Push high_byte (flags for RTI)
        self.ram[0x100 + self.cpu.r.sp as usize] = high_byte;
        self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
        // Push high_byte (PC high)
        self.ram[0x100 + self.cpu.r.sp as usize] = high_byte;
        self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
        // Push low_byte (PC low)
        self.ram[0x100 + self.cpu.r.sp as usize] = ((IDLE_ADDR - 1) & 0xFF) as u8;
        self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
    }

    /// Run a subroutine and wait for it to return.
    /// Matches C++ Sap_Emu::run_routine.
    fn run_routine(&mut self, addr: u32) {
        self.cpu_jsr(addr);
        let max_cycles = 312 * BASE_SCANLINE_PERIOD * 60;

        let cpu_ptr = &mut self.cpu as *mut SapCpu;
        let apu_ptr = &mut self.apu as *mut SapApu;
        let apu2_ptr = &mut self.apu2 as *mut SapApu;
        let time_mask = self.time_mask;
        let is_stereo = self.info.stereo;

        unsafe {
            (*cpu_ptr).run(max_cycles, &mut |addr, data| {
                let cur_time = (*cpu_ptr).time() & time_mask;
                if (addr ^ START_ADDR) <= (END_ADDR - START_ADDR) {
                    (*apu_ptr).write_data(cur_time, addr, data);
                } else if (addr ^ (START_ADDR + 0x10)) <= (END_ADDR - START_ADDR) && is_stereo {
                    (*apu2_ptr).write_data(cur_time, addr ^ 0x10, data);
                }
            });
        }
    }

    /// Call init routine for a track.
    fn call_init(&mut self, track: i32) {
        match self.info.player_type {
            'B' => {
                self.cpu.r.a = track as u8;
                self.run_routine(self.info.init_addr as u32);
            }
            'C' => {
                self.cpu.r.a = 0x70;
                self.cpu.r.x = (self.info.music_addr & 0xFF) as u8;
                self.cpu.r.y = (self.info.music_addr >> 8) as u8;
                self.run_routine((self.info.play_addr + 3) as u32);
                self.cpu.r.a = 0;
                self.cpu.r.x = track as u8;
                self.run_routine((self.info.play_addr + 3) as u32);
            }
            _ => {}
        }
    }

    /// Call play routine.
    fn call_play(&mut self) {
        match self.info.player_type {
            'B' => {
                self.cpu_jsr(self.info.play_addr as u32);
            }
            'C' => {
                self.cpu_jsr((self.info.play_addr + 6) as u32);
            }
            _ => {}
        }
    }

    fn play_period(&self) -> SapTime {
        self.info.fastplay * self.scanline_period
    }
}

impl GmeFile for SapEmu {
    fn format(&self) -> Format {
        Format::Sap
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.set_track_count(n);
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        self.file_data = data.to_vec();

        self.info = parse_info(data)?;

        self.state.set_track_count(self.info.track_count);

        let voice_count = if self.info.stereo {
            OSC_COUNT as i32 * 2
        } else {
            OSC_COUNT as i32
        };
        self.emu_state.voice_count = voice_count;

        // Setup buffer
        self.init_audio(self.emu_state.sample_rate.max(44100))?;

        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, _track: i32) -> GmeResult {
        copy_field(&mut info.game, &self.info.name);
        copy_field(&mut info.author, &self.info.author);
        copy_field(&mut info.copyright, &self.info.copyright);
        info.track_count = self.info.track_count as i64;
        Ok(())
    }
}

impl MusicEmulator for SapEmu {
    fn format(&self) -> Format {
        Format::Sap
    }

    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        self.emu_state.sample_rate = sample_rate;
        self.init_audio(sample_rate)
    }

    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;

        // Clear buffer
        self.stereo_buf.clear();

        // Clear RAM
        self.ram.fill(0);

        // Load ROM data blocks
        let data = &self.file_data;
        let mut pos = self.info.rom_data_offset;
        while data.len().saturating_sub(pos) >= 4 {
            let start = get_le16(&data[pos..]) as usize;
            let end = get_le16(&data[pos + 2..]) as usize;
            pos += 4;
            if end < start {
                break;
            }
            let len = end - start + 1;
            if len > data.len().saturating_sub(pos) {
                break;
            }
            if end < self.ram.len() {
                self.ram[start..=end].copy_from_slice(&data[pos..pos + len]);
            }
            pos += len;
            // Skip 0xFF 0xFF sentinel if present
            if data.len().saturating_sub(pos) >= 2 && data[pos] == 0xFF && data[pos + 1] == 0xFF {
                pos += 2;
            }
        }

        // Reset APUs
        let apu_impl_ptr = &mut *self.apu_impl as *mut SapApuImpl;
        self.apu.reset(apu_impl_ptr);
        self.apu2.reset(apu_impl_ptr);

        // Reset CPU
        let ram_ptr = self.ram.as_mut_ptr();
        self.cpu.reset(ram_ptr);

        // Mute sound during init
        self.time_mask = 0;
        self.call_init(track);
        self.time_mask = -1;

        self.next_play = self.play_period();

        // Wire oscillators
        self.wire_oscillators();

        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;
        self.emu_state.emu_time = 0;
        self.emu_state.silence_time = 0;
        self.emu_state.silence_count = 0;
        self.emu_state.buf_remain = 0;
        self.emu_state.out_time = 0;
        self.emu_state.out_time_scaled = 0;

        // Initial silence detection (matches C++ Music_Emu::start_track).
        // Run the emulator forward up to max_initial_silence seconds to skip
        // initial silence. This advances the APU/CPU state so that the first
        // play() call produces audio from the correct point.
        let max_initial = self.emu_state.max_initial_silence as i64;
        let out_ch = self.emu_state.out_channels() as i64;
        let sr = self.emu_state.sample_rate as i64;
        let end_samples = max_initial * out_ch * sr;
        let _buf_size = self.emu_state.silence_buf.len() as i64;

        while !self.emu_state.ignore_silence_flag && (self.emu_state.emu_time as i64) < end_samples
        {
            self.fill_buf_internal()?;
            if self.emu_state.buf_remain > 0 || self.emu_state.emu_track_ended {
                break;
            }
        }

        // Reset time tracking (matches C++ Music_Emu::start_track)
        self.emu_state.emu_time = self.emu_state.buf_remain as i32;
        self.emu_state.out_time = 0;
        self.emu_state.out_time_scaled = 0;
        self.emu_state.silence_time = 0;
        self.emu_state.silence_count = 0;

        Ok(())
    }

    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        if !self.buf_initialized {
            out[..count as usize].fill(0);
            return Ok(());
        }

        if self.emu_state.track_ended {
            out[..count as usize].fill(0);
            return Ok(());
        }

        let out_channels = self.emu_state.out_channels() as i64;
        let silence_max = 6i64;
        let sample_rate = self.emu_state.sample_rate as i64;

        let mut pos = 0;
        let mut remaining = count as i64;

        // During silence, run emulator at lookahead speed to get ahead (matches C++ fill_buf)
        if self.emu_state.silence_count > 0 {
            // C++: ahead_time = silence_lookahead * (out_time + out_count - silence_time) + silence_time
            let ahead_time = self.emu_state.silence_lookahead as i64
                * (self.emu_state.out_time as i64 + count as i64 - self.emu_state.silence_time)
                + self.emu_state.silence_time;
            // Match C++: while (emu_time < ahead_time && !(buf_remain | emu_track_ended_))
            while (self.emu_state.emu_time as i64) < ahead_time
                && !(self.emu_state.buf_remain > 0 || self.emu_state.emu_track_ended)
            {
                self.fill_buf_internal()?;
            }

            // Fill output with silence
            let silence_to_output = remaining.min(self.emu_state.silence_count);
            out[pos..pos + silence_to_output as usize].fill(0);
            pos += silence_to_output as usize;
            remaining -= silence_to_output;
            self.emu_state.silence_count -= silence_to_output;

            if !self.emu_state.ignore_silence_flag
                && (self.emu_state.emu_time as i64) - self.emu_state.silence_time
                    > silence_max * out_channels * sample_rate
            {
                self.emu_state.emu_track_ended = true;
                self.emu_state.track_ended = true;
                self.emu_state.silence_count = 0;
                self.emu_state.buf_remain = 0;
            }
        }

        // Output any buffered non-silent samples
        if self.emu_state.buf_remain > 0 {
            let buf_size = self.emu_state.silence_buf.len() as i64;
            let to_copy = remaining.min(self.emu_state.buf_remain);
            let src_start = buf_size - self.emu_state.buf_remain;
            out[pos..pos + to_copy as usize].copy_from_slice(
                &self.emu_state.silence_buf
                    [src_start as usize..src_start as usize + to_copy as usize],
            );
            pos += to_copy as usize;
            remaining -= to_copy;
            self.emu_state.buf_remain -= to_copy;
        }

        // Generate remaining samples normally
        if remaining > 0 {
            self.emu_play_internal(remaining, &mut out[pos..])?;
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;

            if !self.emu_state.ignore_silence_flag {
                let silence = count_silence(&out[pos..pos + remaining as usize]);
                if silence < remaining {
                    self.emu_state.silence_time = (self.emu_state.emu_time as i64) - silence;
                }

                // Match C++: only start silence detection after max_initial_silence seconds
                let initial_silence_samples =
                    (self.emu_state.max_initial_silence as i64) * sample_rate * out_channels;
                let silence_duration =
                    (self.emu_state.emu_time as i64) - self.emu_state.silence_time;
                if silence_duration > initial_silence_samples
                    && silence_duration > silence_max * out_channels * sample_rate
                {
                    self.emu_state.emu_track_ended = true;
                    self.emu_state.track_ended = true;
                }

                if silence_duration >= self.emu_state.silence_buf.len() as i64 {
                    self.fill_buf_internal()?; // cause silence detection on next play()
                }
            }
        }

        if self.emu_state.fade_start >= 0
            && self.emu_state.out_time > self.emu_state.fade_start
            && self
                .emu_state
                .handle_fade(self.emu_state.out_time, &mut out[..count as usize])
        {
            self.emu_state.track_ended = true;
            self.emu_state.emu_track_ended = true;
        }

        self.emu_state.out_time += count;
        self.emu_state.out_time_scaled +=
            (count as f64 * self.emu_state.tempo / out_channels as f64) as i32;
        Ok(())
    }

    /// Transliteration of C++ Music_Emu::skip / Music_Emu::skip_.
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

            // C++ Music_Emu::skip_: for long skips, mute sound
            const BUF_SIZE: i32 = 2048;
            let threshold = 30000;
            let mut scratch = [0i16; BUF_SIZE as usize];
            if count > threshold {
                let saved_mute = self.emu_state.mute_mask;
                self.mute_voices_impl(!0);

                while count > threshold / 2 && !self.emu_state.emu_track_ended {
                    self.emu_play_internal(BUF_SIZE as i64, &mut scratch)?;
                    count -= BUF_SIZE;
                }

                self.mute_voices_impl(saved_mute);
            }

            while count > 0 && !self.emu_state.emu_track_ended {
                let n = count.min(BUF_SIZE);
                count -= n;
                self.emu_play_internal(n as i64, &mut scratch)?;
            }
        }

        if self.emu_state.silence_count == 0 && self.emu_state.buf_remain == 0 {
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.tempo = tempo;
        self.scanline_period = (BASE_SCANLINE_PERIOD as f64 / tempo) as SapTime;
    }

    fn set_equalizer_impl(&mut self, settings: &Equalizer) {
        let eq = BlipEq::new(settings.treble);
        self.apu_impl.synth.treble_eq(&eq);
        self.stereo_buf.bass_freq(settings.bass as i32);
    }

    fn set_stereo_depth_impl(&mut self, depth: f64) {
        self.stereo_buf.set_depth(depth);
        self.mute_voices_impl(self.emu_state.mute_mask);
    }

    fn mute_voices_impl(&mut self, mask: i32) {
        self.emu_state.mute_mask = mask;
        self.wire_voices(mask);
    }

    fn voice_count(&self) -> i32 {
        if self.info.stereo {
            OSC_COUNT as i32 * 2
        } else {
            OSC_COUNT as i32
        }
    }

    fn voice_names(&self) -> &[&str] {
        if self.info.stereo {
            &[
                "Wave 1", "Wave 2", "Wave 3", "Wave 4", "Wave 5", "Wave 6", "Wave 7", "Wave 8",
            ]
        } else {
            &["Wave 1", "Wave 2", "Wave 3", "Wave 4"]
        }
    }
}

impl SapEmu {
    /// Run emulation for the given duration.
    /// Matches C++ Sap_Emu::run_clocks.
    fn run_clocks(&mut self, duration: &mut BlipTime) -> GmeResult {
        let target = *duration;
        let mut cpu_time: SapTime = 0;

        // Reset CPU time to 0 (matches C++ set_time(0))
        self.cpu.set_time(0);

        while cpu_time < target {
            // Use raw pointers to avoid borrow checker issues with closures
            let cpu_ptr = &mut self.cpu as *mut SapCpu;
            let apu_ptr = &mut self.apu as *mut SapApu;
            let apu2_ptr = &mut self.apu2 as *mut SapApu;
            let is_stereo = self.info.stereo;
            let time_mask = self.time_mask;

            let mut need_call_play = false;

            unsafe {
                // Always run with full target (matches C++ cpu::run(duration))
                let cpu_result = (*cpu_ptr).run(target, &mut |addr, data| {
                    let cur_time = (*cpu_ptr).time() & time_mask;
                    if (addr ^ START_ADDR) <= (END_ADDR - START_ADDR) {
                        (*apu_ptr).write_data(cur_time, addr, data);
                    } else if (addr ^ (START_ADDR + 0x10)) <= (END_ADDR - START_ADDR) && is_stereo {
                        (*apu2_ptr).write_data(cur_time, addr ^ 0x10, data);
                    }
                });
                cpu_time = (*cpu_ptr).time();

                match cpu_result {
                    CpuResult::Illegal => {
                        cpu_time = (*cpu_ptr).time();
                    }
                    CpuResult::HitIdle | CpuResult::OutOfTime => {
                        // Matches C++: check r.pc == idle_addr after run() returns
                        let at_idle = (*cpu_ptr).r.pc as u32 == IDLE_ADDR;
                        if at_idle {
                            if self.next_play <= target {
                                cpu_time = self.next_play;
                                (*cpu_ptr).set_time(cpu_time);
                                self.next_play += self.play_period();
                                need_call_play = true;
                            } else {
                                cpu_time = target;
                                (*cpu_ptr).set_time(cpu_time);
                            }
                        } else {
                            break;
                        }
                    }
                }
            }

            if need_call_play {
                self.call_play();
                continue;
            }
            if self.cpu.r.pc as u32 == IDLE_ADDR && cpu_time >= target {
                break;
            }
        }

        *duration = cpu_time;
        self.next_play -= cpu_time;
        if self.next_play < 0 {
            self.next_play = 0;
        }
        self.apu.end_frame(cpu_time);
        if self.info.stereo {
            self.apu2.end_frame(cpu_time);
        }
        self.stereo_buf.end_frame(cpu_time);

        Ok(())
    }

    /// Fill internal silence buffer by running emulator at normal speed.
    /// Matches C++ Music_Emu::fill_buf.
    fn fill_buf_internal(&mut self) -> GmeResult {
        if !self.emu_state.emu_track_ended && self.emu_state.buf_remain == 0 {
            let buf_len = self.emu_state.silence_buf.len();
            let mut remaining = buf_len;
            let mut pos = 0;

            while remaining > 0 {
                let avail = self.stereo_buf.samples_avail();
                if avail > 0 {
                    let to_read = remaining.min(avail as usize);
                    let read = self
                        .stereo_buf
                        .read_samples(&mut self.emu_state.silence_buf[pos..], to_read as i64);
                    pos += read as usize;
                    remaining -= read as usize;
                }

                if remaining > 0 {
                    let msec = self.stereo_buf.length();
                    let mut clocks = msec * ATARI_CLOCK_RATE as i32 / 1000;
                    self.run_clocks(&mut clocks)?;
                }
            }

            let silence = count_silence(&self.emu_state.silence_buf);
            if silence < buf_len as i64 {
                self.emu_state.silence_time = (self.emu_state.emu_time as i64) - silence;
                self.emu_state.buf_remain = buf_len as i64;
            } else {
                self.emu_state.silence_count += buf_len as i64;
            }
            // Match C++ fill_buf: emu_time += buf_size
            self.emu_state.emu_time += buf_len as i32;
        }
        Ok(())
    }

    /// Generate samples into the given output buffer.
    /// Matches C++ Music_Emu::emu_play.
    fn emu_play_internal(&mut self, count: i64, out: &mut [i16]) -> GmeResult {
        let mut remaining = count as usize;
        let mut pos = 0;

        while remaining > 0 {
            let avail = self.stereo_buf.samples_avail();
            if avail > 0 {
                let to_read = remaining.min(avail as usize);
                let read = self
                    .stereo_buf
                    .read_samples(&mut out[pos..], to_read as i64);
                pos += read as usize;
                remaining -= read as usize;
            }

            if remaining > 0 {
                let msec = self.stereo_buf.length();
                let mut clocks = msec * ATARI_CLOCK_RATE as i32 / 1000;
                self.run_clocks(&mut clocks)?;
            }
        }
        Ok(())
    }
}

/// Count consecutive silent samples at end of buffer.
fn count_silence(buf: &[i16]) -> i64 {
    let threshold: i16 = 0x10;
    let half = threshold / 2;
    let mut count = 0i64;
    for &s in buf.iter().rev() {
        if (s.wrapping_add(half) as u16) <= threshold as u16 {
            count += 1;
        } else {
            break;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_test_sap() -> SapEmu {
        let data = std::fs::read("../../assets/test.sap")
            .or_else(|_| std::fs::read("assets/test.sap"))
            .expect("test.sap not found");
        let mut emu = SapEmu::new();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.load_mem_impl(&data).expect("Failed to load test.sap");
        emu
    }

    #[test]
    fn test_load_sap() {
        let emu = load_test_sap();
        assert!(emu.state.track_count > 0, "Should have tracks");
    }

    #[test]
    fn test_set_sample_rate() {
        let mut emu = load_test_sap();
        emu.set_sample_rate_impl(44100).unwrap();
        assert_eq!(emu.emu_state.sample_rate, 44100);
        assert!(emu.buf_initialized);
    }

    #[test]
    fn test_start_track() {
        let mut emu = load_test_sap();
        emu.start_track_impl(0).unwrap();
        assert!(!emu.emu_state.track_ended);
        assert!(!emu.emu_state.emu_track_ended);
    }

    #[test]
    fn test_play_produces_nonzero_output() {
        let mut emu = load_test_sap();
        emu.start_track_impl(0).unwrap();

        let count = 8820; // 100ms stereo
        let mut out = vec![0i16; count];
        emu.play_impl(count as i32, &mut out).unwrap();

        let nonzero = out.iter().filter(|&&s| s != 0).count();
        assert!(
            nonzero > 0,
            "Expected non-zero audio output, got {} / {} nonzero",
            nonzero,
            count
        );
    }

    #[test]
    fn test_play_continues_beyond_silence_threshold() {
        let mut emu = load_test_sap();
        emu.start_track_impl(0).unwrap();

        // Play for 10 seconds - should NOT stop early
        let sample_rate = 44100;
        let block_size = sample_rate * 2; // 1 second stereo blocks
        let mut total_played = 0;

        for _sec in 0..10 {
            let mut out = vec![0i16; block_size as usize];
            emu.play_impl(block_size, &mut out).unwrap();
            total_played += block_size;

            if emu.emu_state.track_ended {
                panic!(
                    "Track ended at second {} after {} samples ({} seconds)",
                    _sec,
                    total_played,
                    total_played / (sample_rate * 2)
                );
            }
        }

        // Should have played all 10 seconds
        assert_eq!(total_played, sample_rate * 2 * 10);
    }

    #[test]
    fn test_music_progresses_over_time() {
        let mut emu = load_test_sap();
        emu.start_track_impl(0).unwrap();

        let count = 8820;

        // Verify the emulator produces non-zero output over multiple frames
        let mut has_nonzero = false;
        for _frame in 0..5 {
            let mut out = vec![0i16; count];
            emu.play_impl(count as i32, &mut out).unwrap();
            if out.iter().any(|&s| s != 0) {
                has_nonzero = true;
            }
        }
        assert!(has_nonzero, "Should produce non-zero audio output");
    }

    #[test]
    fn debug_samples_avail() {
        let data = std::fs::read("../../assets/two_tracks.sap")
            .or_else(|_| std::fs::read("assets/two_tracks.sap"))
            .expect("two_tracks.sap not found");
        let mut emu = SapEmu::new();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.load_mem_impl(&data).unwrap();
        emu.start_track_impl(1).unwrap();

        eprintln!(
            "After start_track: buf_remain={} samples_avail={} emu_time={}",
            emu.emu_state.buf_remain,
            emu.stereo_buf.samples_avail(),
            emu.emu_state.emu_time
        );

        // Play 25 frames (to exhaust the initial buffer)
        for frame in 0..25 {
            let mut out = vec![0i16; 100];
            emu.play_impl(100, &mut out).unwrap();
            if frame % 5 == 0 || frame >= 20 {
                eprintln!(
                    "frame={} samples_avail={} buf_remain={}",
                    frame,
                    emu.stereo_buf.samples_avail(),
                    emu.emu_state.buf_remain
                );
            }
        }
    }

    #[test]
    fn compare_two_tracks_audio_with_cpp() {
        let data = std::fs::read("../../assets/two_tracks.sap")
            .or_else(|_| std::fs::read("assets/two_tracks.sap"))
            .expect("two_tracks.sap not found");
        let mut emu = SapEmu::new();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.load_mem_impl(&data).unwrap();
        emu.start_track_impl(1).unwrap();

        let block_size = 4410usize; // 0.1 second blocks
        let mut total = 0i32;
        for _block in 0..150 {
            let mut out = vec![0i16; block_size];
            emu.play_impl(block_size as i32, &mut out).unwrap();
            eprintln!("{} {}", total, out[0]);
            total += block_size as i32;
        }
    }
}
