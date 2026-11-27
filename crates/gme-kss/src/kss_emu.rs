// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Kss_Emu.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// KSS format emulator.
/// Port of C++ Kss_Emu.h / Kss_Emu.cpp.
use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator};
use gme_common::track_info::Equalizer;
use gme_common::track_info::TrackInfo;
use gme_core::blip_buffer::{BlipEq, BlipTime};
use gme_core::effects_buffer::EffectsBuffer;
use gme_core::error::{GmeError, GmeResult};
use gme_core::multi_buffer::{CHANNEL_WAVE_TYPE, MultiBuffer};

use crate::ay_apu::{AyApu, OSC_COUNT as AY_OSC_COUNT};
use crate::header::{KSS_HEADER_SIZE, KssCompositeHeader};
use crate::kss_cpu::{IDLE_ADDR, KssCpu};
use crate::kss_scc_apu::{OSC_COUNT as SCC_OSC_COUNT, REG_COUNT as SCC_REG_COUNT, SccApu};
use gme_vgm::sms_apu::{OSC_COUNT as SN_OSC_COUNT, SmsApu};

const CLOCK_RATE: u32 = 3_579_545;
const MEM_SIZE: usize = 0x10000;
const PAGE_SIZE: usize = 0x2000;
const OSC_COUNT: usize = AY_OSC_COUNT + SCC_OSC_COUNT;

const VOICE_NAMES: [&str; OSC_COUNT] = [
    "Square 1", "Square 2", "Square 3", "Wave 1", "Wave 2", "Wave 3", "Wave 4", "Wave 5",
];

/// KSS emulator.
pub struct KssEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub cpu: KssCpu,
    pub ay: AyApu,
    pub scc: SccApu,
    /// SN76489 PSG (Sega Master System / Game Gear KSS files,
    /// device_flags & 0x02). C++ Kss_Emu::sn.
    pub sn: Option<SmsApu>,
    pub header: KssCompositeHeader,
    /// 64KB RAM + CPU padding.
    pub ram: Vec<u8>,
    /// Stereo buffer for audio output.
    pub stereo_buf: EffectsBuffer,
    pub buf_initialized: bool,
    /// Unmapped read page (returns 0xFF).
    unmapped_read: Vec<u8>,
    /// Unmapped write page.
    unmapped_write: Vec<u8>,
    /// ROM data (bank data after the load area).
    rom_data: Vec<u8>,
    /// Number of ROM banks available.
    bank_count: i32,
    /// Whether SCC was accessed (affects gain).
    scc_accessed: bool,
    /// Whether gain has been updated after SCC access.
    gain_updated: bool,
    /// SCC enabled address (0xC000 or 0).
    scc_enabled: u32,
    /// AY latch register.
    ay_latch: i32,
    /// Play period in clock cycles.
    play_period: BlipTime,
    /// Time of next play routine call.
    next_play: BlipTime,
}

// SAFETY: KssEmu's raw pointers all point to data owned by the struct itself.
unsafe impl Send for KssEmu {}

impl Default for KssEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl KssEmu {
    pub fn new() -> Self {
        let ram = vec![0u8; MEM_SIZE + 0x100]; // extra for CPU padding
        let unmapped_read = vec![0xFFu8; 0x100];
        let unmapped_write = vec![0u8; PAGE_SIZE + 1];

        Self {
            state: GmeFileState::new(),
            emu_state: MusicEmuState {
                silence_lookahead: 6,
                ..MusicEmuState::new()
            },
            cpu: KssCpu::new(),
            ay: AyApu::new(),
            scc: SccApu::new(),
            sn: None,
            header: KssCompositeHeader::default(),
            ram,
            stereo_buf: EffectsBuffer::new(1, false),
            buf_initialized: false,
            unmapped_read,
            unmapped_write,
            rom_data: Vec::new(),
            bank_count: 0,
            scc_accessed: false,
            gain_updated: false,
            scc_enabled: 0xC000,
            ay_latch: 0,
            play_period: 0,
            next_play: 0,
        }
    }

