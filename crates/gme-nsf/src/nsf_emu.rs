// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Nsf_Emu.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator};
use gme_common::track_info::Equalizer;
use gme_common::track_info::TrackInfo;
use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipTime};
use gme_core::effects_buffer::EffectsBuffer;
use gme_core::error::{GmeError, GmeResult};
use gme_core::multi_buffer::{
    CHANNEL_MIXED_TYPE, CHANNEL_NOISE_TYPE, CHANNEL_WAVE_TYPE, MultiBuffer,
};

use crate::apu::NesApu;
use crate::cpu::{NesCpu, Registers};
use crate::header::NsfHeader;
use crate::nsfe::NsfeInfo;
use crate::oscs::dmc::DmcReader;
use crate::oscs::fds::NesFdsApu;
use crate::oscs::fme7::NesFme7Apu;
use crate::oscs::mmc5::NesMmc5Apu;
use crate::oscs::namco::NesNamcoApu;
use crate::oscs::vrc6::NesVrc6Apu;
use crate::oscs::vrc7::NesVrc7Apu;
use crate::rom::RomData;

const BANK_SIZE: usize = 0x1000;
const BANK_COUNT: usize = 8;
const BADOP_ADDR: u16 = 0x5FF8;

/// C++ stores play_period pre-multiplied by clock_divisor and divides at runtime.
/// This matches the C++ rounding behavior exactly.
const CLOCK_DIVISOR: i32 = 12;

/// Standard NTSC/PAL playback rates (microseconds per play call).
/// When speed matches these and tempo == 1.0, C++ uses exact NES PPU timing.
const STANDARD_NTSC_SPEED: u16 = 0x411A; // 16666 µs
const STANDARD_PAL_SPEED: u16 = 0x4E20; // 20000 µs

// Expansion chip flag bits
const VRC6_FLAG: u8 = 0x01;
const VRC7_FLAG: u8 = 0x02;
const FDS_FLAG: u8 = 0x04;
const MMC5_FLAG: u8 = 0x08;
const NAMCO_FLAG: u8 = 0x10;
const FME7_FLAG: u8 = 0x20;

/// NSF emulator.
pub struct NsfEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub cpu: NesCpu,
    pub apu: NesApu,
    pub rom: RomData,
    pub header: NsfHeader,
    pub banks: [u8; BANK_COUNT],
    pub initial_banks: [u8; BANK_COUNT],
    pub sram: [u8; 0x2000],
    pub play_period: i32,
    pub next_play: i32,
    pub play_ready: i32,
    pub play_extra: i32,
    pub saved_state: Registers,
    pub track_count: i32,
    pub load_addr: u16,
    pub init_addr: u16,
    pub play_addr: u16,
    /// Output buffer. C++ gme_new_emu wraps NSF (type flags_ & 1) in
    /// Effects_Buffer(1) with effects disabled — not Stereo_Buffer.
    pub stereo_buf: EffectsBuffer,
    /// Whether stereo_buf has been initialized.
    pub buf_initialized: bool,
    /// Stored mute mask, applied during start_track_impl before pre-fill.
    pub mute_mask: i32,
    // Expansion chips
    pub vrc6: Option<NesVrc6Apu>,
    pub fds: Option<NesFdsApu>,
    pub fme7: Option<NesFme7Apu>,
    pub namco: Option<NesNamcoApu>,
    pub mmc5: Option<NesMmc5Apu>,
    pub vrc7: Option<NesVrc7Apu>,
    /// MMC5 multiplier registers ($5205/$5206)
    pub mmc5_mul: [u8; 2],
    // NSFE metadata (None for plain NSF files)
    pub nsfe_info: Option<NsfeInfo>,
}

use gme_common::music_emu::count_silence;

const SILENCE_BUF_SIZE: i64 = 2048;
const SILENCE_MAX_SECS: i64 = 6;

impl Default for NsfEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl NsfEmu {
    pub fn new() -> Self {
        Self {
            state: GmeFileState::new(),
            emu_state: MusicEmuState::new(),
            cpu: NesCpu::new(),
            apu: NesApu::new(),
            rom: RomData::new(BANK_SIZE),
            header: NsfHeader::default(),
            banks: [0; BANK_COUNT],
            initial_banks: [0; BANK_COUNT],
            sram: [0; 0x2000],
            play_period: 0,
            next_play: 0,
            play_ready: 0,
            play_extra: 0,
            saved_state: Registers::default(),
            track_count: 0,
            load_addr: 0x8000,
            init_addr: 0x8000,
            play_addr: 0x8000,
            stereo_buf: EffectsBuffer::new(1, false),
            buf_initialized: false,
            mute_mask: 0,
            vrc6: None,
            fds: None,
            fme7: None,
            namco: None,
            mmc5: None,
            vrc7: None,
            mmc5_mul: [0; 2],
            nsfe_info: None,
        }
    }

