// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Vgm_Emu.cpp, Vgm_Emu_Impl.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator, count_silence};
use gme_common::track_info::Equalizer;
use gme_common::track_info::TrackInfo;
use gme_core::blip_buffer::{
    BLIP_MED_QUALITY, BLIP_SAMPLE_BITS, BlipBuffer, BlipEq, BlipSynth, BlipTime,
};
use gme_core::effects_buffer::EffectsBuffer;
use gme_core::endian::get_le32;
use gme_core::error::{GmeError, GmeResult};
use gme_core::multi_buffer::{CHANNEL_NOISE_TYPE, CHANNEL_WAVE_TYPE, MultiBuffer};

use crate::dual_resampler::DualResampler;
use crate::header::{self, HEADER_SIZE, VGM_MIN_FILE_SIZE, VGM_RATE, VgmHeader};
use crate::sms_apu::SmsApu;
use crate::ym2413::Ym2413;
use crate::ym2612_nuked::Ym2612NukedEmu;

const STEREO: usize = 2;
const SILENCE_BUF_SIZE: i64 = 2048;
const SILENCE_MAX_SECS: i64 = 6;
const BLIP_TIME_BITS: u64 = 12;
const FM_TIME_BITS: u64 = 12;

const CMD_PSG: u8 = 0x50;
const CMD_YM2413: u8 = 0x51;
const CMD_YM2612_PORT0: u8 = 0x52;
const CMD_YM2612_PORT1: u8 = 0x53;
const CMD_YM2151: u8 = 0x54;
const CMD_DELAY: u8 = 0x61;
const CMD_DELAY_735: u8 = 0x62;
const CMD_DELAY_882: u8 = 0x63;
const CMD_BYTE_DELAY: u8 = 0x64;
const CMD_END: u8 = 0x66;
const CMD_DATA_BLOCK: u8 = 0x67;
#[allow(dead_code)]
const CMD_SHORT_DELAY: u8 = 0x70;
#[allow(dead_code)]
const CMD_PCM_DELAY: u8 = 0x80;
const CMD_PCM_SEEK: u8 = 0xE0;

const CMD_GG_STEREO: u8 = 0x4F;
const CMD_GG_STEREO_2: u8 = 0x3F;
const CMD_PSG_2: u8 = 0x30;
const CMD_YM2413_2: u8 = 0xA1;
const CMD_YM2612_2_PORT0: u8 = 0xA2;
const CMD_YM2612_2_PORT1: u8 = 0xA3;

const PCM_BLOCK_TYPE: u8 = 0x00;
const YM2612_DAC_PORT: u8 = 0x2A;

const FM_GAIN: f64 = 3.0; // FM emulators are internally quieter to avoid 16-bit overflow
const ROLLOFF: f64 = 0.990;
const OVERSAMPLE_FACTOR: f64 = 1.0;

fn command_len(cmd: u8) -> usize {
    match cmd >> 4 {
        0x03 | 0x04 => 2,
        0x05 | 0x0A | 0x0B => 3,
        0x0C | 0x0D => 4,
        0x0E | 0x0F => 5,
        _ => 1,
    }
}

const PSG_VOICE_NAMES: [&str; 4] = ["Square 1", "Square 2", "Square 3", "Noise"];
const FM_VOICE_NAMES: [&str; 8] = ["FM 1", "FM 2", "FM 3", "FM 4", "FM 5", "FM 6", "PCM", "PSG"];

/// C++ `Ym_Emu<Ym2612_Emu>` (Vgm_Emu_Impl.h): tracks how far into the current
/// frame the chip has rendered, writing samples through a raw pointer into the
/// frame's FM output buffer. The pointer is wired per frame in `begin_frame`,
/// so moving `VgmEmu` between frames is safe.
struct YmEmu {
    emu: Ym2612NukedEmu,
    last_time: i32,
    out: *mut i16,
}

const YM_DISABLED_TIME: i32 = -1;

impl YmEmu {
    fn new() -> Self {
        Self {
            emu: Ym2612NukedEmu::new(),
            last_time: YM_DISABLED_TIME,
            out: std::ptr::null_mut(),
        }
    }

    fn enable(&mut self, b: bool) {
        self.last_time = if b { 0 } else { YM_DISABLED_TIME };
    }

    fn enabled(&self) -> bool {
        self.last_time != YM_DISABLED_TIME
    }

    fn begin_frame(&mut self, p: *mut i16) {
        debug_assert!(self.enabled());
        self.out = p;
        self.last_time = 0;
    }

