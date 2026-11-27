// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Nes_Oscs.cpp (2026-06-09 to 2026-06-10), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use super::NesOsc;
use gme_core::blip_buffer::{BlipSynth, BlipTime};

// 1:1 port of C++ Nes_Dmc (Nes_Oscs.cpp)

/// C++ Nes_Apu::no_irq = INT_MAX/2 + 1.
pub const NO_IRQ: BlipTime = i32::MAX / 2 + 1;

const LOOP_FLAG: u8 = 0x40;

/// DMC period table: [NTSC, PAL].
const DMC_PERIOD_TABLE: [[i32; 16]; 2] = [
    [
        428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
    ], // NTSC
    [
        398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
    ], // PAL
];

/// Raw-pointer view of the CPU memory map used for DPCM sample fetches
/// (mirrors C++ Nsf_Emu's prg_reader). The pointers target arrays inside the
/// emulator struct, which can move (e.g. when boxed), so this is only valid
/// for a single emulation call: the emulator refreshes it at the start of
/// each frame while it holds &mut self.
#[derive(Clone, Copy)]
pub struct DmcReader {
    pub rom_data: *const u8,
    pub rom_len: usize,
    pub code_map: *const [usize; 33],
    pub low_mem: *const [u8; 0x800],
    pub sram: *const [u8; 0x2000],
}

impl DmcReader {
    fn read(&self, addr: u16) -> u8 {
        unsafe {
            match addr {
                0x0000..=0x1FFF => (*self.low_mem)[addr as usize & 0x7FF],
                0x6000..=0x7FFF => (*self.sram)[(addr - 0x6000) as usize],
                0x8000..=0xFFFF => {
                    let page = addr as usize >> 11;
                    let idx = (*self.code_map)[page].wrapping_add(addr as usize);
                    if idx < self.rom_len {
                        *self.rom_data.add(idx)
                    } else {
                        (addr >> 8) as u8
                    }
                }
                _ => (addr >> 8) as u8,
            }
        }
    }
}

/// Non-linear DAC curve (C++ dac_table).
const DAC_TABLE: [u8; 128] = [
    0, 1, 2, 3, 4, 5, 6, 7, 7, 8, 9, 10, 11, 12, 13, 14, //
    15, 15, 16, 17, 18, 19, 20, 20, 21, 22, 23, 24, 24, 25, 26, 27, //
    27, 28, 29, 30, 31, 31, 32, 33, 33, 34, 35, 36, 36, 37, 38, 38, //
    39, 40, 41, 41, 42, 43, 43, 44, 45, 45, 46, 47, 47, 48, 48, 49, //
    50, 50, 51, 52, 52, 53, 53, 54, 55, 55, 56, 56, 57, 58, 58, 59, //
    59, 60, 60, 61, 61, 62, 63, 63, 64, 64, 65, 65, 66, 66, 67, 67, //
    68, 68, 69, 70, 70, 71, 71, 72, 72, 73, 73, 74, 74, 75, 75, 75, //
    76, 76, 77, 77, 78, 78, 79, 79, 80, 80, 81, 81, 82, 82, 82, 83,
];

pub struct NesDmc {
    pub osc: NesOsc,
    /// 15-bit offset of next byte to read; CPU address is $8000 + address.
    pub address: i32,
    pub period: i32,
    pub buf: i32,
    pub bits_remain: i32,
    pub bits: i32,
    pub buf_full: bool,
    pub silence: bool,
    pub dac: i32,
    pub next_irq: BlipTime,
    pub irq_enabled: bool,
    pub irq_flag: bool,
    pub pal_mode: bool,
    pub nonlinear: bool,
    /// Memory context for DPCM sample fetches; refreshed by the emulator
    /// before each emulation frame.
    pub reader: Option<DmcReader>,
    /// C++ fill_buffer does `apu->osc_enables &= ~0x10` via back-pointer; we
    /// set this instead and the APU consumes it right after run()/start().
    pub pending_disable: bool,
    /// C++ calls apu->irq_changed() via back-pointer; the APU consumes this
    /// the same way.
    pub irq_dirty: bool,
}

impl Default for NesDmc {
    fn default() -> Self {
        Self::new()
    }
}

impl NesDmc {
    pub fn new() -> Self {
        Self {
            osc: NesOsc::new(),
            address: 0,
            period: 0x1AC,
            buf: 0,
            bits_remain: 1,
            bits: 0,
            buf_full: false,
            silence: true,
            dac: 0,
            next_irq: NO_IRQ,
            irq_enabled: false,
            irq_flag: false,
            pal_mode: false,
            nonlinear: false,
            reader: None,
            pending_disable: false,
            irq_dirty: false,
        }
    }

    /// C++ Nes_Dmc::reset.
    pub fn reset(&mut self) {
        self.address = 0;
        self.dac = 0;
        self.buf = 0;
        self.bits_remain = 1;
        self.bits = 0;
        self.buf_full = false;
        self.silence = true;
        self.next_irq = NO_IRQ;
        self.irq_flag = false;
        self.irq_enabled = false;
        self.pending_disable = false;
        self.irq_dirty = false;

        self.osc.reset();
        self.period = 0x1AC;
    }