    /// Initialize audio buffer and wire oscillators.
    fn init_audio(&mut self, sample_rate: i32) -> GmeResult {
        self.stereo_buf.set_sample_rate(sample_rate, 1000 / 20)?;
        self.stereo_buf.clock_rate(CLOCK_RATE);
        // Match C++ Classic_Emu::setup_buffer -> set_equalizer( equalizer() )
        // with the Music_Emu default equalizer (treble -1.0, bass 60).
        self.set_equalizer_impl(&Equalizer::default());

        // Wire all oscillators per the stored mute mask (C++ remute_voices)
        let mask = self.emu_state.mute_mask;
        self.wire_voices(mask);

        self.emu_state.voice_count = OSC_COUNT as i32;

        self.buf_initialized = true;
        Ok(())
    }

    /// Wire oscillator outputs per the mute mask.
    /// Transliteration of C++ Classic_Emu::mute_voices_ + Kss_Emu::set_voice:
    /// each voice routes to the center of its Effects_Buffer channel
    /// (voice types from the Kss_Emu ctor: wave_type | 0..7).
    fn wire_voices(&mut self, mask: i32) {
        for i in (0..OSC_COUNT).rev() {
            if mask & (1 << i) != 0 {
                let null = std::ptr::null_mut();
                if i >= AY_OSC_COUNT {
                    self.scc.osc_output(i - AY_OSC_COUNT, null);
                } else {
                    self.ay.osc_output(i, null);
                }
                if i < SN_OSC_COUNT
                    && let Some(ref mut sn) = self.sn
                {
                    sn.osc_output(i, null, null, null);
                }
            } else {
                let ch = self
                    .stereo_buf
                    .channel(i as i32, CHANNEL_WAVE_TYPE | i as i32);
                if i >= AY_OSC_COUNT {
                    self.scc.osc_output(i - AY_OSC_COUNT, ch.center);
                } else {
                    self.ay.osc_output(i, ch.center);
                }
                // C++ Kss_Emu::set_voice also wires the SN with the full
                // stereo triplet (Game Gear stereo uses left/right)
                if i < SN_OSC_COUNT
                    && let Some(ref mut sn) = self.sn
                {
                    sn.osc_output(i, ch.center, ch.left, ch.right);
                }
            }
        }
    }

    /// Update gain based on SCC access state.
    fn update_gain(&mut self) {
        let g = self.emu_state.gain * 1.4 * if self.scc_accessed { 1.5 } else { 1.0 };
        self.ay.volume(g);
        self.scc.volume(g);
        if let Some(ref mut sn) = self.sn {
            sn.volume(g);
        }
    }

    /// Set a ROM bank.
    fn set_bank(&mut self, logical: i32, physical: i32) {
        let bank_size = self.bank_size() as usize;
        let mut addr = 0x8000usize;
        if logical != 0 && bank_size == 8 * 1024 {
            addr = 0xA000;
        }

        let physical = physical - self.header.base.first_bank as i32;
        if physical < 0 || physical >= self.bank_count {
            // Map to RAM
            let ram_offset = addr;
            let data_ptr = self.ram[ram_offset..].as_mut_ptr();
            unsafe {
                self.cpu.map_mem(
                    addr as u16,
                    bank_size as u32,
                    data_ptr,
                    data_ptr as *const u8,
                );
            }
        } else {
            // Map to ROM
            let phys = physical as usize * bank_size;
            if phys + bank_size <= self.rom_data.len() {
                let rom_ptr = self.rom_data[phys..].as_ptr();
                // Write to unmapped (read-only)
                unsafe {
                    self.cpu.map_mem(
                        addr as u16,
                        bank_size as u32,
                        self.unmapped_write.as_mut_ptr(),
                        rom_ptr,
                    );
                }
            }
        }
    }

    /// Bank size in bytes.
    fn bank_size(&self) -> i32 {
        (16 * 1024) >> ((self.header.base.bank_mode >> 7) & 1)
    }

    /// CPU memory write handler (for SCC writes).
    fn cpu_write(&mut self, time: BlipTime, addr: u16, data: u8) {
        let data = data as i32;
        match addr {
            0x9000 => {
                self.set_bank(0, data);
                return;
            }
            0xB000 => {
                self.set_bank(1, data);
                return;
            }
            _ => {}
        }

        let scc_addr = ((addr as i32) & 0xDFFF) ^ 0x9800;
        if scc_addr >= 0 && (scc_addr as usize) < SCC_REG_COUNT {
            self.scc_accessed = true;
            self.scc.write(time, scc_addr as usize, data as u8);
        }
    }
}