    fn run_until(&mut self, time: i32) -> bool {
        let count = time - self.last_time;
        if count > 0 {
            if self.last_time < 0 {
                return false;
            }
            self.last_time = time;
            let p = self.out;
            unsafe {
                self.out = p.add(count as usize * Ym2612NukedEmu::OUT_CHAN_COUNT);
                let buf = std::slice::from_raw_parts_mut(
                    p,
                    count as usize * Ym2612NukedEmu::OUT_CHAN_COUNT,
                );
                self.emu.run(count, buf);
            }
        }
        true
    }
}

pub struct VgmEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub header: VgmHeader,

    psg: [SmsApu; 2],
    psg_dual: bool,
    psg_t6w28: bool,
    psg_rate: u32,
    // C++ Classic_Emu's Stereo_Buffer; PSG oscillators render into this
    // C++ gme_new_emu wraps VGM (type flags_ & 1) in Effects_Buffer(1)
    // with effects disabled — not Stereo_Buffer.
    stereo_buf: EffectsBuffer,
    // C++ Vgm_Emu::blip_buf; only used by the FM/DAC path
    blip_buf: BlipBuffer,

    vgm_rate: u32,
    blip_time_factor: u64,
    fm_time_factor: u64,
    // C++ `long fm_time_offset` (64-bit); stateful across frames
    fm_time_offset: i64,
    fm_rate: f64,
    uses_fm: bool,
    disable_oversampling: bool,

    ym2413: Vec<Ym2413>,
    ym2612: [YmEmu; 2],
    dual: DualResampler,
    // Scratch frame buffer the FM chips render into (C++ renders directly into
    // the resampler's input buffer; a scratch keeps the borrows disjoint)
    fm_buf: Vec<i16>,

    dac_synth: BlipSynth,

    pub pos: usize,
    pub data_end: usize,
    dac_amp: i32,
    dac_disabled: i32,

    pcm_data: usize,
    pcm_pos: usize,

    vgm_time: i32,
    loop_begin: usize,

    buf_initialized: bool,
}

impl Default for VgmEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl VgmEmu {
    pub fn new() -> Self {
        Self {
            state: GmeFileState::new(),
            emu_state: MusicEmuState::new(),
            header: VgmHeader {
                version: 0,
                psg_rate: 0,
                ym2413_rate: 0,
                gd3_offset: 0,
                track_duration: 0,
                loop_offset: 0,
                loop_duration: 0,
                noise_feedback: 0,
                noise_width: 0,
                ym2612_rate: 0,
                ym2151_rate: 0,
                data_offset: 0,
            },
            psg: [SmsApu::new(), SmsApu::new()],
            psg_dual: false,
            psg_t6w28: false,
            psg_rate: 0,
            stereo_buf: EffectsBuffer::new(1, false),
            blip_buf: BlipBuffer::new(),
            vgm_rate: VGM_RATE as u32,
            blip_time_factor: 0,
            fm_time_factor: 0,
            fm_time_offset: 0,
            fm_rate: 0.0,
            uses_fm: false,
            disable_oversampling: false,
            ym2413: Vec::new(),
            ym2612: [YmEmu::new(), YmEmu::new()],
            dual: DualResampler::new(),
            fm_buf: Vec::new(),
            dac_synth: BlipSynth::new(BLIP_MED_QUALITY),
            dac_amp: -1,
            dac_disabled: -1,
            pcm_data: 0,
            pcm_pos: 0,
            vgm_time: 0,
            pos: 0,
            loop_begin: 0,
            data_end: 0,
            buf_initialized: false,
        }
    }

    /// C++ `Vgm_Emu_Impl::update_fm_rates`: update pre-1.10 header FM rates
    /// by scanning commands.
    fn update_fm_rates(&self, ym2413_rate: &mut i64, ym2612_rate: &mut i64) {
        let data = &self.state.file_data;
        let mut p = 0x40usize;
        while p < data.len() {
            match data[p] {
                CMD_END => return,
                CMD_PSG | CMD_BYTE_DELAY => p += 2,
                CMD_DELAY => p += 3,
                CMD_DATA_BLOCK => {
                    if p + 7 > data.len() {
                        return;
                    }
                    p += 7 + get_le32(&data[p + 3..]) as usize;
                }
                CMD_YM2413 => {
                    *ym2612_rate = 0;
                    return;
                }
                CMD_YM2612_PORT0 | CMD_YM2612_PORT1 => {
                    *ym2612_rate = *ym2413_rate;
                    *ym2413_rate = 0;
                    return;
                }
                CMD_YM2151 => {
                    *ym2413_rate = 0;
                    *ym2612_rate = 0;
                    return;
                }
                cmd => p += command_len(cmd),
            }
        }
    }