    /// Initialize the audio buffer and wire oscillators to it.
    fn init_audio(&mut self, sample_rate: i32) -> GmeResult {
        self.stereo_buf.set_sample_rate(sample_rate, 1000 / 20)?;
        let clock_rate = if self.header.is_pal() {
            1662607u32
        } else {
            1789773u32
        };
        self.stereo_buf.clock_rate(clock_rate);
        // Match C++ Nsf_Emu::nes_eq = make_equalizer(-1.0, 80)

        // Create expansion chips based on chip_flags
        let flags = self.header.chip_flags();
        if flags & VRC6_FLAG != 0 && self.vrc6.is_none() {
            self.vrc6 = Some(NesVrc6Apu::new());
        }
        if flags & FDS_FLAG != 0 && self.fds.is_none() {
            self.fds = Some(NesFdsApu::new());
        }
        if flags & FME7_FLAG != 0 && self.fme7.is_none() {
            self.fme7 = Some(NesFme7Apu::new());
        }
        if flags & NAMCO_FLAG != 0 && self.namco.is_none() {
            self.namco = Some(NesNamcoApu::new());
        }
        if flags & MMC5_FLAG != 0 && self.mmc5.is_none() {
            self.mmc5 = Some(NesMmc5Apu::new());
        }
        if flags & VRC7_FLAG != 0 && self.vrc7.is_none() {
            self.vrc7 = Some(NesVrc7Apu::new());
        }
        if flags & !(VRC6_FLAG | FDS_FLAG | FME7_FLAG | NAMCO_FLAG | MMC5_FLAG | VRC7_FLAG) != 0 {
            self.state
                .set_warning("Uses unsupported audio expansion hardware");
        }

        // Wire all oscillators to their Effects_Buffer channels
        // (C++ remute_voices via Classic_Emu::mute_voices_ + Nsf_Emu::set_voice).
        let mask = self.mute_mask;
        self.wire_voices(mask);

        // Apply volume gain matching C++ Nsf_Emu::init_sound().
        // C++ sets base gain 1.4, then multiplies by 0.75 per expansion chip.
        let mut adjusted_gain = 1.4f64;
        if self.vrc6.is_some() {
            adjusted_gain *= 0.75;
        }
        if self.fds.is_some() {
            adjusted_gain *= 0.75;
        }
        if self.fme7.is_some() {
            adjusted_gain *= 0.75;
        }
        if self.namco.is_some() {
            adjusted_gain *= 0.75;
        }
        if self.mmc5.is_some() {
            adjusted_gain *= 0.75;
        }
        if self.vrc7.is_some() {
            adjusted_gain *= 0.75;
        }
        // Apply gain to all present chips (matching C++ lines 315-327)
        if let Some(ref mut vrc6) = self.vrc6 {
            vrc6.volume(adjusted_gain);
        }
        if let Some(ref mut fds) = self.fds {
            fds.volume(adjusted_gain);
        }
        if let Some(ref mut fme7) = self.fme7 {
            fme7.volume(adjusted_gain);
        }
        if let Some(ref mut namco) = self.namco {
            namco.volume(adjusted_gain);
        }
        if let Some(ref mut mmc5) = self.mmc5 {
            mmc5.volume(adjusted_gain);
        }
        if let Some(ref mut vrc7) = self.vrc7 {
            vrc7.volume(adjusted_gain);
        }
        self.apu.volume(adjusted_gain);

        // Match C++ Nsf_Emu::nes_eq = make_equalizer(-1.0, 80)
        self.set_equalizer_impl(&Equalizer::new(-1.0, 80.0));

        self.buf_initialized = true;
        Ok(())
    }

    /// If the sample rate was set before loading (the public GameMusicEmu API
    /// does this), audio was initialized before the header was known — no
    /// expansion chips and possibly the wrong clock rate. Redo it now that
    /// chip_flags/is_pal are available.
    fn reinit_audio_after_load(&mut self) -> GmeResult {
        if self.buf_initialized {
            self.init_audio(self.emu_state.sample_rate)?;
        }
        Ok(())
    }

    /// Point the DMC's DPCM sample reader at this emulator's current memory.
    /// Must be called at the start of every emulation frame: the pointers
    /// target arrays inside `self`, so they go stale whenever the emulator
    /// struct moves (e.g. boxed by the public API) or `rom_data` reallocates.
    fn refresh_dmc_reader(&mut self) {
        self.apu.dmc.reader = Some(DmcReader {
            rom_data: self.cpu.rom_data.as_ptr(),
            rom_len: self.cpu.rom_data.len(),
            code_map: &self.cpu.code_map,
            low_mem: &self.cpu.low_mem,
            sram: &self.sram,
        });
    }

    /// C++ Nsf_Emu::load_ address validation: zero addresses default to
    /// $8000; load/init below $8000 reject the file ("Corrupt file").
    fn validate_addrs(&mut self) -> GmeResult {
        const ROM_BEGIN: u16 = 0x8000;
        if self.load_addr == 0 {
            self.load_addr = ROM_BEGIN;
        }
        if self.init_addr == 0 {
            self.init_addr = ROM_BEGIN;
        }
        if self.play_addr == 0 {
            self.play_addr = ROM_BEGIN;
        }
        if self.load_addr < ROM_BEGIN || self.init_addr < ROM_BEGIN {
            // C++: "Corrupt file (invalid load/init/play address)"
            return Err(GmeError::CorruptFile);
        }
        Ok(())
    }

    /// C++ Nsf_Emu::load_ bank setup: positional defaults (slots before the
    /// load bank or past the ROM map to bank 0), but if ANY header bank slot
    /// is nonzero the ENTIRE header array is used verbatim — zero slots
    /// included.
    fn setup_bank_mapping(&mut self) {
        let total_banks = (self.rom.size as usize) / BANK_SIZE;
        let first_bank = (self.load_addr as i64 - 0x8000) / BANK_SIZE as i64;
        for i in 0..BANK_COUNT {
            // unsigned wrap like C++ `unsigned bank = i - first_bank`
            let mut bank = (i as i64 - first_bank) as u64 as usize;
            if bank >= total_banks {
                bank = 0;
            }
            self.initial_banks[i] = bank as u8;

            if self.header.banks[i] != 0 {
                // bank-switched
                self.initial_banks = self.header.banks;
                break;
            }
        }
    }