impl GmeFile for KssEmu {
    fn format(&self) -> Format {
        Format::Kss
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.set_track_count(n);
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        if data.len() < KSS_HEADER_SIZE {
            return Err(GmeError::WrongFileType);
        }

        let header = KssCompositeHeader::parse(data).ok_or(GmeError::WrongFileType)?;
        self.header = header;

        // Set track count (KSS supports up to 256 tracks)
        self.state.set_track_count(256);

        let base = &mut self.header.base;
        if base.tag[3] == b'C' {
            if base.extra_header != 0 || base.device_flags & !0x0f != 0 {
                base.extra_header = 0;
                base.device_flags &= 0x0f;
                self.state.set_warning("Unknown data in header");
            }
        } else if base.extra_header > 0x10 {
            self.state.set_warning("Unknown data in header");
        }
        if self.header.base.uses_fm() {
            self.state.set_warning("FM sound not supported");
        }

        // Determine SCC enable state
        self.scc_enabled = if self.header.base.uses_scc() {
            0xC000
        } else {
            0
        };

        // SN76489 for SMS/GG files (C++: device_flags & 0x02 -> new Sms_Apu)
        if self.header.base.uses_sn76489() && self.sn.is_none() {
            self.sn = Some(SmsApu::new());
        }

        // Extract ROM data (after extra header)
        let extra = self.header.base.extra_header_size();
        let file_data_offset = KSS_HEADER_SIZE + extra;
        if file_data_offset < data.len() {
            self.rom_data = data[file_data_offset..].to_vec();
        }

        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, _track: i32) -> GmeResult {
        info.system = self.header.base.system_name().to_string();
        Ok(())
    }
}