    /// C++ `Vgm_Emu::setup_fm`
    fn setup_fm(&mut self) -> GmeResult {
        // C++ reads the raw header fields as `long`; dual-chip flags come from
        // the raw value, masking happens inside each branch
        let mut ym2612_rate = self.header.ym2612_rate as i64;
        let ym2612_dual = (ym2612_rate & 0x4000_0000) != 0;
        let mut ym2413_rate = self.header.ym2413_rate as i64;
        if ym2413_rate != 0 && self.header.version < 0x110 {
            self.update_fm_rates(&mut ym2413_rate, &mut ym2612_rate);
        }

        self.ym2413.clear();
        for chip in &mut self.ym2612 {
            chip.enable(false);
        }
        let clock = ym2413_rate as u32 & 0x3fff_ffff;
        if clock != 0 {
            let count = if self.header.ym2413_dual() { 2 } else { 1 };
            let is_vrc7 = ym2413_rate & 0x8000_0000 != 0;
            self.ym2413.extend(
                (0..count).map(|_| Ym2413::new(clock, self.psg_rate, self.emu_state.gain, is_vrc7)),
            );
        }
        self.uses_fm = !self.ym2413.is_empty();

        self.fm_rate = self.blip_buf.sample_rate() as f64 * OVERSAMPLE_FACTOR;

        if ym2612_rate != 0 {
            ym2612_rate &= 0x3FFF_FFFF; // C++: &= ~0xC0000000 (32-bit unsigned mask)
            self.uses_fm = true;
            if self.disable_oversampling {
                self.fm_rate = ym2612_rate as f64 / 144.0;
            }
            self.ym2612[0]
                .emu
                .set_rate(self.fm_rate, ym2612_rate as f64);
            self.ym2612[0].enable(true);
            if ym2612_dual {
                self.ym2612[1]
                    .emu
                    .set_rate(self.fm_rate, ym2612_rate as f64);
                self.ym2612[1].enable(true);
            }
        }

        if self.uses_fm {
            self.dual.setup(
                self.fm_rate / self.blip_buf.sample_rate() as f64,
                ROLLOFF,
                FM_GAIN * self.emu_state.gain,
            );
            let pairs =
                self.blip_buf.length_ms() as i64 * self.blip_buf.sample_rate() as i64 / 1000;
            self.dual.reset(pairs as usize);
            self.fm_buf = vec![0; self.dual.resampler_size];
            self.psg[0].volume(0.135 * FM_GAIN * self.emu_state.gain);
            if self.psg_dual {
                self.psg[1].volume(0.135 * FM_GAIN * self.emu_state.gain);
            }
        } else {
            // C++ sets both PSG volumes unconditionally here
            self.psg[0].volume(self.emu_state.gain);
            self.psg[1].volume(self.emu_state.gain);
        }

        self.emu_state.voice_names_static =
            self.voice_names().iter().map(|s| s.to_string()).collect();
        self.emu_state.voice_count = self.emu_state.voice_names_static.len() as i32;
        Ok(())
    }

    /// Post-load setup requiring both the parsed header and the sample rate.
    /// C++ does this at the end of Vgm_Emu::load_mem_ (setup_fm +
    /// Classic_Emu::setup_buffer, which applies the constructor's equalizer:
    /// -14 dB treble, 80 Hz bass).
    fn setup_classic(&mut self) -> GmeResult {
        self.setup_fm()?;
        self.stereo_buf.clock_rate(self.psg_rate);
        self.set_equalizer_impl(&Equalizer::new(-14.0, 80.0));
        self.set_tempo_internal();
        Ok(())
    }

    fn set_tempo_internal(&mut self) {
        if self.psg_rate == 0 {
            return;
        }
        let t = self.emu_state.tempo;
        self.vgm_rate = (VGM_RATE as f64 * t + 0.5) as u32;
        self.blip_time_factor = ((1u64 << BLIP_TIME_BITS) as f64 / self.vgm_rate as f64
            * self.psg_rate as f64
            + 0.5) as u64;
        self.fm_time_factor =
            2 + (self.fm_rate * (1u64 << FM_TIME_BITS) as f64 / self.vgm_rate as f64 + 0.5) as u64;
    }

    fn to_blip_time(&self, vgm_time: i32) -> BlipTime {
        ((vgm_time as u64 * self.blip_time_factor) >> BLIP_TIME_BITS) as BlipTime
    }

    // C++ `Vgm_Emu_Impl::to_fm_time`: (t * fm_time_factor + fm_time_offset) >> fm_time_bits
    // computed in uint64 with int -> uint64 sign extension
    fn to_fm_time(&self, vgm_time: i32) -> i32 {
        ((vgm_time as i64 as u64)
            .wrapping_mul(self.fm_time_factor)
            .wrapping_add(self.fm_time_offset as u64)
            >> FM_TIME_BITS) as i32
    }

