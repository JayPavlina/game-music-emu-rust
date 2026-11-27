// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Nes_Oscs.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
pub mod dmc;
pub use gme_core::emu2413;
pub mod fds;
pub mod fme7;
pub mod mmc5;
pub mod namco;
pub mod noise;
pub mod square;
pub mod triangle;
pub mod vrc6;
pub mod vrc7;

use gme_core::blip_buffer::BlipBuffer;

/// Base oscillator state.
pub struct NesOsc {
    pub regs: [u8; 4],
    pub reg_written: [bool; 4],
    pub output: Option<*mut BlipBuffer>,
    pub length_counter: i32,
    pub delay: i32,
    pub last_amp: i32,
}

impl Default for NesOsc {
    fn default() -> Self {
        Self::new()
    }
}

impl NesOsc {
    pub fn new() -> Self {
        Self {
            regs: [0; 4],
            reg_written: [false; 4],
            output: None,
            length_counter: 0,
            delay: 0,
            last_amp: 0,
        }
    }

    /// C++ Nes_Osc::reset only clears delay and last_amp; registers are
    /// re-initialized by the APU's reset write sequence.
    pub fn reset(&mut self) {
        self.delay = 0;
        self.last_amp = 0;
    }

    /// Raw 11-bit period from regs 2/3 (C++ Nes_Osc::period).
    #[inline]
    pub fn period(&self) -> i32 {
        ((self.regs[3] & 7) as i32) * 0x100 + self.regs[2] as i32
    }

    /// C++ Nes_Osc::update_amp.
    #[inline]
    pub fn update_amp(&mut self, amp: i32) -> i32 {
        let delta = amp - self.last_amp;
        self.last_amp = amp;
        delta
    }

    /// C++ Nes_Osc::clock_length.
    #[inline]
    pub fn clock_length(&mut self, halt_mask: u8) {
        if self.length_counter != 0 && self.regs[0] & halt_mask == 0 {
            self.length_counter -= 1;
        }
    }
}

/// Envelope oscillator (base for square and noise).
pub struct NesEnvelope {
    pub osc: NesOsc,
    pub envelope: i32,
    pub env_delay: i32,
}

impl Default for NesEnvelope {
    fn default() -> Self {
        Self::new()
    }
}

impl NesEnvelope {
    pub fn new() -> Self {
        Self {
            osc: NesOsc::new(),
            envelope: 0,
            env_delay: 0,
        }
    }

    pub fn reset(&mut self) {
        self.osc.reset();
        self.envelope = 0;
        self.env_delay = 0;
    }

    #[inline]
    pub fn clock_envelope(&mut self) {
        let period = (self.osc.regs[0] & 15) as i32;
        if self.osc.reg_written[3] {
            self.osc.reg_written[3] = false;
            self.env_delay = period;
            self.envelope = 15;
        } else {
            self.env_delay -= 1;
            if self.env_delay < 0 {
                self.env_delay = period;
                if self.envelope > 0 || self.osc.regs[0] & 0x20 != 0 {
                    self.envelope = (self.envelope - 1) & 15;
                }
            }
        }
    }

    #[inline]
    pub fn volume(&self) -> i32 {
        if self.osc.length_counter == 0 {
            0
        } else if self.osc.regs[0] & 0x10 != 0 {
            (self.osc.regs[0] & 15) as i32
        } else {
            self.envelope
        }
    }
}

/// Length counter table (NTSC).
pub const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