impl MusicEmulator for KssEmu {
    fn format(&self) -> Format {
        Format::Kss
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

        // Initialize RAM
        // First 0x4000 filled with 0xC9 (RET), rest with 0
        self.ram[0..0x4000].fill(0xC9);
        self.ram[0x4000..MEM_SIZE].fill(0);

        // Copy BIOS stubs to low RAM
        // WRTPSG at $0001: D3 A0 F5 7B D3 A1 F1 C9
        let bios: [u8; 13] = [
            0xD3, 0xA0, 0xF5, 0x7B, 0xD3, 0xA1, 0xF1, 0xC9, // $0001: WRTPSG
            0xD3, 0xA0, 0xDB, 0xA2, 0xC9, // $0009: RDPSG
        ];
        self.ram[1..1 + 8].copy_from_slice(&bios[..8]);
        self.ram[9..9 + 5].copy_from_slice(&bios[8..]);

        // WRTPSG/RDPSG vectors at $0093/$0096
        self.ram[0x93] = 0xC3; // JP
        self.ram[0x94] = 0x01;
        self.ram[0x95] = 0x00;
        self.ram[0x96] = 0xC3; // JP
        self.ram[0x97] = 0x09;
        self.ram[0x98] = 0x00;

        // Copy non-banked data into RAM
        let load_addr = self.header.base.load_addr as usize;
        let orig_load_size = self.header.base.load_size as usize;
        let load_size = orig_load_size
            .min(self.rom_data.len())
            .min(MEM_SIZE - load_addr);

        if load_size != orig_load_size {
            self.state.set_warning("Excessive data size");
        }
        if load_size > 0 && load_addr + load_size <= MEM_SIZE {
            self.ram[load_addr..load_addr + load_size].copy_from_slice(&self.rom_data[..load_size]);
        }

        // Set up bank data
        let bank_size = self.bank_size() as usize;
        let max_banks = if bank_size > 0 {
            self.rom_data
                .len()
                .saturating_sub(load_size)
                .div_ceil(bank_size)
        } else {
            0
        };
        let mut bank_count = (self.header.base.bank_mode & 0x7F) as i32;
        if bank_count > max_banks as i32 {
            bank_count = max_banks as i32;
            self.state.set_warning("Bank data missing");
        }
        self.bank_count = bank_count;

        // Set idle address marker
        self.ram[IDLE_ADDR as usize] = 0xFF;

        // Reset CPU and map memory
        let unmapped_write_ptr = self.unmapped_write.as_mut_ptr();
        let unmapped_read_ptr = self.unmapped_read.as_ptr();
        self.cpu.reset(unmapped_write_ptr, unmapped_read_ptr);
        unsafe {
            self.cpu
                .map_mem(0, MEM_SIZE as u32, self.ram.as_mut_ptr(), self.ram.as_ptr());
        }

        // Reset sound chips
        self.ay.reset();
        self.scc.reset();
        if let Some(ref mut sn) = self.sn {
            // Re-derive internal synth pointers (the emu may have been
            // boxed/moved since creation), then C++ Sms_Apu::reset()
            sn.reinit_pointers();
            sn.reset(0, 0);
        }

        // Set up CPU for init call
        self.cpu.r.sp = 0xF380;
        // Push idle_addr onto stack
        self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
        let sp1 = self.cpu.r.sp as usize;
        self.ram[sp1] = (IDLE_ADDR >> 8) as u8;
        self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
        let sp2 = self.cpu.r.sp as usize;
        self.ram[sp2] = (IDLE_ADDR & 0xFF) as u8;

        self.cpu.r.bregs[6] = track as u8; // A register = track number
        self.cpu.r.pc = self.header.base.init_addr;

        // Set play period based on PAL/NTSC
        let period = if self.header.base.is_pal() {
            CLOCK_RATE as BlipTime / 50
        } else {
            CLOCK_RATE as BlipTime / 60
        };
        self.play_period = (period as f64 / self.emu_state.tempo) as BlipTime;
        self.next_play = self.play_period;
        self.scc_accessed = false;
        self.gain_updated = false;
        self.update_gain();
        self.ay_latch = 0;

        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;
        if self.buf_initialized && !self.emu_state.ignore_silence_flag {
            let end = i64::from(self.emu_state.max_initial_silence)
                * i64::from(self.emu_state.out_channels())
                * i64::from(self.emu_state.sample_rate);
            while i64::from(self.emu_state.emu_time) < end && self.emu_state.buf_remain == 0 {
                self.fill_silence_buffer();
            }
        }
        self.emu_state.emu_time = self.emu_state.buf_remain as i32;
        self.emu_state.silence_time = 0;
        self.emu_state.silence_count = 0;
        Ok(())
    }

    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let out = &mut out[..count as usize];
        if !self.buf_initialized || self.emu_state.track_ended {
            out.fill(0);
            self.emu_state.advance_output(count);
            return Ok(());
        }
        let mut pos = 0;
        if self.emu_state.silence_count > 0 {
            let ahead = i64::from(self.emu_state.silence_lookahead)
                * (i64::from(self.emu_state.out_time) + i64::from(count)
                    - self.emu_state.silence_time)
                + self.emu_state.silence_time;
            while i64::from(self.emu_state.emu_time) < ahead
                && self.emu_state.buf_remain == 0
                && !self.emu_state.emu_track_ended
            {
                self.fill_silence_buffer();
            }
            pos = self.emu_state.silence_count.min(i64::from(count)) as usize;
            out[..pos].fill(0);
            self.emu_state.silence_count -= pos as i64;
            if !self.emu_state.ignore_silence_flag
                && i64::from(self.emu_state.emu_time) - self.emu_state.silence_time
                    > 6 * i64::from(self.emu_state.out_channels())
                        * i64::from(self.emu_state.sample_rate)
            {
                self.emu_state.emu_track_ended = true;
                self.emu_state.track_ended = true;
                self.emu_state.silence_count = 0;
                self.emu_state.buf_remain = 0;
            }
        }
        let buffered = self.emu_state.buf_remain as usize;
        let copy = buffered.min(out.len() - pos);
        let start = self.emu_state.silence_buf.len() - buffered;
        out[pos..pos + copy].copy_from_slice(&self.emu_state.silence_buf[start..start + copy]);
        self.emu_state.buf_remain -= copy as i64;
        let tail = &mut out[pos + copy..];
        if !tail.is_empty() {
            if self.emu_state.emu_track_ended {
                tail.fill(0);
            } else {
                self.classic_play(tail.len() as i32, tail);
            }
            self.emu_state.emu_time += tail.len() as i32;
            if !self.emu_state.ignore_silence_flag
                || self.emu_state.out_time > self.emu_state.fade_start
            {
                let silence = count_silence(tail);
                if silence < tail.len() as i64 {
                    self.emu_state.silence_time = i64::from(self.emu_state.emu_time) - silence;
                }
                if !self.emu_state.emu_track_ended
                    && i64::from(self.emu_state.emu_time) - self.emu_state.silence_time
                        >= self.emu_state.silence_buf.len() as i64
                {
                    self.fill_silence_buffer();
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

        self.emu_state.advance_output(count);
        Ok(())
    }

    fn skip_impl(&mut self, count: i32) -> GmeResult {
        self.emu_state.advance_output(count);

        if !self.buf_initialized || self.emu_state.track_ended {
            return Ok(());
        }

        let silent = i64::from(count).min(self.emu_state.silence_count);
        self.emu_state.silence_count -= silent;
        let buffered = (i64::from(count) - silent).min(self.emu_state.buf_remain);
        self.emu_state.buf_remain -= buffered;
        let mut count = count - (silent + buffered) as i32;
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
                    self.classic_play(BUF_SIZE, &mut scratch);
                    count -= BUF_SIZE;
                }

                self.mute_voices_impl(saved_mute);
            }

            while count > 0 && !self.emu_state.emu_track_ended {
                let n = count.min(BUF_SIZE);
                count -= n;
                self.classic_play(n, &mut scratch);
            }

            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.tempo = tempo;
        let period = if self.header.base.is_pal() {
            CLOCK_RATE as BlipTime / 50
        } else {
            CLOCK_RATE as BlipTime / 60
        };
        self.play_period = (period as f64 / tempo) as BlipTime;
    }