    fn write_pcm(&mut self, vgm_time: i32, amp: i32) {
        let blip_time = self.to_blip_time(vgm_time);
        let old = self.dac_amp;
        let delta = amp - old;
        self.dac_amp = amp;
        if old >= 0 {
            self.dac_synth
                .offset_at(blip_time, delta, &mut self.blip_buf);
        } else {
            self.dac_amp |= self.dac_disabled;
        }
    }

    fn run_commands(&mut self, end_time: i32, frame_clocks: BlipTime) -> BlipTime {
        let mut vgm_time = self.vgm_time;
        let mut pos = self.pos;

        if pos >= self.data_end {
            self.emu_state.emu_track_ended = true;
        }

        while vgm_time < end_time && pos < self.data_end {
            let cmd = self.state.file_data[pos];
            pos += 1;

            match cmd {
                CMD_END => {
                    pos = self.loop_begin;
                }
                CMD_DELAY_735 => {
                    vgm_time += 735;
                }
                CMD_DELAY_882 => {
                    vgm_time += 882;
                }
                CMD_GG_STEREO => {
                    if pos < self.state.file_data.len() {
                        let d = self.state.file_data[pos] as i32;
                        pos += 1;
                        self.psg[0].write_ggstereo(self.to_blip_time(vgm_time), d);
                    }
                }
                CMD_PSG => {
                    if pos < self.state.file_data.len() {
                        let d = self.state.file_data[pos] as i32;
                        pos += 1;
                        self.psg[0].write_data(self.to_blip_time(vgm_time), d);
                    }
                }
                CMD_GG_STEREO_2 => {
                    if pos < self.state.file_data.len() {
                        let d = self.state.file_data[pos] as i32;
                        pos += 1;
                        self.psg[1].write_ggstereo(self.to_blip_time(vgm_time), d);
                    }
                }
                CMD_PSG_2 => {
                    if pos < self.state.file_data.len() {
                        let d = self.state.file_data[pos] as i32;
                        pos += 1;
                        self.psg[1].write_data(self.to_blip_time(vgm_time), d);
                    }
                }
                CMD_DELAY => {
                    if pos + 1 < self.state.file_data.len() {
                        let d = self.state.file_data[pos] as i32
                            + ((self.state.file_data[pos + 1] as i32) << 8);
                        vgm_time += d;
                        pos += 2;
                    }
                }
                CMD_BYTE_DELAY => {
                    if pos < self.state.file_data.len() {
                        vgm_time += self.state.file_data[pos] as i32;
                        pos += 1;
                    }
                }
                CMD_YM2413 | CMD_YM2413_2 => {
                    if let Some(bytes) = self.state.file_data.get(pos..pos + 2) {
                        let time = self.to_blip_time(vgm_time).min(frame_clocks);
                        let index = usize::from(cmd == CMD_YM2413_2);
                        if let Some(chip) = self.ym2413.get_mut(index) {
                            chip.run_until(time, &mut self.blip_buf, self.emu_state.mute_mask);
                            chip.write(bytes[0], bytes[1]);
                        }
                    }
                    pos += 2;
                }
                CMD_YM2612_PORT0 => {
                    if pos + 1 < self.state.file_data.len() {
                        let addr = self.state.file_data[pos];
                        let data_val = self.state.file_data[pos + 1];
                        if addr == YM2612_DAC_PORT {
                            self.write_pcm(vgm_time, data_val as i32);
                        } else {
                            let t = self.to_fm_time(vgm_time);
                            if self.ym2612[0].run_until(t) {
                                if addr == 0x2B {
                                    self.dac_disabled = ((data_val >> 7) & 1) as i32 - 1;
                                    self.dac_amp |= self.dac_disabled;
                                }
                                self.ym2612[0].emu.write0(addr as i32, data_val as i32);
                            }
                        }
                        pos += 2;
                    }
                }
                CMD_YM2612_PORT1 => {
                    if pos + 1 < self.state.file_data.len() {
                        let addr = self.state.file_data[pos];
                        let data_val = self.state.file_data[pos + 1];
                        let t = self.to_fm_time(vgm_time);
                        if self.ym2612[0].run_until(t) {
                            self.ym2612[0].emu.write1(addr as i32, data_val as i32);
                        }
                        pos += 2;
                    }
                }
                CMD_YM2612_2_PORT0 => {
                    if pos + 1 < self.state.file_data.len() {
                        let addr = self.state.file_data[pos];
                        let data_val = self.state.file_data[pos + 1];
                        if addr == YM2612_DAC_PORT {
                            self.write_pcm(vgm_time, data_val as i32);
                        } else {
                            let t = self.to_fm_time(vgm_time);
                            if self.ym2612[1].run_until(t) {
                                if addr == 0x2B {
                                    self.dac_disabled = ((data_val >> 7) & 1) as i32 - 1;
                                    self.dac_amp |= self.dac_disabled;
                                }
                                self.ym2612[1].emu.write0(addr as i32, data_val as i32);
                            }
                        }
                        pos += 2;
                    }
                }
                CMD_YM2612_2_PORT1 => {
                    if pos + 1 < self.state.file_data.len() {
                        let addr = self.state.file_data[pos];
                        let data_val = self.state.file_data[pos + 1];
                        let t = self.to_fm_time(vgm_time);
                        if self.ym2612[1].run_until(t) {
                            self.ym2612[1].emu.write1(addr as i32, data_val as i32);
                        }
                        pos += 2;
                    }
                }
                CMD_DATA_BLOCK => {
                    if pos + 6 <= self.state.file_data.len() && self.state.file_data[pos] == CMD_END
                    {
                        let block_type = self.state.file_data[pos + 1];
                        let size = get_le32(&self.state.file_data[pos + 2..]) as usize;
                        pos += 6;
                        if block_type == PCM_BLOCK_TYPE {
                            self.pcm_data = pos;
                        }
                        pos += size;
                    }
                }
                CMD_PCM_SEEK => {
                    if pos + 4 <= self.state.file_data.len() {
                        let offset = self.state.file_data[pos] as usize
                            + ((self.state.file_data[pos + 1] as usize) << 8)
                            + ((self.state.file_data[pos + 2] as usize) << 16)
                            + ((self.state.file_data[pos + 3] as usize) << 24);
                        self.pcm_pos = self.pcm_data + offset;
                        pos += 4;
                    }
                }
                _ => {
                    match cmd & 0xF0 {
                        0x80 => {
                            // PCM delay: play sample + wait
                            if self.pcm_pos < self.state.file_data.len() {
                                let amp = self.state.file_data[self.pcm_pos] as i32;
                                self.pcm_pos += 1;
                                self.write_pcm(vgm_time, amp);
                            }
                            vgm_time += (cmd & 0x0F) as i32;
                        }
                        0x70 => {
                            vgm_time += ((cmd & 0x0F) + 1) as i32;
                        }
                        0x50 => {
                            pos += 2;
                        }
                        _ => {
                            let len = command_len(cmd);
                            pos += len - 1;
                        }
                    }
                }
            }
        }

        vgm_time -= end_time;
        self.pos = pos;
        self.vgm_time = vgm_time;

        self.to_blip_time(end_time)
    }