    /// Run one step of CPU with I/O via unsafe pointers.
    /// end_time must be set on self.cpu before calling.
    fn cpu_step(&mut self) -> bool {
        let cpu_ptr = &mut self.cpu as *mut NesCpu;
        let apu_ptr = &mut self.apu as *mut NesApu;
        let rom_ptr = &self.rom as *const RomData;
        let sram_ptr = &mut self.sram as *mut [u8; 0x2000];
        let banks_ptr = &mut self.banks as *mut [u8; BANK_COUNT];

        let low_mem = &self.cpu.low_mem as *const [u8; 0x800];
        let code_map = &self.cpu.code_map as *const [usize; 33];
        let rom_data_ptr = self.cpu.rom_data.as_ptr();
        let rom_data_len = self.cpu.rom_data.len();

        // Expansion chip pointers (null if not present)
        let vrc6_ptr = self
            .vrc6
            .as_mut()
            .map(|c| c as *mut NesVrc6Apu)
            .unwrap_or(std::ptr::null_mut());
        let fds_ptr = self
            .fds
            .as_mut()
            .map(|c| c as *mut NesFdsApu)
            .unwrap_or(std::ptr::null_mut());
        let fme7_ptr = self
            .fme7
            .as_mut()
            .map(|c| c as *mut NesFme7Apu)
            .unwrap_or(std::ptr::null_mut());
        let namco_ptr = self
            .namco
            .as_mut()
            .map(|c| c as *mut NesNamcoApu)
            .unwrap_or(std::ptr::null_mut());
        let mmc5_ptr = self
            .mmc5
            .as_mut()
            .map(|c| c as *mut NesMmc5Apu)
            .unwrap_or(std::ptr::null_mut());
        let vrc7_ptr = self
            .vrc7
            .as_mut()
            .map(|c| c as *mut NesVrc7Apu)
            .unwrap_or(std::ptr::null_mut());
        let mmc5_mul_ptr = &mut self.mmc5_mul as *mut [u8; 2];

        let badop_addr = self.cpu.badop_addr;

        unsafe {
            (&mut *cpu_ptr).run(
                &|addr, _time| {
                    if addr == badop_addr {
                        return NesCpu::BAD_OPCODE;
                    }
                    match addr {
                        0x0000..=0x1FFF => (*low_mem)[addr as usize & 0x7FF],
                        0x8000..=0xFFFF => {
                            let page = addr as usize >> 11;
                            let map_entry = (*code_map)[page];
                            if map_entry == usize::MAX {
                                NesCpu::BAD_OPCODE
                            } else {
                                let idx = map_entry.wrapping_add(addr as usize);
                                if idx < rom_data_len {
                                    *rom_data_ptr.add(idx)
                                } else {
                                    (addr >> 8) as u8
                                }
                            }
                        }
                        0x6000..=0x7FFF => (*sram_ptr)[(addr - 0x6000) as usize],
                        0x4015 => (&mut *apu_ptr).read_status(_time),
                        // FDS readable registers
                        0x4040..=0x4092 if !fds_ptr.is_null() => (&mut *fds_ptr).read(_time, addr),
                        // Namco data port
                        0x4800 if !namco_ptr.is_null() => (&mut *namco_ptr).read_data(),
                        // MMC5 exram
                        0x5C00..=0x5FFF if !mmc5_ptr.is_null() => {
                            (*mmc5_ptr).exram[(addr - 0x5C00) as usize]
                        }
                        // MMC5 multiplier product (lo/hi)
                        0x5205 | 0x5206 if !mmc5_ptr.is_null() => {
                            let product = (*mmc5_mul_ptr)[0] as u32 * (*mmc5_mul_ptr)[1] as u32;
                            ((product >> ((addr - 0x5205) * 8)) & 0xFF) as u8
                        }
                        _ => (addr >> 8) as u8,
                    }
                },
                &|addr, data, time| match addr {
                    0x6000..=0x7FFF => (*sram_ptr)[(addr - 0x6000) as usize] = data,
                    0x0000..=0x1FFF => (&mut *cpu_ptr).low_mem[addr as usize & 0x7FF] = data,
                    0x4000..=0x4017 => {
                        (&mut *apu_ptr).write_register(time, addr, data);
                    }
                    0x5FF8..=0x5FFF => {
                        let bank = (addr - 0x5FF8) as usize;
                        (*banks_ptr)[bank] = data;
                        let offset = (&*rom_ptr).mask_addr(data as i32 * BANK_SIZE as i32);
                        let rom_offset = (&*rom_ptr).at_addr(offset);
                        (&mut *cpu_ptr).map_code(
                            (bank as u16 + 8) * BANK_SIZE as u16,
                            BANK_SIZE as u16,
                            rom_offset,
                        );
                    }
                    // VRC6: $9000-$9002, $A000-$A002, $B000-$B002
                    0x9000..=0x9002 | 0xA000..=0xA002 | 0xB000..=0xB002 if !vrc6_ptr.is_null() => {
                        let osc = (addr - 0x9000) / 0x1000;
                        let reg = (addr - 0x9000) % 0x1000;
                        (&mut *vrc6_ptr).write_osc(time, osc as usize, reg as usize, data);
                    }
                    // FDS: $4040-$4092
                    0x4040..=0x4092 if !fds_ptr.is_null() => {
                        (&mut *fds_ptr).write(time, addr, data);
                    }
                    // Namco: $4800 data, $F800 address (checked before FME-7,
                    // matching C++ cpu_write_misc dispatch order)
                    0x4800 if !namco_ptr.is_null() => {
                        (&mut *namco_ptr).write_data(time, data);
                    }
                    0xF800 if !namco_ptr.is_null() => {
                        (&mut *namco_ptr).write_addr(data);
                    }
                    // MMC5: $5000-$5015 registers, $5205/$5206 multiplier,
                    // $5C00+ exram (bank selects at $5FF8 take precedence above)
                    0x5000..=0x5015 if !mmc5_ptr.is_null() => {
                        (&mut *mmc5_ptr).write_register(time, addr, data);
                    }
                    0x5205 | 0x5206 if !mmc5_ptr.is_null() => {
                        (*mmc5_mul_ptr)[(addr - 0x5205) as usize] = data;
                    }
                    0x5C00..=0x5FFF if !mmc5_ptr.is_null() => {
                        (*mmc5_ptr).exram[(addr - 0x5C00) as usize] = data;
                    }
                    // VRC7: $9010 = register latch, $9028-$9030 = data
                    // (C++ cpu_write_misc: addr == 0x9010, (addr - 0x9028) <= 0x08)
                    0x9010 if !vrc7_ptr.is_null() => {
                        (&mut *vrc7_ptr).write_reg(data);
                    }
                    0x9028..=0x9030 if !vrc7_ptr.is_null() => {
                        (&mut *vrc7_ptr).write_data(time, data);
                    }
                    // FME-7: $C000-$DFFF = latch, $E000-$FFFF = data
                    0xC000..=0xDFFF if !fme7_ptr.is_null() => {
                        (&mut *fme7_ptr).write_latch(data);
                    }
                    0xE000..=0xFFFF if !fme7_ptr.is_null() => {
                        (&mut *fme7_ptr).write_data(time, data);
                    }
                    _ => {}
                },
            )
        }
    }