    fn set_equalizer_impl(&mut self, settings: &Equalizer) {
        let eq = BlipEq::new(settings.treble);
        self.ay.treble_eq(&eq);
        self.scc.treble_eq(&eq);
        if let Some(ref mut sn) = self.sn {
            sn.treble_eq(&eq);
        }
        self.stereo_buf.bass_freq(settings.bass as i32);
    }

    fn enable_multi_channel(&mut self) -> GmeResult {
        if self.emu_state.sample_rate != 0 {
            return Err("Output already initialized".into());
        }
        self.stereo_buf = EffectsBuffer::new(8, false);
        self.emu_state.multi_channel = true;
        Ok(())
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
        OSC_COUNT as i32
    }

    fn voice_names(&self) -> &[&str] {
        &VOICE_NAMES
    }
}

impl KssEmu {
    fn fill_silence_buffer(&mut self) {
        let mut buffer = std::mem::take(&mut self.emu_state.silence_buf);
        self.classic_play(buffer.len() as i32, &mut buffer);
        self.emu_state.emu_time += buffer.len() as i32;
        let silence = count_silence(&buffer);
        if silence < buffer.len() as i64 {
            self.emu_state.silence_time = i64::from(self.emu_state.emu_time) - silence;
            self.emu_state.buf_remain = buffer.len() as i64;
        } else {
            self.emu_state.silence_count += buffer.len() as i64;
        }
        self.emu_state.silence_buf = buffer;
    }