    // C++ Vgm_Emu::run_clocks
    fn run_clocks(&mut self, time_io: &mut BlipTime, msec: i32) {
        *time_io = self.run_commands(msec * self.vgm_rate as i32 / 1000, *time_io);
        self.psg[0].end_frame(*time_io);
        if self.psg_dual {
            self.psg[1].end_frame(*time_io);
        }
    }

    // C++ Classic_Emu::play_
    fn play_classic(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let mut remain = count as i64;
        let mut pos = 0usize;
        while remain > 0 {
            let read = self
                .stereo_buf
                .read_samples(&mut out[pos..pos + remain as usize], remain);
            remain -= read;
            pos += read as usize;
            if remain > 0 {
                let msec = self.stereo_buf.length();
                let mut clocks_emulated = (msec as i64 * self.psg_rate as i64 / 1000) as BlipTime;
                self.run_clocks(&mut clocks_emulated, msec);
                self.stereo_buf.end_frame(clocks_emulated);
            }
        }
        Ok(())
    }

    // C++ Vgm_Emu_Impl::play_frame: runs commands and FM chips for one frame,
    // rendering pairs*2 FM samples into fm_buf. Returns the sample count.
    fn play_frame(&mut self, blip_time: BlipTime, sample_count: i32) -> i32 {
        // to do (from C++): timing is working mostly by luck
        let min_pairs = sample_count >> 1;
        // C++: int vgm_time = ((long) min_pairs << fm_time_bits) / fm_time_factor - 1;
        // (i64 / u64 -> unsigned division, then truncated to int)
        let mut vgm_time = ((((min_pairs as i64) << FM_TIME_BITS) as u64 / self.fm_time_factor)
            .wrapping_sub(1)) as i32;
        let mut pairs = self.to_fm_time(vgm_time);
        while pairs < min_pairs {
            vgm_time += 1;
            pairs = self.to_fm_time(vgm_time);
        }

        if self.ym2612[0].enabled() {
            let p = self.fm_buf.as_mut_ptr();
            self.ym2612[0].begin_frame(p);
            if self.ym2612[1].enabled() {
                self.ym2612[1].begin_frame(p);
            }
        }
        self.fm_buf[..pairs as usize * STEREO].fill(0);

        self.run_commands(vgm_time, blip_time);
        for chip in &mut self.ym2413 {
            chip.end_frame(blip_time, &mut self.blip_buf, self.emu_state.mute_mask);
        }

        if self.ym2612[0].enabled() {
            self.ym2612[0].run_until(pairs);
        }
        if self.ym2612[1].enabled() {
            self.ym2612[1].run_until(pairs);
        }

        // C++: fm_time_offset = (vgm_time * fm_time_factor + fm_time_offset)
        //          - ((long) pairs << fm_time_bits);   [uint64 wrapping math]
        self.fm_time_offset = ((vgm_time as i64 as u64)
            .wrapping_mul(self.fm_time_factor)
            .wrapping_add(self.fm_time_offset as u64)
            .wrapping_sub(((pairs as i64) << FM_TIME_BITS) as u64))
            as i64;

        self.psg[0].end_frame(blip_time);
        if self.psg_dual {
            self.psg[1].end_frame(blip_time);
        }

        pairs * STEREO as i32
    }

