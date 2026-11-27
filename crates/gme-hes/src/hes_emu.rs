// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Hes_Emu.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! TurboGrafx-16/PC Engine HES music file emulator.
//! Port of C++ Hes_Emu.h / Hes_Emu.cpp.

use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator, count_silence};
use gme_common::track_info::Equalizer;
use gme_common::track_info::TrackInfo;
use gme_core::blip_buffer::BlipEq;
use gme_core::effects_buffer::EffectsBuffer;
use gme_core::endian::{get_le16, get_le32};
use gme_core::error::{GmeError, GmeResult};
use gme_core::multi_buffer::{CHANNEL_MIXED_TYPE, CHANNEL_WAVE_TYPE, MultiBuffer};

use crate::adpcm::{HesAdpcm, IO_ADDR as ADPCM_IO_ADDR, IO_SIZE as ADPCM_IO_SIZE};
use crate::apu::{END_ADDR as APU_END, HesApu, OSC_COUNT, START_ADDR as APU_START};
use crate::cpu::{
    FUTURE_HES_TIME, HesCpu, HesTime, IDLE_ADDR, PAGE_COUNT, PAGE_SHIFT, PAGE_SIZE, PageRef, ST_I,
    WritePage,
};
use crate::rom::RomData;

pub(crate) const TIMER_MASK: i32 = 0x04;
pub(crate) const VDP_MASK: i32 = 0x02;
const UNMAPPED: i32 = 0xFF;

const PERIOD_60HZ: i64 = 262 * 455; // scanlines * clocks per scanline
const CLOCK_RATE: i64 = 7_159_091;
const HEADER_SIZE: usize = 0x20;
const ROM_MAX: i64 = 0x100000;

const SGX_SIZE: usize = 3 * PAGE_SIZE + crate::cpu::CPU_PAD;

const SILENCE_BUF_SIZE: i64 = 2048;
const SILENCE_MAX_SECS: i64 = 6;

const VOICE_NAMES: [&str; OSC_COUNT] =
    ["Wave 1", "Wave 2", "Wave 3", "Wave 4", "Multi 1", "Multi 2"];

/// Voice types from the C++ Hes_Emu constructor (set_voice_types).
const VOICE_TYPES: [i32; OSC_COUNT] = [
    CHANNEL_WAVE_TYPE,
    CHANNEL_WAVE_TYPE | 1,
    CHANNEL_WAVE_TYPE | 2,
    CHANNEL_WAVE_TYPE | 3,
    CHANNEL_MIXED_TYPE,
    CHANNEL_MIXED_TYPE | 1,
];

/// HES file header.
#[derive(Clone, Default)]
pub struct HesHeader {
    pub tag: [u8; 4],
    pub vers: u8,
    pub first_track: u8,
    pub init_addr: [u8; 2],
    pub banks: [u8; 8],
    pub data_tag: [u8; 4],
    pub size: [u8; 4],
    pub addr: [u8; 4],
    pub unused: [u8; 4],
}

impl HesHeader {
    pub fn parse(h: &[u8]) -> Option<Self> {
        if h.len() < HEADER_SIZE {
            return None;
        }
        let mut out = Self::default();
        out.tag.copy_from_slice(&h[0..4]);
        out.vers = h[4];
        out.first_track = h[5];
        out.init_addr.copy_from_slice(&h[6..8]);
        out.banks.copy_from_slice(&h[8..16]);
        out.data_tag.copy_from_slice(&h[16..20]);
        out.size.copy_from_slice(&h[20..24]);
        out.addr.copy_from_slice(&h[24..28]);
        out.unused.copy_from_slice(&h[28..32]);
        Some(out)
    }
}

pub(crate) struct TimerState {
    pub last_time: HesTime,
    pub count: i32,
    pub load: i32,
    pub raw_load: i32,
    pub enabled: u8,
    pub fired: u8,
}

pub(crate) struct VdpState {
    pub next_vbl: HesTime,
    pub latch: u8,
    pub control: u8,
}

pub(crate) struct IrqState {
    pub timer: HesTime,
    pub vdp: HesTime,
    pub disables: u8,
}