    /// C++ Nes_Dmc::recalc_irq, with apu->last_dmc_time passed in.
    pub fn recalc_irq(&mut self, last_dmc_time: BlipTime) {
        let mut irq = NO_IRQ;
        if self.irq_enabled && self.osc.length_counter != 0 {
            irq = last_dmc_time
                + self.osc.delay
                + ((self.osc.length_counter - 1) * 8 + self.bits_remain - 1) * self.period
                + 1;
        }
        if irq != self.next_irq {
            self.next_irq = irq;
            self.irq_dirty = true;
        }
    }

    /// C++ Nes_Dmc::next_read_time (Nes_Apu.h).
    pub fn next_read_time(&self, last_dmc_time: BlipTime) -> BlipTime {
        if self.osc.length_counter == 0 {
            return NO_IRQ; // not reading
        }
        last_dmc_time + self.osc.delay + (self.bits_remain - 1) * self.period
    }

    /// C++ Nes_Dmc::write_register (regs are stored by the APU beforehand).
    pub fn write_register(&mut self, addr: usize, data: u8, last_dmc_time: BlipTime) {
        if addr == 0 {
            self.period = DMC_PERIOD_TABLE[self.pal_mode as usize][(data & 15) as usize];
            self.irq_enabled = (data & 0xC0) == 0x80; // enabled only if loop disabled
            self.irq_flag &= self.irq_enabled;
            self.recalc_irq(last_dmc_time);
        } else if addr == 1 {
            let old_dac = self.dac;
            self.dac = (data & 0x7F) as i32;

            // adjust last_amp so that "pop" amplitude will be properly non-linear
            // with respect to change in dac
            let faked_nonlinear = self.dac
                - (DAC_TABLE[self.dac as usize] as i32 - DAC_TABLE[old_dac as usize] as i32);
            if !self.nonlinear {
                self.osc.last_amp = faked_nonlinear;
            }
        }
    }

    /// C++ Nes_Dmc::start.
    pub fn start(&mut self, last_dmc_time: BlipTime) {
        self.reload_sample();
        self.fill_buffer();
        self.recalc_irq(last_dmc_time);
    }

    /// C++ Nes_Dmc::reload_sample.
    fn reload_sample(&mut self) {
        self.address = 0x4000 + self.osc.regs[2] as i32 * 0x40;
        self.osc.length_counter = self.osc.regs[3] as i32 * 0x10 + 1;
    }

    /// C++ Nes_Dmc::fill_buffer.
    fn fill_buffer(&mut self) {
        if !self.buf_full && self.osc.length_counter != 0 {
            if let Some(reader) = self.reader {
                self.buf = reader.read(0x8000u16.wrapping_add(self.address as u16)) as i32;
            }
            self.address = (self.address + 1) & 0x7FFF;
            self.buf_full = true;
            self.osc.length_counter -= 1;
            if self.osc.length_counter == 0 {
                if self.osc.regs[0] & LOOP_FLAG != 0 {
                    self.reload_sample();
                } else {
                    self.pending_disable = true;
                    self.irq_flag = self.irq_enabled;
                    self.next_irq = NO_IRQ;
                    self.irq_dirty = true;
                }
            }
        }
    }

    /// C++ Nes_Dmc::run.
    pub fn run(&mut self, mut time: BlipTime, end_time: BlipTime, synth: &BlipSynth) {
        let delta = self.osc.update_amp(self.dac);
        let out_ptr = self.osc.output;
        match out_ptr {
            None => {
                self.silence = true;
            }
            Some(o) => {
                let output = unsafe { &mut *o };
                output.set_modified();
                if delta != 0 {
                    synth.offset_at(time, delta, output);
                }
            }
        }

        time += self.osc.delay;
        if time < end_time {
            let mut bits_remain = self.bits_remain;
            if self.silence && !self.buf_full {
                let count = (end_time - time + self.period - 1) / self.period;
                bits_remain = (bits_remain - 1 + 8 - (count % 8)) % 8 + 1;
                time += count * self.period;
            } else {
                let period = self.period;
                let mut bits = self.bits;
                let mut dac = self.dac;

                loop {
                    if !self.silence {
                        let step = (bits & 1) * 4 - 2;
                        bits >>= 1;
                        if (dac + step) as u32 <= 0x7F {
                            dac += step;
                            if let Some(o) = out_ptr {
                                synth.offset_at(time, step, unsafe { &mut *o });
                            }
                        }
                    }

                    time += period;

                    bits_remain -= 1;
                    if bits_remain == 0 {
                        bits_remain = 8;
                        if !self.buf_full {
                            self.silence = true;
                        } else {
                            self.silence = false;
                            bits = self.buf;
                            self.buf_full = false;
                            if out_ptr.is_none() {
                                self.silence = true;
                            }
                            self.fill_buffer();
                        }
                    }

                    if time >= end_time {
                        break;
                    }
                }

                self.dac = dac;
                self.osc.last_amp = dac;
                self.bits = bits;
            }
            self.bits_remain = bits_remain;
        }
        self.osc.delay = time - end_time;
    }
}