    // C++ Dual_Resampler::play_frame_. Unlike C++ (which mixes into `out`),
    // this always mixes in place into dual.sample_buf; callers copy out of it.
    // The values are identical: C++'s "extra" path also mixes into sample_buf.
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

    // C++ Dual_Resampler::mix_samples: mixes the mono PSG/DAC blip buffer into
    // the resampled FM stereo stream (here in place in dual.sample_buf),
    // reading the blip buffer through Blip_Reader semantics.
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

    // C++ Dual_Resampler::dual_play (called from Vgm_Emu::play_ when uses_fm)
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

    // C++ Vgm_Emu::play_
    fn play_dispatch(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        if !self.uses_fm {
            return self.play_classic(count, out);
        }
        self.dual_play(count as i64, out);
        Ok(())
    }

    /// C++ Music_Emu::fill_buf: render one silence-detection buffer ahead of
    /// the output position, holding it in emu_state.silence_buf.
    fn fill_buf(&mut self) -> GmeResult {
        debug_assert!(self.emu_state.buf_remain == 0);
        if !self.emu_state.emu_track_ended {
            // C++ emu_play: emulator time advances by what was generated
            self.emu_state.emu_time += SILENCE_BUF_SIZE as i32;
            let mut buf = std::mem::take(&mut self.emu_state.silence_buf);
            let result = self.play_dispatch(SILENCE_BUF_SIZE as i32, &mut buf);
            self.emu_state.silence_buf = buf;
            result?;
            let silence = count_silence(&mut self.emu_state.silence_buf);
            if silence < SILENCE_BUF_SIZE {
                self.emu_state.silence_time = (self.emu_state.emu_time as i64) - silence;
                self.emu_state.buf_remain = SILENCE_BUF_SIZE;
                return Ok(());
            }
        }
        self.emu_state.silence_count += SILENCE_BUF_SIZE;
        Ok(())
    }

    // C++ Vgm_Emu::set_voice (via Classic_Emu::mute_voices_)
    fn set_voice(
        &mut self,
        i: usize,
        center: *mut BlipBuffer,
        left: *mut BlipBuffer,
        right: *mut BlipBuffer,
    ) {
        if self.psg_dual {
            if self.psg_t6w28 {
                // TODO (from C++): proper output of each PSG chip: 0 - all right, 1 - all left
                self.psg[0].osc_output(i, center, right, right);
                self.psg[1].osc_output(i, center, left, left);
            } else {
                self.psg[0].osc_output(i, center, left, right);
                self.psg[1].osc_output(i, center, left, right);
            }
        } else {
            self.psg[0].osc_output(i, center, left, right);
        }
    }
}