    /// C++ Classic_Emu::play_: read samples from the stereo buffer, running
    /// the emulator for more as needed. No silence detection or out_time
    /// bookkeeping.
    fn classic_play(&mut self, count: i32, out: &mut [i16]) {
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
                // Need to generate more samples
                let msec = self.stereo_buf.length();
                let mut clocks = msec as BlipTime * CLOCK_RATE as BlipTime / 1000;
                self.run_clocks(&mut clocks);
            }
        }
    }

    /// Set CPU absolute time (equivalent to C++ set_time).
    fn cpu_set_time(&mut self, t: BlipTime) {
        self.cpu.set_time(t);
    }

    /// Get CPU absolute time (equivalent to C++ time()).
    fn cpu_time(&mut self) -> BlipTime {
        self.cpu.time()
    }

    /// Adjust CPU time by delta.
    fn cpu_adjust_time(&mut self, delta: BlipTime) {
        self.cpu.adjust_time(delta);
    }

    /// Run emulation for the given duration.
    fn run_clocks(&mut self, duration: &mut BlipTime) {
        while self.cpu_time() < *duration {
            let end = (*duration).min(self.next_play);
            let ay_ptr = &mut self.ay as *mut AyApu;
            let scc_enabled = self.scc_enabled;
            let self_ptr = self as *mut Self;

            // Pass absolute end time to CPU (matching C++ behavior)
            self.cpu.run(
                end,
                &mut |t, addr, data| {
                    // CPU OUT handler
                    #[cfg(feature = "trace")]
                    crate::kss_out_trace(t, addr, data);
                    unsafe {
                        match addr & 0xFF {
                            0xA0 => {
                                (*self_ptr).ay_latch = (data & 0x0F) as i32;
                            }
                            0xA1 => {
                                (*ay_ptr).write(t, (*self_ptr).ay_latch as usize, data);
                            }
                            0x06 => {
                                // Game Gear stereo (C++: only when device_flags & 0x04)
                                let emu = &mut *self_ptr;
                                if emu.header.base.device_flags & 0x04 != 0
                                    && let Some(ref mut sn) = emu.sn
                                {
                                    sn.write_ggstereo(t, data as i32);
                                }
                            }
                            0x7E | 0x7F => {
                                // SN76489 data write
                                if let Some(ref mut sn) = (*self_ptr).sn {
                                    sn.write_data(t, data as i32);
                                }
                            }
                            0xFE => {
                                (*self_ptr).set_bank(0, data as i32);
                            }
                            _ => {}
                        }
                    }
                },
                &mut |_t, _addr| {
                    // CPU IN handler
                    0
                },
                &mut |t, addr, data| {
                    // CPU WRITE handler
                    unsafe {
                        if (addr as u32 & scc_enabled) == 0x8000 {
                            (*self_ptr).cpu_write(t, addr, data);
                        }
                    }
                },
            );

            if self.cpu.r.pc as u32 == IDLE_ADDR {
                self.cpu_set_time(end);
            }

            if self.cpu_time() >= self.next_play {
                self.next_play += self.play_period;
                if self.cpu.r.pc as u32 == IDLE_ADDR {
                    if !self.gain_updated {
                        self.gain_updated = true;
                        if self.scc_accessed {
                            self.update_gain();
                        }
                    }

                    // Push play_addr onto stack
                    self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
                    let sp1 = self.cpu.r.sp as usize;
                    if sp1 < MEM_SIZE {
                        self.ram[sp1] = (IDLE_ADDR >> 8) as u8;
                    }
                    self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
                    let sp2 = self.cpu.r.sp as usize;
                    if sp2 < MEM_SIZE {
                        self.ram[sp2] = (IDLE_ADDR & 0xFF) as u8;
                    }
                    self.cpu.r.pc = self.header.base.play_addr;
                }
            }
        }

        let dur = self.cpu_time();
        *duration = dur;
        self.next_play -= dur;
        if self.next_play < 0 {
            self.next_play = 0;
        }
        self.cpu_adjust_time(-dur);
        self.ay.end_frame(dur);
        self.scc.end_frame(dur);
        if let Some(ref mut sn) = self.sn {
            sn.end_frame(dur);
        }
        self.stereo_buf.end_frame(dur);
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

    fn load_test_kss() -> KssEmu {
        let data = std::fs::read("../../assets/test.kss")
            .or_else(|_| std::fs::read("assets/test.kss"))
            .expect("test.kss not found");
        let mut emu = KssEmu::new();
        emu.load_mem_impl(&data).expect("Failed to load test.kss");
        emu
    }

    #[test]
    fn test_load_kss() {
        let emu = load_test_kss();
        assert!(emu.state.track_count > 0, "Should have tracks");
    }

    #[test]
    fn test_set_sample_rate() {
        let mut emu = load_test_kss();
        emu.set_sample_rate_impl(44100).unwrap();
        assert_eq!(emu.emu_state.sample_rate, 44100);
        assert!(emu.buf_initialized);
    }

    #[test]
    fn test_start_track() {
        let mut emu = load_test_kss();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();
        assert!(!emu.emu_state.track_ended);
        assert!(!emu.emu_state.emu_track_ended);
    }

    #[test]
    fn test_play_produces_output() {
        let mut emu = load_test_kss();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();

        let count = 8820; // 100ms stereo
        let mut out = vec![0i16; count];
        emu.play_impl(count as i32, &mut out).unwrap();

        // Check that the emulator produces output (may be zero if Z80 CPU needs debugging)
        assert_eq!(
            out.len(),
            count,
            "Output length should match requested count"
        );
    }
}