    #[allow(dead_code)]
    fn reset_cpu_for_track(&mut self, track: i32) {
        self.cpu.low_mem.fill(0);
        self.sram.fill(0);
        self.cpu.reset();

        self.cpu.badop_addr = BADOP_ADDR;
        self.saved_state.pc = BADOP_ADDR;
        // Start CPU first — this marks all pages as unmapped
        self.cpu
            .start(self.init_addr, track as u8, self.header.is_pal());

        // Then map low memory and ROM banks (overwriting unmapped pages)
        for i in (0..0x8000).step_by(0x800) {
            self.cpu.map_code(i as u16, 0x800, 0);
        }

        for i in 0..BANK_COUNT {
            self.banks[i] = self.initial_banks[i];
            let offset = self
                .rom
                .mask_addr(self.initial_banks[i] as i32 * BANK_SIZE as i32);
            let rom_offset = self.rom.at_addr(offset);
            let bank_addr = (i as u16 + 8) * BANK_SIZE as u16;
            self.cpu.map_code(bank_addr, BANK_SIZE as u16, rom_offset);
        }

        self.apu.reset();

        // Reset expansion chips
        if let Some(ref mut vrc6) = self.vrc6 {
            vrc6.reset();
        }
        if let Some(ref mut fds) = self.fds {
            fds.reset();
        }
        if let Some(ref mut fme7) = self.fme7 {
            fme7.reset();
        }
        if let Some(ref mut namco) = self.namco {
            namco.reset();
        }
        if let Some(ref mut mmc5) = self.mmc5 {
            // C++ start_track_ clears the multiplier and exram before reset
            self.mmc5_mul = [0; 2];
            mmc5.exram = [0; crate::oscs::mmc5::EXRAM_SIZE];
            mmc5.reset();
        }
        if let Some(ref mut vrc7) = self.vrc7 {
            vrc7.reset();
        }

        // Re-wire oscillator outputs after reset
        let mask = self.mute_mask;
        self.wire_voices(mask);

        self.set_tempo_impl(self.emu_state.tempo);
        self.next_play = 0;
        self.play_ready = 0;
        self.play_extra = 0;
    }

    fn run_clocks(&mut self, duration: &mut BlipTime) -> GmeResult {
        // Re-derive DMC sample-fetch pointers from the current location of
        // self (stable for the duration of this &mut self call).
        self.refresh_dmc_reader();

        // Reset CPU time to 0 each frame (matching C++ set_time(0))
        self.cpu.set_time(0);

        while self.cpu.time() < *duration {
            // End time: whichever comes first — duration or next_play
            let mut end = self.next_play.min(*duration);
            // Keep CPU deltas within reasonable range
            end = end.min(self.cpu.time() + 32767);

            self.cpu.end_time = end;
            let hit_badop = self.cpu_step();
            let time = self.cpu.time();

            if hit_badop && self.cpu.r.pc == self.cpu.badop_addr {
                // Play routine returned (hit bad opcode)
                self.play_ready = 1;
                if self.saved_state.pc != self.cpu.badop_addr {
                    self.cpu.r = self.saved_state;
                    self.saved_state.pc = self.cpu.badop_addr;
                } else {
                    self.cpu.set_time(end);
                }
            }

            // Check if it's time to call the play routine
            if time >= self.next_play {
                let period = (self.play_period + self.play_extra) / CLOCK_DIVISOR;
                self.play_extra = self.play_period - period * CLOCK_DIVISOR;
                self.next_play += period;

                if self.play_ready > 0 {
                    self.play_ready -= 1;
                    if self.play_ready == 0 {
                        // Save state and JSR to play_addr
                        self.saved_state = self.cpu.r;
                        self.cpu
                            .push(((self.cpu.badop_addr.wrapping_sub(1)) >> 8) as u8);
                        self.cpu
                            .push(((self.cpu.badop_addr.wrapping_sub(1)) & 0xFF) as u8);
                        self.cpu.r.pc = self.play_addr;
                    }
                }
            }
        }

        // Actual duration emulated
        let actual = self.cpu.time();
        *duration = actual;

        // Normalize next_play relative to new frame
        self.next_play -= actual;
        if self.next_play < 0 {
            self.next_play = 0;
        }

        // End frame for APU (runs oscillators, shifts internal times to 0)
        self.apu.end_frame(actual);

        // End frame for expansion chips
        if let Some(ref mut vrc6) = self.vrc6 {
            vrc6.end_frame(actual);
        }
        if let Some(ref mut fds) = self.fds {
            fds.end_frame(actual);
        }
        if let Some(ref mut fme7) = self.fme7 {
            fme7.end_frame(actual);
        }
        if let Some(ref mut namco) = self.namco {
            namco.end_frame(actual);
        }
        if let Some(ref mut mmc5) = self.mmc5 {
            mmc5.end_frame(actual);
        }
        if let Some(ref mut vrc7) = self.vrc7 {
            vrc7.end_frame(actual);
        }

        // End frame for Blip_Buffers (advances sample window)
        self.stereo_buf.end_frame(actual);

        Ok(())
    }

    /// The buffer each voice renders into, per C++ Classic_Emu::mute_voices_:
    /// channel(i, voice_type).center, or None when muted. Voice types from
    /// the C++ Nsf_Emu::init_sound table.
    fn voice_chan(&mut self, i: usize, mask: i32) -> Option<*mut BlipBuffer> {
        if mask & (1 << i) != 0 {
            return None;
        }
        let t = match i {
            0 => CHANNEL_WAVE_TYPE | 1,
            1 => CHANNEL_WAVE_TYPE | 2,
            2 => CHANNEL_WAVE_TYPE,
            3 => CHANNEL_NOISE_TYPE,
            4 => CHANNEL_MIXED_TYPE | 1,
            _ => CHANNEL_WAVE_TYPE | (3 + (i - 5)) as i32,
        };
        Some(self.stereo_buf.channel(i as i32, t).center)
    }

    /// Wire all oscillators to their Effects_Buffer channels per the mute
    /// mask (C++ Classic_Emu::mute_voices_ + Nsf_Emu::set_voice).
    fn wire_voices(&mut self, mask: i32) {
        // Base APU: voices 0-4 (Square1, Square2, Triangle, Noise, DMC)
        self.apu.square1.env.osc.output = self.voice_chan(0, mask);
        self.apu.square2.env.osc.output = self.voice_chan(1, mask);
        self.apu.triangle.osc.output = self.voice_chan(2, mask);
        self.apu.noise.env.osc.output = self.voice_chan(3, mask);
        self.apu.dmc.osc.output = self.voice_chan(4, mask);

        // Expansion voices start at index 5; at most one chip per file.
        let v: Vec<Option<*mut BlipBuffer>> = (5..13).map(|i| self.voice_chan(i, mask)).collect();
        // VRC6: C++ puts the saw first (HANDLE_CHIP_VRC6 `if (--i < 0) i = 2`):
        // voice 5 = Saw, 6 = Pulse 1, 7 = Pulse 2.
        if let Some(ref mut vrc6) = self.vrc6 {
            vrc6.osc_output(2, v[0]);
            vrc6.osc_output(0, v[1]);
            vrc6.osc_output(1, v[2]);
        }
        if let Some(ref mut fds) = self.fds {
            fds.set_output(v[0]);
        }
        if let Some(ref mut fme7) = self.fme7 {
            for (i, &buf) in v.iter().enumerate().take(3) {
                fme7.osc_output(i, buf);
            }
        }
        if let Some(ref mut namco) = self.namco {
            for (i, &buf) in v.iter().enumerate().take(crate::oscs::namco::OSC_COUNT) {
                namco.osc_output(i, buf);
            }
        }
        if let Some(ref mut mmc5) = self.mmc5 {
            for (i, &buf) in v.iter().enumerate().take(crate::oscs::mmc5::OSC_COUNT) {
                mmc5.osc_output(i, buf);
            }
        }
        if let Some(ref mut vrc7) = self.vrc7 {
            for (i, &buf) in v.iter().enumerate().take(crate::oscs::vrc7::OSC_COUNT) {
                vrc7.osc_output(i, buf);
            }
        }
    }