impl GmeFile for VgmEmu {
    fn format(&self) -> Format {
        Format::Vgm
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.raw_track_count = n;
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        if data.len() < VGM_MIN_FILE_SIZE {
            return Err(GmeError::WrongFileType);
        }

        let hdr = VgmHeader::parse(data).ok_or(GmeError::WrongFileType)?;
        self.header = hdr.clone();

        // C++: raw value 0 defaults to 3579545; cap applies before flag extraction
        let mut psg_rate = hdr.psg_rate;
        if psg_rate == 0 {
            psg_rate = 3_579_545;
        } else if psg_rate > 100_000_000 {
            psg_rate = 100_000_000;
        }
        self.psg_dual = psg_rate & 0x4000_0000 != 0;
        self.psg_t6w28 = psg_rate & 0x8000_0000 != 0;
        self.psg_rate = psg_rate & 0x0FFF_FFFF;
        self.blip_buf.clock_rate(self.psg_rate);

        self.state.file_data = data.to_vec();
        self.state.set_track_count(1); // VGM files hold a single track

        // Set voice names
        if hdr.uses_fm() {
            self.emu_state.voice_count = 8;
            self.emu_state.voice_names_static =
                FM_VOICE_NAMES.iter().map(|s| s.to_string()).collect();
        } else {
            self.emu_state.voice_count = 4;
            self.emu_state.voice_names_static =
                PSG_VOICE_NAMES.iter().map(|s| s.to_string()).collect();
        }

        // C++ requires the sample rate before load; this crate allows either
        // order, so finish buffer setup once both are known
        if self.emu_state.sample_rate != 0 {
            self.setup_classic()?;
        }

        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, _track: i32) -> GmeResult {
        header::get_vgm_track_info(&self.state.file_data, info);
        Ok(())
    }
}

