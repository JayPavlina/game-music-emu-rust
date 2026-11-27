// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2009 Shay Green (Game_Music_Emu)
// Rust adaptation of Nes_Mmc5_Apu.h (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipTime};

use crate::apu::NesApu;

pub const OSC_COUNT: usize = 3;
pub const EXRAM_SIZE: usize = 1024;

/// MMC5 sound: two extra squares + PCM, implemented as a second Nes_Apu with
/// register remapping. Transliterated from C++ Nes_Mmc5_Apu.h.
pub struct NesMmc5Apu {
    pub apu: NesApu,
    pub exram: [u8; EXRAM_SIZE],
}

impl Default for NesMmc5Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesMmc5Apu {
    pub const REGS_ADDR: u16 = 0x5000;
    pub const REGS_SIZE: u16 = 0x16;

    pub fn new() -> Self {
        Self {
            apu: NesApu::new(),
            exram: [0; EXRAM_SIZE],
        }
    }

    pub fn reset(&mut self) {
        self.apu.reset();
    }

    pub fn volume(&mut self, v: f64) {
        self.apu.volume(v);
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.apu.treble_eq(eq);
    }

    pub fn end_frame(&mut self, time: BlipTime) {
        self.apu.end_frame(time);
    }

    pub fn write_register(&mut self, time: BlipTime, addr: u16, data: u8) {
        match addr {
            // channel enables: squares only, then fall through to the APU
            0x5015 => self.apu.write_register(time, addr - 0x1000, data & 0x03),
            0x5000 | 0x5002 | 0x5003 // Square 1
            | 0x5004 | 0x5006 | 0x5007 // Square 2
            | 0x5011 // DAC
            => self.apu.write_register(time, addr - 0x1000, data),
            0x5010 => {} // some things write to this for some reason
            _ => {}
        }
    }

    /// in: square 1, square 2, PCM
    /// out: square 1, square 2, skipped, skipped, PCM
    pub fn osc_output(&mut self, i: usize, buf: Option<*mut BlipBuffer>) {
        debug_assert!(i < OSC_COUNT);
        let i = if i > 1 { i + 2 } else { i };
        match i {
            0 => self.apu.square1.env.osc.output = buf,
            1 => self.apu.square2.env.osc.output = buf,
            4 => self.apu.dmc.osc.output = buf,
            _ => {}
        }
    }

    pub fn set_output(&mut self, buf: *mut BlipBuffer) {
        let buf = if buf.is_null() { None } else { Some(buf) };
        for i in 0..OSC_COUNT {
            self.osc_output(i, buf);
        }
    }
}