    /// C++ Classic_Emu::play_: read from the stereo buffer, running emulation
    /// frames as needed. No silence/fade/out_time bookkeeping — that lives in
    /// play_impl (C++ Music_Emu::play).
    fn raw_play(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let mut remaining = count;
        let mut pos = 0usize;
        while remaining > 0 {
            let avail = self.stereo_buf.samples_avail();
            if avail > 0 {
                let to_read = remaining.min(avail as i32);
                let read = self
                    .stereo_buf
                    .read_samples(&mut out[pos..], to_read as i64);
                pos += read as usize;
                remaining -= read as i32;
            }

            if remaining > 0 {
                let msec = self.stereo_buf.length();
                let clock_rate = if self.header.is_pal() {
                    1662607i32
                } else {
                    1789773i32
                };
                let mut clocks = msec * clock_rate / 1000;
                self.run_clocks(&mut clocks)?;
            }
        }
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
}

impl GmeFile for NsfEmu {
    fn format(&self) -> Format {
        Format::Nsf
    }
    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }
    fn set_raw_track_count(&mut self, n: i32) {
        self.state.set_track_count(n);
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        // NSFE path: parse chunks, build virtual NSF stream
        if data.len() >= 4 && &data[..4] == b"NSFE" {
            let nsfe = NsfeInfo::parse(data)?;
            let data_end = nsfe.data_offset.saturating_add(nsfe.data_size);
            if data_end > data.len() {
                return Err(GmeError::CorruptFile);
            }
            let mut virtual_nsf = Vec::with_capacity(NsfHeader::SIZE + nsfe.data_size);
            virtual_nsf.extend_from_slice(&nsfe.header.to_bytes());
            virtual_nsf.extend_from_slice(&data[nsfe.data_offset..data_end]);

            let header = NsfHeader::parse(&virtual_nsf).ok_or(GmeError::WrongFileType)?;
            self.track_count = nsfe.track_count();
            let _header_bytes = self.rom.load(&virtual_nsf, NsfHeader::SIZE)?;
            self.header = header;
            self.load_addr = self.header.load_addr;
            self.init_addr = self.header.init_addr;
            self.play_addr = self.header.play_addr;
            self.validate_addrs()?;
            // C++ order: rom.set_addr first (computes rounded size), then banks
            self.rom
                .set_addr(self.load_addr as i32 % BANK_SIZE as i32, BANK_SIZE as i32);
            self.setup_bank_mapping();
            self.cpu.rom_data = self.rom.data.clone();

            for i in 0..BANK_COUNT {
                let offset = self
                    .rom
                    .mask_addr(self.initial_banks[i] as i32 * BANK_SIZE as i32);
                let rom_offset = self.rom.at_addr(offset);
                self.cpu.map_code(
                    (i as u16 + 8) * BANK_SIZE as u16,
                    BANK_SIZE as u16,
                    rom_offset,
                );
            }

            self.emu_state.voice_count = 5;
            self.state.set_track_count(self.track_count);
            self.nsfe_info = Some(nsfe);
            return self.reinit_audio_after_load();
        }

        // Standard NSF path
        let header = NsfHeader::parse(data).ok_or(GmeError::WrongFileType)?;
        if data[5] != 1 {
            self.state.set_warning("Unknown file version");
        }
        self.track_count = 256;
        let _header_bytes = self.rom.load(data, NsfHeader::SIZE)?;
        self.header = header;
        self.load_addr = self.header.load_addr;
        self.init_addr = self.header.init_addr;
        self.play_addr = self.header.play_addr;
        self.validate_addrs()?;
        // C++ order: rom.set_addr first (computes rounded size), then banks
        // Match C++ rom.set_addr(load_addr % bank_size) which prepends padding
        // to the ROM data so bank mapping works for non-aligned load addresses.
        self.rom
            .set_addr(self.load_addr as i32 % BANK_SIZE as i32, BANK_SIZE as i32);
        self.setup_bank_mapping();
        self.cpu.rom_data = self.rom.data.clone();

        for i in 0..BANK_COUNT {
            let offset = self
                .rom
                .mask_addr(self.initial_banks[i] as i32 * BANK_SIZE as i32);
            let rom_offset = self.rom.at_addr(offset);
            self.cpu.map_code(
                (i as u16 + 8) * BANK_SIZE as u16,
                BANK_SIZE as u16,
                rom_offset,
            );
        }

        self.emu_state.voice_count = 5;
        self.state.set_track_count(self.track_count);
        self.reinit_audio_after_load()
    }

    fn clear_playlist_impl(&mut self) {
        if let Some(info) = &mut self.nsfe_info {
            info.playlist.clear();
            self.track_count = info.track_count();
            self.state.set_track_count(self.track_count);
        }
    }

    fn track_info_impl(&self, info: &mut TrackInfo, track: i32) -> GmeResult {
        if let Some(ref nsfe) = self.nsfe_info {
            let remapped = nsfe.remap_track(track);
            info.system = "Nintendo NES".to_string();
            info.game = nsfe.game.clone();
            info.author = nsfe.author.clone();
            info.copyright = nsfe.copyright.clone();
            info.dumper = nsfe.dumper.clone();
            info.track_count = nsfe.track_count() as i64;
            if remapped >= 0 {
                let idx = remapped as usize;
                if idx < nsfe.track_names.len() {
                    info.song = nsfe.track_names[idx].clone();
                }
                if idx < nsfe.track_times.len() {
                    let ms = nsfe.track_times[idx];
                    if ms > 0 {
                        info.length = ms as i64;
                    }
                }
            }
            return Ok(());
        }
        info.system = "Nintendo NES".to_string();
        info.game = self.header.game_name_str();
        info.author = self.header.artist_name_str();
        info.copyright = self.header.copyright_str();
        info.track_count = self.track_count as i64;
        Ok(())
    }
}