impl MusicEmulator for VgmEmu {
    fn format(&self) -> Format {
        Format::Vgm
    }

    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        self.emu_state.sample_rate = sample_rate;
        // C++ Vgm_Emu::set_sample_rate_: FM blip buffer uses 1000/30 ms
        self.blip_buf
            .set_sample_rate(sample_rate, 1000 / 30)
            .map_err(|_| GmeError::Internal("Failed to set sample rate"))?;
        // C++ Classic_Emu::set_sample_rate_: stereo buffer uses 1000/20 ms
        self.stereo_buf
            .set_sample_rate(sample_rate, 1000 / 20)
            .map_err(|_| GmeError::Internal("Failed to set sample rate"))?;
        self.fm_rate = sample_rate as f64;
        self.buf_initialized = true;
        // If the file was loaded before the sample rate was set, finish setup now
        if self.psg_rate != 0 {
            self.setup_classic()?;
        }
        Ok(())
    }

    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;

        // Reinit internal pointers (struct may have been moved since creation)
        self.psg[0].reinit_pointers();
        if self.psg_dual {
            self.psg[1].reinit_pointers();
        }

        self.psg[0].reset(self.header.noise_feedback, self.header.noise_width as i32);
        if self.psg_dual {
            self.psg[1].reset(self.header.noise_feedback, self.header.noise_width as i32);
        }

        let data = &self.state.file_data;
        self.data_end = data.len();
        self.dac_disabled = -1;
        self.pos = HEADER_SIZE;
        self.pcm_data = HEADER_SIZE;
        self.pcm_pos = HEADER_SIZE;
        self.dac_amp = -1;
        self.vgm_time = 0;

        // Handle VGM data offset for version >= 1.50
        if self.header.version >= 0x150 {
            let data_offset = self.header.data_offset;
            if data_offset != 0 {
                // data_offset is relative to 0x34 in header, we're at HEADER_SIZE (0x40)
                // net offset = data_offset + 0x34 - 0x40 = data_offset - 12
                self.pos = HEADER_SIZE + data_offset as usize - 12;
            }
        }

        // Set up loop point
        self.loop_begin = self.data_end;
        if self.header.loop_offset != 0 {
            let loop_off = self.header.loop_offset as usize;
            let loop_abs = loop_off + 0x1C; // offset of loop_offset field in header
            if loop_abs < self.data_end {
                self.loop_begin = loop_abs;
            }
        }

        // C++ Classic_Emu::start_track_ clears the stereo buffer; the FM blip
        // buffer is only cleared on the FM path (Vgm_Emu::start_track_)
        self.stereo_buf.clear();
        if self.uses_fm {
            if self.ym2612[0].enabled() {
                self.ym2612[0].emu.reset();
            }
            if self.ym2612[1].enabled() {
                self.ym2612[1].emu.reset();
            }
            for chip in &mut self.ym2413 {
                chip.reset();
            }
            self.fm_time_offset = 0;
            self.blip_buf.clear(true);
            self.dual.clear();
        }

        // C++ Music_Emu::start_track → remute_voices(): wires each voice to
        // the stereo buffer channels (or null if muted)
        self.mute_voices_impl(self.emu_state.mute_mask);

        // (C++ has no autoload fade for VGM — only Spc_Emu sets one.)

        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;
        self.emu_state.emu_time = 0;
        self.emu_state.buf_remain = 0;
        self.emu_state.silence_time = 0;
        self.emu_state.silence_count = 0;
        self.emu_state.out_time = 0;
        self.emu_state.out_time_scaled = 0;

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

    // C++ Music_Emu::play (structure proven bit-exact by gme-spc/gme-nsf)
    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let out_count = count as usize;

        if !self.buf_initialized || self.emu_state.track_ended {
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
                self.play_dispatch(remain, &mut out[pos..])?;
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
            let threshold = 30000;
            let mut scratch = [0i16; SILENCE_BUF_SIZE as usize];
            if count > threshold {
                let saved_mute = self.emu_state.mute_mask;
                self.mute_voices_impl(!0);

                while count > threshold / 2 && !self.emu_state.emu_track_ended {
                    self.play_dispatch(SILENCE_BUF_SIZE as i32, &mut scratch)?;
                    count -= SILENCE_BUF_SIZE as i32;
                }

                self.mute_voices_impl(saved_mute);
            }

            while count > 0 && !self.emu_state.emu_track_ended {
                let n = (count as i64).min(SILENCE_BUF_SIZE) as i32;
                count -= n;
                self.play_dispatch(n, &mut scratch[..n as usize])?;
            }
        }

        if self.emu_state.silence_count == 0 && self.emu_state.buf_remain == 0 {
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.set_tempo_range(tempo);
        self.set_tempo_internal();
    }

    fn set_equalizer_impl(&mut self, settings: &Equalizer) {
        let eq = BlipEq::new(settings.treble);
        self.psg[0].treble_eq(&eq);
        if self.psg_dual {
            self.psg[1].treble_eq(&eq);
        }
        for chip in &mut self.ym2413 {
            chip.treble_eq(&eq);
        }
        self.dac_synth.treble_eq(&eq);
        self.stereo_buf.bass_freq(settings.bass as i32);
    }

    fn set_stereo_depth_impl(&mut self, depth: f64) {
        self.stereo_buf.set_depth(depth);
        self.mute_voices_impl(self.emu_state.mute_mask);
    }

    fn mute_voices_impl(&mut self, mask: i32) {
        self.emu_state.mute_mask = mask;
        let null = std::ptr::null_mut();

        // C++ Classic_Emu::mute_voices_: wire each voice to the stereo buffer
        // channels via set_voice, or to null when muted
        let voice_count = if self.uses_fm { 8 } else { 4 };
        for i in (0..voice_count).rev() {
            if mask & (1 << i) != 0 {
                self.set_voice(i, null, null, null);
            } else {
                // C++ Vgm_Emu ctor voice types
                const VOICE_TYPES: [i32; 4] = [
                    CHANNEL_WAVE_TYPE | 1,
                    CHANNEL_WAVE_TYPE,
                    CHANNEL_WAVE_TYPE | 2,
                    CHANNEL_NOISE_TYPE,
                ];
                let ch = self.stereo_buf.channel(i as i32, VOICE_TYPES[i & 3]);
                self.set_voice(i, ch.center, ch.left, ch.right);
            }
        }

        if self.uses_fm {
            // C++ Vgm_Emu::mute_voices_: PSG is wired by output() (bit 7),
            // DAC by volume (bit 6)
            let center = &mut self.blip_buf as *mut BlipBuffer;
            if mask & 0x80 != 0 {
                self.psg[0].output_mono(null);
                if self.psg_dual {
                    self.psg[1].output_mono(null);
                }
            } else {
                self.psg[0].output_mono(center);
                if self.psg_dual {
                    self.psg[1].output_mono(center);
                }
            }
            if self.ym2612[0].enabled() {
                if mask & 0x40 != 0 {
                    self.dac_synth.volume_unit(0.0);
                } else {
                    self.dac_synth
                        .volume_unit(0.1115 / 256.0 * FM_GAIN * self.emu_state.gain);
                }
                self.ym2612[0].emu.mute_voices(mask);
                if self.ym2612[1].enabled() {
                    self.ym2612[1].emu.mute_voices(mask);
                }
            }
        }
    }

    fn voice_count(&self) -> i32 {
        self.emu_state.voice_count
    }

    fn voice_names(&self) -> &[&str] {
        if !self.ym2413.is_empty() {
            if self.ym2612[0].enabled() {
                &[
                    "FM 1",
                    "FM 2",
                    "FM 3",
                    "FM 4",
                    "FM 5",
                    "FM 6–9",
                    "PCM / Rhythm",
                    "PSG",
                ]
            } else {
                &[
                    "FM 1", "FM 2", "FM 3", "FM 4", "FM 5", "FM 6–9", "Rhythm", "PSG",
                ]
            }
        } else if self.uses_fm {
            &FM_VOICE_NAMES
        } else {
            &PSG_VOICE_NAMES
        }
    }
}

unsafe impl Send for VgmEmu {}