/// HES emulator.
pub struct HesEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub cpu: HesCpu,
    pub(crate) write_pages: [WritePage; PAGE_COUNT + 1],
    pub rom: RomData,
    pub header: HesHeader,
    pub(crate) play_period: HesTime,
    pub(crate) timer_base: i32,
    pub(crate) timer: TimerState,
    pub(crate) vdp: VdpState,
    pub(crate) irq: IrqState,
    pub apu: HesApu,
    pub adpcm: HesAdpcm,
    pub(crate) sgx: Vec<u8>,
    /// Output buffer. The C++ reference path (gme_new_emu with the HES
    /// type's stereo flag set) routes audio through Effects_Buffer(1) with
    /// effects disabled, NOT Stereo_Buffer, so the port must too.
    pub stereo_buf: EffectsBuffer,
    pub buf_initialized: bool,
}

// SAFETY: HesEmu's raw pointers (APU osc outputs) point into the
// heap-allocated buffer list owned by the struct itself. No shared
// references or thread-unsafe interior mutability.
unsafe impl Send for HesEmu {}

impl Default for HesEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl HesEmu {
    pub fn new() -> Self {
        let mut emu_state = MusicEmuState::new();
        // C++ Hes_Emu ctor: set_silence_lookahead( 6 ); set_gain( 1.11 );
        emu_state.silence_lookahead = 6;
        emu_state.gain = 1.11;

        Self {
            state: GmeFileState::new(),
            emu_state,
            cpu: HesCpu::new(),
            write_pages: [WritePage::None; PAGE_COUNT + 1],
            rom: RomData::new(),
            header: HesHeader::default(),
            play_period: 0,
            timer_base: 0,
            timer: TimerState {
                last_time: 0,
                count: 0,
                load: 0,
                raw_load: 0, // C++ ctor: timer.raw_load = 0
                enabled: 0,
                fired: 0,
            },
            vdp: VdpState {
                next_vbl: 0,
                latch: 0,
                control: 0,
            },
            irq: IrqState {
                timer: 0,
                vdp: 0,
                disables: 0,
            },
            apu: HesApu::new(),
            adpcm: HesAdpcm::new(),
            sgx: vec![0u8; SGX_SIZE],
            stereo_buf: EffectsBuffer::new(1, false),
            buf_initialized: false,
        }
    }