impl MusicEmulator for NsfEmu {
    fn format(&self) -> Format {
        Format::Nsf
    }

    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        self.emu_state.sample_rate = sample_rate;
        self.init_audio(sample_rate)
    }

    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        // C++ Music_Emu::start_track stores the pre-remap track, so a
        // restart (e.g. seeking backward) re-runs the same remapping.
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;

        // Remap track for NSFE playlist
        let track = if let Some(ref nsfe) = self.nsfe_info {
            nsfe.remap_track(track)
        } else {
            track
        };

        // Match C++ Classic_Emu::start_track_ which clears buffer first
        self.stereo_buf.clear();

        // Match C++ Nsf_Emu::start_track_: CPU reset + bank mapping
        self.cpu.low_mem.fill(0);
        self.sram.fill(0);
        self.cpu.reset();
        self.cpu.badop_addr = BADOP_ADDR;
        self.saved_state.pc = BADOP_ADDR;
        self.cpu
            .start(self.init_addr, track as u8, self.header.is_pal());

        for i in (0..0x8000).step_by(0x800) {
            self.cpu.map_code(i as u16, 0x800, 0);
        }
        for i in 0..BANK_COUNT {
            self.banks[i] = self.initial_banks[i];
            let offset = self
                .rom
                .mask_addr(self.initial_banks[i] as i32 * BANK_SIZE as i32);
            let rom_offset = self.rom.at_addr(offset);
            let bank_addr = (i as u16 + 8) * BANK_SIZE as u16;
            self.cpu.map_code(bank_addr, BANK_SIZE as u16, rom_offset);
        }

        // Reset expansion chips, then re-wire all oscillator outputs
        if let Some(ref mut vrc6) = self.vrc6 {
            vrc6.reset();
        }
        if let Some(ref mut fds) = self.fds {
            fds.reset();
        }
        if let Some(ref mut fme7) = self.fme7 {
            fme7.reset();
        }
        if let Some(ref mut namco) = self.namco {
            namco.reset();
        }
        if let Some(ref mut mmc5) = self.mmc5 {
            // C++ start_track_ clears the multiplier and exram before reset
            self.mmc5_mul = [0; 2];
            mmc5.exram = [0; crate::oscs::mmc5::EXRAM_SIZE];
            mmc5.reset();
        }
        if let Some(ref mut vrc7) = self.vrc7 {
            vrc7.reset();
        }
        let mask = self.mute_mask;
        self.wire_voices(mask);

        // APU initialization matching C++ start_track_():
        self.apu.pal_mode = self.header.is_pal();
        let initial_dmc_dac = if self.header.speed_flags() & 0x20 != 0 {
            0x3F
        } else {
            0x00
        };
        self.apu.reset_and_init(initial_dmc_dac);
        self.apu.write_register(0, 0x4015, 0x0F);
        let frame_mode: u8 = if self.header.speed_flags() & 0x10 != 0 {
            0x80
        } else {
            0
        };
        self.apu.write_register(0, 0x4017, frame_mode);

        self.set_tempo_impl(self.emu_state.tempo);
        // Set up for play routine calls.
        // Use sentinel values — run_clocks handles saved_state.pc == badop_addr correctly.
        self.saved_state.pc = self.cpu.badop_addr; // sentinel = no saved state
        self.play_ready = 4;
        self.play_extra = 0;
        self.next_play = self.play_period / CLOCK_DIVISOR;

        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;
        self.emu_state.emu_time = 0;
        self.emu_state.buf_remain = 0;
        self.emu_state.silence_time = 0;
        self.emu_state.silence_count = 0;
        self.emu_state.out_time = 0;
        self.emu_state.out_time_scaled = 0;

        // Apply stored mute mask before pre-fill (matches C++ remute_voices in start_track_)
        if self.mute_mask != 0 {
            self.mute_voices_impl(self.mute_mask);
        }

        // Pre-fill: run emulator until non-silent audio found (C++ Music_Emu::start_track)
        if !self.emu_state.ignore_silence_flag {
            let end = self.emu_state.max_initial_silence as i64
                * i64::from(self.emu_state.out_channels())
                * self.emu_state.sample_rate as i64;
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

    // C++ Music_Emu::play (structure proven bit-exact by gme-spc)
    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let out_count = count as usize;

        if !self.buf_initialized || self.emu_state.track_ended {
            out[..out_count].fill(0);
            self.emu_state.advance_output(count);
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
                    > SILENCE_MAX_SECS
                        * i64::from(self.emu_state.out_channels())
                        * self.emu_state.sample_rate as i64
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

        self.emu_state.advance_output(count);
        Ok(())
    }

    /// Transliteration of C++ Music_Emu::skip / Music_Emu::skip_.
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
            self.emu_state.emu_time += count;

            // C++ Music_Emu::skip_: for long skips, mute sound
            let threshold = 30000;
            let mut scratch = [0i16; SILENCE_BUF_SIZE as usize];
            if count > threshold {
                let saved_mute = self.mute_mask;
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
        let mut count = 5; // Base APU
        if self.vrc6.is_some() {
            count += 3;
        }
        if self.fds.is_some() {
            count += 1;
        }
        if self.fme7.is_some() {
            count += 3;
        }
        if self.namco.is_some() {
            count += 8;
        }
        if self.mmc5.is_some() {
            count += 3;
        }
        if self.vrc7.is_some() {
            count += 6;
        }
        count
    }

    fn voice_names(&self) -> &[&str] {
        // Mirrors C++ Nsf_Emu::init_sound naming. NSF files use at most one
        // expansion chip, so a per-chip static slice suffices.
        if self.vrc6.is_some() {
            &[
                "Square 1", "Square 2", "Triangle", "Noise", "DMC", "Saw Wave", "Square 3",
                "Square 4",
            ]
        } else if self.fme7.is_some() {
            &[
                "Square 1", "Square 2", "Triangle", "Noise", "DMC", "Square 3", "Square 4",
                "Square 5",
            ]
        } else if self.fds.is_some() {
            &["Square 1", "Square 2", "Triangle", "Noise", "DMC", "Wave"]
        } else if self.namco.is_some() {
            &[
                "Square 1", "Square 2", "Triangle", "Noise", "DMC", "Wave 1", "Wave 2", "Wave 3",
                "Wave 4", "Wave 5", "Wave 6", "Wave 7", "Wave 8",
            ]
        } else if self.mmc5.is_some() {
            &[
                "Square 1", "Square 2", "Triangle", "Noise", "DMC", "Square 3", "Square 4", "PCM",
            ]
        } else if self.vrc7.is_some() {
            &[
                "Square 1", "Square 2", "Triangle", "Noise", "DMC", "FM 1", "FM 2", "FM 3", "FM 4",
                "FM 5", "FM 6",
            ]
        } else {
            &["Square 1", "Square 2", "Triangle", "Noise", "DMC"]
        }
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.tempo = tempo;
        let (speed, standard, clock, period) = if self.header.is_pal() {
            (
                self.header.pal_speed,
                STANDARD_PAL_SPEED,
                1662607.125,
                33247 * CLOCK_DIVISOR,
            )
        } else {
            (
                self.header.ntsc_speed,
                STANDARD_NTSC_SPEED,
                1789772.72727,
                262 * 341 * 4 - 2,
            )
        };
        let speed = if speed == 0 { standard } else { speed };
        self.play_period = if speed == standard && tempo == 1.0 {
            period
        } else {
            (f64::from(speed) * clock / (1_000_000.0 / f64::from(CLOCK_DIVISOR) * tempo)) as i32
        };
        self.apu.set_tempo(tempo);
        self.apu.dmc.pal_mode = self.apu.pal_mode;
        if let Some(ref mut fds) = self.fds {
            fds.set_tempo(tempo);
        }
    }

    fn set_equalizer_impl(&mut self, settings: &Equalizer) {
        let eq = BlipEq::new(settings.treble);
        self.apu.treble_eq(&eq);
        if let Some(ref mut vrc6) = self.vrc6 {
            vrc6.treble_eq(&eq);
        }
        if let Some(ref mut fds) = self.fds {
            fds.treble_eq(&eq);
        }
        if let Some(ref mut fme7) = self.fme7 {
            fme7.treble_eq(&eq);
        }
        if let Some(ref mut namco) = self.namco {
            namco.treble_eq(&eq);
        }
        if let Some(ref mut mmc5) = self.mmc5 {
            mmc5.treble_eq(&eq);
        }
        if let Some(ref mut vrc7) = self.vrc7 {
            vrc7.treble_eq(&eq);
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
        self.mute_mask = mask;
        self.wire_voices(mask);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gme_common::gme_file::GmeFile;

    fn load_test_nsf() -> NsfEmu {
        let data = std::fs::read("../../assets/test.nsf")
            .or_else(|_| std::fs::read("assets/test.nsf"))
            .expect("test.nsf not found");
        let mut emu = NsfEmu::new();
        // Load directly via the impl method (bypasses GmeFileState to avoid borrow issues)
        emu.load_mem_impl(&data).expect("Failed to load test.nsf");
        emu.state.set_track_count(emu.track_count);
        emu
    }

    #[test]
    fn test_load_nsf() {
        let emu = load_test_nsf();
        assert_eq!(emu.header.game_name_str(), "Pulse sweep");
        assert!(emu.track_count > 0);
    }

    #[test]
    fn test_set_sample_rate() {
        let mut emu = load_test_nsf();
        emu.set_sample_rate_impl(44100).unwrap();
        assert_eq!(emu.emu_state.sample_rate, 44100);
        assert!(emu.buf_initialized);
    }

    #[test]
    fn test_start_track() {
        let mut emu = load_test_nsf();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();
        assert!(!emu.emu_state.track_ended);
        assert!(!emu.emu_state.emu_track_ended);
    }

    #[test]
    fn test_play_produces_nonzero_output() {
        let mut emu = load_test_nsf();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();

        // Play a small amount of audio (10ms = 441 stereo samples)
        let count = 882;
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
    fn test_header_parsing() {
        let data = std::fs::read("../../assets/test.nsf")
            .or_else(|_| std::fs::read("assets/test.nsf"))
            .expect("test.nsf not found");
        let header = NsfHeader::parse(&data).unwrap();
        assert_eq!(&header.game_name[..11], b"Pulse sweep");
        assert_eq!(header.load_addr, 0x8000);
        assert!(header.init_addr >= 0x8000);
        assert!(header.play_addr >= 0x8000);
        println!("Load addr: 0x{:04X}", header.load_addr);
        println!("Init addr: 0x{:04X}", header.init_addr);
        println!("Play addr: 0x{:04X}", header.play_addr);
        println!("NTSC speed: {}", header.ntsc_speed);
        println!("Banks: {:?}", header.banks);
        println!("Region: {}", header.region);
        println!("Expansion: {}", header.expansion);
    }

    #[test]
    fn test_rom_loading() {
        let data = std::fs::read("../../assets/test.nsf")
            .or_else(|_| std::fs::read("assets/test.nsf"))
            .expect("test.nsf not found");
        let mut rom = RomData::new(0x1000);
        let _header = rom.load(&data, NsfHeader::SIZE).unwrap();
        println!("ROM file_size: {}", rom.file_size);
        println!("ROM data len: {}", rom.data.len());
        assert!(rom.file_size > 0);
    }

    #[test]
    fn test_vrc6_load_and_play() {
        let data = std::fs::read("../../assets/expansion/vrc6.nsf")
            .or_else(|_| std::fs::read("assets/expansion/vrc6.nsf"))
            .expect("vrc6.nsf not found");
        let mut emu = NsfEmu::new();
        emu.load_mem_impl(&data).expect("Failed to load vrc6.nsf");
        emu.state.set_track_count(emu.track_count);

        assert_eq!(
            emu.header.chip_flags() & VRC6_FLAG,
            VRC6_FLAG,
            "Should have VRC6 flag"
        );

        emu.set_sample_rate_impl(44100).unwrap();
        assert!(
            emu.vrc6.is_some(),
            "VRC6 chip should be created after init_audio"
        );

        emu.start_track_impl(0).unwrap();

        // Play 2 seconds
        let count = 44100 * 4; // stereo samples
        let mut out = vec![0i16; count];
        emu.play_impl(count as i32, &mut out).unwrap();

        let nonzero = out.iter().filter(|&&s| s != 0).count();
        assert!(
            nonzero > 0,
            "VRC6 should produce non-zero output, got {} / {} nonzero",
            nonzero,
            count
        );
    }

    #[test]
    fn test_fds_load_and_play() {
        let data = std::fs::read("../../assets/expansion/fds.nsf")
            .or_else(|_| std::fs::read("assets/expansion/fds.nsf"))
            .expect("fds.nsf not found");
        let mut emu = NsfEmu::new();
        emu.load_mem_impl(&data).expect("Failed to load fds.nsf");
        emu.state.set_track_count(emu.track_count);

        assert_eq!(
            emu.header.chip_flags() & FDS_FLAG,
            FDS_FLAG,
            "Should have FDS flag"
        );

        emu.set_sample_rate_impl(44100).unwrap();
        assert!(
            emu.fds.is_some(),
            "FDS chip should be created after init_audio"
        );

        emu.start_track_impl(0).unwrap();

        // Play 2 seconds
        let count = 44100 * 4;
        let mut out = vec![0i16; count];
        emu.play_impl(count as i32, &mut out).unwrap();

        let nonzero = out.iter().filter(|&&s| s != 0).count();
        assert!(
            nonzero > 0,
            "FDS should produce non-zero output, got {} / {} nonzero",
            nonzero,
            count
        );
    }

    #[test]
    fn test_voice_count_with_expansion() {
        let data = std::fs::read("../../assets/expansion/vrc6.nsf")
            .or_else(|_| std::fs::read("assets/expansion/vrc6.nsf"))
            .expect("vrc6.nsf not found");
        let mut emu = NsfEmu::new();
        emu.load_mem_impl(&data).unwrap();
        emu.set_sample_rate_impl(44100).unwrap();

        // VRC6 adds 3 voices: Saw, Square 3, Square 4
        assert_eq!(emu.voice_count(), 8, "Should have 5 base + 3 VRC6 voices");
    }

    #[test]
    fn test_fme7_voice_count() {
        let data = std::fs::read("../../assets/expansion/fme7.nsfe")
            .or_else(|_| std::fs::read("assets/expansion/fme7.nsfe"))
            .expect("fme7.nsfe not found");
        let mut emu = NsfEmu::new();
        emu.load_mem_impl(&data).unwrap();
        emu.set_sample_rate_impl(44100).unwrap();

        // FME7 adds 3 voices: Square A, B, C
        assert_eq!(emu.voice_count(), 8, "Should have 5 base + 3 FME7 voices");
    }

    #[test]
    fn test_nsfe_load_and_play() {
        let data = std::fs::read("../../assets/test.nsfe")
            .or_else(|_| std::fs::read("assets/test.nsfe"))
            .expect("test.nsfe not found");
        let mut emu = NsfEmu::new();
        emu.load_mem_impl(&data).expect("Failed to load test.nsfe");
        assert!(emu.nsfe_info.is_some(), "nsfe_info should be set");
        assert_eq!(emu.track_count, 1);

        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();

        let count = 882;
        let mut out = vec![0i16; count];
        emu.play_impl(count as i32, &mut out).unwrap();

        let nonzero = out.iter().filter(|&&s| s != 0).count();
        assert!(
            nonzero > 0,
            "NSFE should produce non-zero output, got {} / {} nonzero",
            nonzero,
            count
        );
    }

    #[test]
    fn test_fme7_nsfe_loads() {
        let data = std::fs::read("../../assets/expansion/fme7.nsfe")
            .or_else(|_| std::fs::read("assets/expansion/fme7.nsfe"))
            .expect("fme7.nsfe not found");
        let mut emu = NsfEmu::new();
        emu.load_mem_impl(&data).expect("Failed to load fme7.nsfe");
        assert!(emu.nsfe_info.is_some());
        assert!(emu.track_count > 0);

        assert_eq!(
            emu.header.chip_flags() & FME7_FLAG,
            FME7_FLAG,
            "Should have FME7 flag"
        );

        emu.set_sample_rate_impl(44100).unwrap();
        assert!(
            emu.fme7.is_some(),
            "FME7 chip should be created after init_audio"
        );

        emu.start_track_impl(0).unwrap();

        // Play 2 seconds
        let count = 44100 * 4; // stereo samples
        let mut out = vec![0i16; count];
        emu.play_impl(count as i32, &mut out).unwrap();

        let nonzero = out.iter().filter(|&&s| s != 0).count();
        assert!(
            nonzero > 0,
            "FME7 should produce non-zero output, got {} / {} nonzero",
            nonzero,
            count
        );
    }

    #[test]
    fn test_vrc6_channel_isolation() {
        let data = std::fs::read("../../assets/expansion/vrc6.nsf")
            .or_else(|_| std::fs::read("assets/expansion/vrc6.nsf"))
            .expect("vrc6.nsf not found");
        let mut emu = NsfEmu::new();
        emu.load_mem_impl(&data).unwrap();
        emu.state.set_track_count(emu.track_count);
        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();

        let count = 44100 * 4;
        let mut all_out = vec![0i16; count];
        emu.play_impl(count as i32, &mut all_out).unwrap();
        let all_nonzero = all_out.iter().filter(|&&s| s != 0).count();

        emu.start_track_impl(0).unwrap();
        emu.mute_voices_impl(0x1F);
        let mut vrc6_only = vec![0i16; count];
        emu.play_impl(count as i32, &mut vrc6_only).unwrap();
        let vrc6_nonzero = vrc6_only.iter().filter(|&&s| s != 0).count();

        emu.start_track_impl(0).unwrap();
        emu.mute_voices_impl(0xE0);
        let mut apu_only = vec![0i16; count];
        emu.play_impl(count as i32, &mut apu_only).unwrap();
        let apu_nonzero = apu_only.iter().filter(|&&s| s != 0).count();

        eprintln!("VRC6 Channel Isolation:");
        eprintln!("  All voices: {} / {} nonzero", all_nonzero, count);
        eprintln!("  VRC6 only:  {} / {} nonzero", vrc6_nonzero, count);
        eprintln!("  APU only:   {} / {} nonzero", apu_nonzero, count);

        assert!(all_nonzero > 0, "All voices should produce output");
        assert!(apu_nonzero > 0, "APU should produce output");
        assert!(vrc6_nonzero > 0, "VRC6 should produce output");
    }
}