    /// Wire oscillator outputs per the mute mask. Transliteration of C++
    /// Classic_Emu::mute_voices_ + Hes_Emu::set_voice.
    fn wire_voices(&mut self, mask: i32) {
        for i in (0..OSC_COUNT).rev() {
            if mask & (1 << i) != 0 {
                self.apu.osc_output(
                    i,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
            } else {
                let ch = self.stereo_buf.channel(i as i32, VOICE_TYPES[i]);
                self.apu.osc_output(i, ch.center, ch.left, ch.right);
            }
        }
    }

    // ------------------------------------------------------------------
    // CPU time bookkeeping (C++ Hes_Cpu inline methods)
    // ------------------------------------------------------------------

    #[inline]
    pub(crate) fn time(&self) -> HesTime {
        self.cpu.time()
    }

    fn cpu_update_end_time(&mut self, end: HesTime, irq: HesTime) -> i32 {
        let mut t = end;
        if irq < t && (self.cpu.r.status & ST_I) == 0 {
            t = irq;
        }
        let delta = self.cpu.state_base - t;
        self.cpu.state_base = t;
        delta
    }

    pub(crate) fn cpu_set_irq_time(&mut self, t: HesTime) {
        self.cpu.irq_time = t;
        let d = self.cpu_update_end_time(self.cpu.end_time, t);
        self.cpu.state_time += d;
    }

    pub(crate) fn cpu_set_end_time(&mut self, t: HesTime) {
        self.cpu.end_time = t;
        let d = self.cpu_update_end_time(t, self.cpu.irq_time);
        self.cpu.state_time += d;
    }

    // ------------------------------------------------------------------
    // Memory mapping
    // ------------------------------------------------------------------

    /// C++ Hes_Emu::cpu_set_mmr (page data + write page).
    fn page_for_bank(&self, bank: i32) -> (PageRef, WritePage) {
        if bank < 0x80 {
            return (
                PageRef::Rom(self.rom.at_addr(bank * PAGE_SIZE as i32)),
                WritePage::None,
            );
        }
        match bank {
            0xF8 => (PageRef::Ram, WritePage::Ram),
            0xF9..=0xFB => {
                let off = (bank - 0xF9) as usize * PAGE_SIZE;
                (PageRef::Sgx(off), WritePage::Sgx(off))
            }
            _ => (PageRef::Rom(self.rom.unmapped()), WritePage::None),
        }
    }

    /// C++ Hes_Cpu::set_mmr.
    pub(crate) fn set_mmr(&mut self, reg: usize, bank: i32) {
        debug_assert!(reg <= PAGE_COUNT);
        debug_assert!((0..0x100).contains(&bank));
        self.cpu.mmr[reg] = bank as u8;
        let (code, wp) = self.page_for_bank(bank);
        self.cpu.code_map[reg] = code;
        self.write_pages[reg] = wp;
    }

    /// Read a byte through the code map (C++ get_code deref). `idx` may
    /// exceed the page size by up to CPU_PAD bytes (operand/LE16 reads).
    #[inline]
    pub(crate) fn read_code(&self, page: usize, idx: usize) -> u8 {
        match self.cpu.code_map[page] {
            PageRef::Rom(o) => self.rom.data[o + idx],
            PageRef::Ram => self.cpu.ram[idx],
            PageRef::Sgx(o) => self.sgx[o + idx],
        }
    }

    /// C++ Hes_Emu::cpu_read (full version).
    pub(crate) fn cpu_read_full(&mut self, addr: u16) -> i32 {
        let page = (addr >> PAGE_SHIFT) as usize;
        let mut result = self.read_code(page, addr as usize & (PAGE_SIZE - 1)) as i32;
        if self.cpu.mmr[page] == 0xFF {
            result = self.cpu_read_io(addr);
        }
        result
    }

    /// C++ Hes_Emu::cpu_write (full version).
    pub(crate) fn cpu_write_full(&mut self, addr: u16, data: i32) {
        let page = (addr >> PAGE_SHIFT) as usize;
        let masked = addr as usize & (PAGE_SIZE - 1);
        match self.write_pages[page] {
            WritePage::Ram => self.cpu.ram[masked] = data as u8,
            WritePage::Sgx(o) => self.sgx[o + masked] = data as u8,
            WritePage::None => {
                if self.cpu.mmr[page] == 0xFF {
                    self.cpu_write_io(masked as u16, data);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Hardware (I/O page)
    // ------------------------------------------------------------------

    /// C++ Hes_Emu::cpu_write_vdp.
    pub(crate) fn cpu_write_vdp(&mut self, addr: i32, data: i32) {
        match addr {
            0 => {
                self.vdp.latch = (data & 0x1F) as u8;
            }
            2 if self.vdp.latch == 5 => {
                let t = self.time();
                self.run_until(t);
                self.vdp.control = data as u8;
                self.irq_changed();
            }
            _ => {
                // case 2 with other latches, case 3 and others:
                // debug print only in C++
            }
        }
    }

    /// C++ Hes_Emu::cpu_write_ (addr already masked to the I/O page).
    pub(crate) fn cpu_write_io(&mut self, addr: u16, data: i32) {
        if (addr as u32).wrapping_sub(APU_START as u32) <= (APU_END - APU_START) as u32 {
            // avoid going way past end when a long block xfer is writing to I/O space
            let t = self.time().min(self.cpu.end_time + 8);
            self.apu.write_data(t, addr as i32, data);
            return;
        }

        if (addr as u32).wrapping_sub(ADPCM_IO_ADDR) < ADPCM_IO_SIZE {
            let t = self.time().min(self.cpu.end_time + 6);
            self.adpcm.write_data(t, addr as i32, data);
            return;
        }

        let time = self.time();
        match addr {
            0x0000 | 0x0002 | 0x0003 => {
                self.cpu_write_vdp(addr as i32, data);
                return;
            }

            0x0C00 => {
                self.run_until(time);
                self.timer.raw_load = (data & 0x7F) + 1;
                self.recalc_timer_load();
                self.timer.count = self.timer.load;
            }

            0x0C01 => {
                let data = data & 1;
                if self.timer.enabled == data as u8 {
                    return;
                }
                self.run_until(time);
                self.timer.enabled = data as u8;
                if data != 0 {
                    self.timer.count = self.timer.load;
                }
            }

            0x1402 => {
                self.run_until(time);
                self.irq.disables = data as u8;
            }

            0x1403 => {
                self.run_until(time);
                if self.timer.enabled != 0 {
                    self.timer.count = self.timer.load;
                }
                self.timer.fired = 0;
            }

            // Debug build (no NDEBUG): 0x1000 (I/O port), 0x0402-0x0405
            // (palette) and everything else return without irq_changed.
            _ => return,
        }

        self.irq_changed();
    }

    /// C++ Hes_Emu::cpu_read_.
    pub(crate) fn cpu_read_io(&mut self, addr: u16) -> i32 {
        let time = self.time();
        let addr = addr & (PAGE_SIZE as u16 - 1);
        match addr {
            0x0000 => {
                if self.irq.vdp > time {
                    return 0;
                }
                self.irq.vdp = FUTURE_HES_TIME;
                self.run_until(time);
                self.irq_changed();
                0x20
            }

            0x0002 | 0x0003 => 0,

            0x0C01 | 0x0C00 => {
                self.run_until(time);
                (((self.timer.count - 1) as u32) / self.timer_base as u32) as i32
            }

            0x1402 => self.irq.disables as i32,

            0x1403 => {
                let mut status = 0;
                if self.irq.timer <= time {
                    status |= TIMER_MASK;
                }
                if self.irq.vdp <= time {
                    status |= VDP_MASK;
                }
                status
            }

            0x180A..=0x180D => self.adpcm.read_data(time, addr as i32),

            _ => UNMAPPED,
        }
    }

    // ------------------------------------------------------------------
    // Timer / IRQ emulation
    // ------------------------------------------------------------------

    pub(crate) fn recalc_timer_load(&mut self) {
        self.timer.load = self.timer.raw_load * self.timer_base + 1;
    }

    fn set_tempo_internal(&mut self, t: f64) {
        self.play_period = (PERIOD_60HZ as f64 / t) as HesTime;
        self.timer_base = (1024.0 / t) as i32;
        self.recalc_timer_load();
    }

    /// C++ Hes_Emu::run_until.
    pub(crate) fn run_until(&mut self, present: HesTime) {
        while self.vdp.next_vbl < present {
            self.vdp.next_vbl += self.play_period;
        }

        let elapsed = present - self.timer.last_time;
        if elapsed > 0 {
            if self.timer.enabled != 0 {
                self.timer.count -= elapsed;
                if self.timer.count <= 0 {
                    self.timer.count += self.timer.load;
                }
            }
            self.timer.last_time = present;
        }
    }

    /// C++ Hes_Emu::irq_changed.
    pub(crate) fn irq_changed(&mut self) {
        let present = self.time();

        if self.irq.timer > present {
            self.irq.timer = FUTURE_HES_TIME;
            if self.timer.enabled != 0 && self.timer.fired == 0 {
                self.irq.timer = present + self.timer.count;
            }
        }

        if self.irq.vdp > present {
            self.irq.vdp = FUTURE_HES_TIME;
            if self.vdp.control & 0x08 != 0 {
                self.irq.vdp = self.vdp.next_vbl;
            }
        }

        let mut time = FUTURE_HES_TIME;
        if self.irq.disables as i32 & TIMER_MASK == 0 {
            time = self.irq.timer;
        }
        if self.irq.disables as i32 & VDP_MASK == 0 {
            time = time.min(self.irq.vdp);
        }

        self.cpu_set_irq_time(time);
    }

    /// C++ Hes_Emu::cpu_done.
    pub(crate) fn cpu_done(&mut self) -> i32 {
        if self.cpu.r.status & ST_I == 0 {
            let present = self.time();

            if self.irq.timer <= present && self.irq.disables as i32 & TIMER_MASK == 0 {
                self.timer.fired = 1;
                self.irq.timer = FUTURE_HES_TIME;
                self.irq_changed(); // overkill, but matches C++
                return 0x0A;
            }

            if self.irq.vdp <= present && self.irq.disables as i32 & VDP_MASK == 0 {
                return 0x08;
            }
        }
        0
    }

    // ------------------------------------------------------------------
    // Frame emulation
    // ------------------------------------------------------------------

    /// C++ Hes_Emu::run_clocks (plus Classic_Emu's buf end_frame).
    fn run_clocks(&mut self, duration: HesTime) {
        if self.cpu_run(duration) {
            self.state
                .set_warning("Emulation error (illegal instruction)");
        }

        self.run_until(duration);

        // end time frame
        self.timer.last_time -= duration;
        self.vdp.next_vbl -= duration;
        self.cpu.end_frame(duration);
        adjust_time(&mut self.irq.timer, duration);
        adjust_time(&mut self.irq.vdp, duration);
        self.apu.end_frame(duration);
        self.adpcm.end_frame(duration);

        // C++ Classic_Emu::play_ calls buf->end_frame after run_clocks
        self.stereo_buf.end_frame(duration);
    }

    /// C++ Classic_Emu::play_.
    fn raw_play(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let count = count as usize;
        let mut remain = count;
        while remain > 0 {
            let read = self
                .stereo_buf
                .read_samples(&mut out[count - remain..], remain as i64);
            remain -= read as usize;
            if remain > 0 {
                let msec = self.stereo_buf.length();
                let clocks = (msec as i64 * CLOCK_RATE / 1000) as HesTime;
                self.run_clocks(clocks);
            }
        }
        Ok(())
    }

    /// C++ Music_Emu::fill_buf.
    fn fill_buf(&mut self) -> GmeResult {
        debug_assert!(self.emu_state.buf_remain == 0);
        if !self.emu_state.emu_track_ended {
            self.emu_state.emu_time += SILENCE_BUF_SIZE as i32;
            let mut buf = std::mem::take(&mut self.emu_state.silence_buf);
            let result = self.raw_play(SILENCE_BUF_SIZE as i32, &mut buf);
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

    /// Configure buffer + sound chips (C++ load_: apu.volume(gain) and
    /// setup_buffer(7159091) -> clock rate + equalizer). Deterministic, so
    /// safe to call from both set_sample_rate_impl and load_mem_impl.
    fn config_sound(&mut self) {
        self.stereo_buf.clock_rate(CLOCK_RATE as u32);
        self.apu.volume(self.emu_state.gain);
        self.set_equalizer_impl(&Equalizer::default());
    }
}

fn adjust_time(time: &mut HesTime, delta: HesTime) {
    if *time < FUTURE_HES_TIME {
        *time -= delta;
        if *time < 0 {
            *time = 0;
        }
    }
}

// ----------------------------------------------------------------------
// Track info helpers (C++ copy_field / copy_hes_fields)
// ----------------------------------------------------------------------

fn copy_field(data: &[u8]) -> Option<(String, usize)> {
    if data.len() < 0x20 {
        return None;
    }
    let mut len = 0x20usize;
    if data[0x1F] != 0 && data.len() >= 0x30 && data[0x2F] == 0 {
        len = 0x30; // fields are sometimes 16 bytes longer
    }
    if data.len() < len {
        return None;
    }

    let mut i = 0;
    while i < len && data[i] != 0 {
        if (data[i].wrapping_add(1)) < b' ' + 1 {
            return None; // non-ASCII found (also treats 0xFF as non-text)
        }
        i += 1;
    }
    let text_end = i;
    while i < len {
        if data[i] != 0 {
            return None; // data after terminator
        }
        i += 1;
    }

    let s = String::from_utf8_lossy(&data[..text_end])
        .trim_end()
        .to_string();
    Some((s, len))
}

fn copy_hes_fields(data: &[u8], info: &mut TrackInfo) {
    if data.is_empty() || data[0] < b' ' {
        return;
    }
    let mut pos = 0usize;
    let fields: [&mut String; 3] = [&mut info.game, &mut info.author, &mut info.copyright];
    for field in fields {
        match copy_field(&data[pos..]) {
            Some((s, len)) => {
                *field = s;
                pos += len;
            }
            None => return,
        }
    }
}

// ----------------------------------------------------------------------
// GmeFile / MusicEmulator impls
// ----------------------------------------------------------------------

impl GmeFile for HesEmu {
    fn format(&self) -> Format {
        Format::Hes
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.set_track_count(n);
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        let header_bytes = self
            .rom
            .load(data, HEADER_SIZE, UNMAPPED as u8)
            .map_err(|_| GmeError::WrongFileType)?;
        let header = HesHeader::parse(&header_bytes).ok_or(GmeError::WrongFileType)?;

        if &header.tag != b"HESM" {
            return Err(GmeError::WrongFileType);
        }
        if header.vers != 0 {
            self.state.set_warning("Unknown file version");
        }
        if &header.data_tag != b"DATA" {
            self.state.set_warning("Data header missing");
        }
        if header.unused != [0; 4] {
            self.state.set_warning("Unknown header data");
        }

        let mut addr = get_le32(&header.addr) as i64;
        let size = get_le32(&header.size) as i64;
        if addr & !(ROM_MAX - 1) != 0 {
            self.state.set_warning("Invalid address");
            addr &= ROM_MAX - 1;
        }
        if (addr + size) as u64 > ROM_MAX as u64 {
            self.state.set_warning("Invalid size");
        }
        if size != self.rom.file_size {
            if size <= self.rom.file_size - 4
                && &self.rom.data[self.rom.begin_index() + size as usize
                    ..self.rom.begin_index() + size as usize + 4]
                    == b"DATA"
            {
                self.state.set_warning("Multiple DATA not supported");
            } else if size < self.rom.file_size {
                self.state.set_warning("Extra file data");
            } else {
                self.state.set_warning("Missing file data");
            }
        }

        self.header = header;
        self.rom.set_addr(addr as i32);

        self.emu_state.voice_count = OSC_COUNT as i32;
        // bass_freq needs a sample rate; if the file is loaded first (the
        // public API loads before set_sample_rate_impl), set_sample_rate_impl
        // will run config_sound afterwards.
        if self.buf_initialized {
            self.config_sound();
        }

        self.state.set_track_count(256);

        // C++ post_load_: set_tempo(tempo_) then remute_voices()
        self.set_tempo_internal(self.emu_state.tempo);
        let mask = self.emu_state.mute_mask;
        self.wire_voices(mask);

        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, _track: i32) -> GmeResult {
        info.system = "PC Engine".to_string();
        info.track_count = 256;
        let start = self.rom.begin_index() + 0x20;
        if start < self.rom.data.len() {
            copy_hes_fields(&self.rom.data[start..], info);
        }
        Ok(())
    }
}

impl MusicEmulator for HesEmu {
    fn format(&self) -> Format {
        Format::Hes
    }

    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        self.emu_state.sample_rate = sample_rate;
        self.stereo_buf.set_sample_rate(sample_rate, 1000 / 20)?;
        self.config_sound();
        self.buf_initialized = true;
        Ok(())
    }

    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        // C++ Music_Emu::start_track
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;

        // C++ Classic_Emu::start_track_: clear buffer
        self.stereo_buf.clear();

        // C++ Hes_Emu::start_track_
        self.cpu.ram.fill(0); // some HES music relies on zero fill
        self.sgx.fill(0);

        self.apu.reset();
        self.adpcm.reset();
        self.cpu.reset();

        // Channel wiring with the stored mute mask (persistent in C++ from
        // remute_voices at load; re-derive the buffer pointers here so they
        // are valid even after the emulator struct has been moved/boxed).
        let mask = self.emu_state.mute_mask;
        self.wire_voices(mask);

        for i in 0..self.header.banks.len() {
            let bank = self.header.banks[i] as i32;
            self.set_mmr(i, bank);
        }
        self.set_mmr(PAGE_COUNT, 0xFF); // unmapped beyond end of address space

        self.irq.disables = (TIMER_MASK | VDP_MASK) as u8;
        self.irq.timer = FUTURE_HES_TIME;
        self.irq.vdp = FUTURE_HES_TIME;

        self.timer.enabled = 0;
        self.timer.raw_load = 0x80;
        self.timer.count = self.timer.load;
        self.timer.fired = 0;
        self.timer.last_time = 0;

        self.vdp.latch = 0;
        self.vdp.control = 0;
        self.vdp.next_vbl = 0;

        self.cpu.ram[0x1FF] = ((IDLE_ADDR - 1) >> 8) as u8;
        self.cpu.ram[0x1FE] = ((IDLE_ADDR - 1) & 0xFF) as u8;
        self.cpu.r.sp = 0xFD;
        self.cpu.r.pc = get_le16(&self.header.init_addr);
        self.cpu.r.a = track as u8;

        self.recalc_timer_load();

        // C++ Music_Emu::start_track bookkeeping
        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;

        // Pre-fill: run emulator until non-silent audio found
        if !self.emu_state.ignore_silence_flag {
            let end =
                self.emu_state.max_initial_silence as i64 * 2 * self.emu_state.sample_rate as i64;
            while (self.emu_state.emu_time as i64) < end {
                self.fill_buf()?;
                if self.emu_state.buf_remain > 0 || self.emu_state.emu_track_ended {
                    break;
                }
            }
            // C++: emu_time = buf_remain — initial silence discarded entirely
            self.emu_state.emu_time = self.emu_state.buf_remain as i32;
            self.emu_state.out_time = 0;
            self.emu_state.out_time_scaled = 0;
            self.emu_state.silence_time = 0;
            self.emu_state.silence_count = 0;
        }
        Ok(())
    }

    /// C++ Music_Emu::play (structure proven bit-exact by gme-spc/gme-nsf).
    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let out_count = count as usize;

        if !self.buf_initialized || self.emu_state.track_ended {
            out[..out_count].fill(0);
            self.emu_state.out_time += count;
            self.emu_state.out_time_scaled += (count as f64 * self.emu_state.tempo / 2.0) as i32;
            return Ok(());
        }

        let mut pos = 0usize;

        if self.emu_state.silence_count > 0 {
            // during a run of silence, run emulator ahead
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

        if self.emu_state.buf_remain > 0 && pos < out_count {
            let n = (self.emu_state.buf_remain as usize).min(out_count - pos);
            let buf_start = (SILENCE_BUF_SIZE - self.emu_state.buf_remain) as usize;
            let end = buf_start + n;
            out[pos..pos + n].copy_from_slice(&self.emu_state.silence_buf[buf_start..end]);
            self.emu_state.buf_remain -= n as i64;
            pos += n;
        }

        if pos < out_count {
            let remain = (out_count - pos) as i32;
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
                let silence = count_silence(&mut out[pos..pos + remain as usize]);
                if silence < remain as i64 {
                    self.emu_state.silence_time = self.emu_state.emu_time as i64 - silence;
                }
                if self.emu_state.emu_time as i64 - self.emu_state.silence_time >= SILENCE_BUF_SIZE
                {
                    self.fill_buf()?; // cause silence detection on next play()
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
            let mut scratch = vec![0i16; SILENCE_BUF_SIZE as usize];
            if count > threshold {
                let saved_mute = self.emu_state.mute_mask;
                self.mute_voices_impl(!0);

                while count > threshold / 2 && !self.emu_state.emu_track_ended {
                    self.raw_play(SILENCE_BUF_SIZE as i32, &mut scratch)?;
                    count -= SILENCE_BUF_SIZE as i32;
                }

                self.mute_voices_impl(saved_mute);
            }

            while count > 0 && !self.emu_state.emu_track_ended {
                let n = (count as i64).min(SILENCE_BUF_SIZE) as i32;
                count -= n;
                self.raw_play(n, &mut scratch)?;
            }
        }

        if self.emu_state.silence_count == 0 && self.emu_state.buf_remain == 0 {
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    fn voice_count(&self) -> i32 {
        OSC_COUNT as i32
    }

    fn voice_names(&self) -> &[&str] {
        &VOICE_NAMES
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.tempo = tempo;
        self.set_tempo_internal(tempo);
    }

    fn set_equalizer_impl(&mut self, settings: &Equalizer) {
        let eq = BlipEq::new(settings.treble);
        self.apu.treble_eq(&eq);
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
}
