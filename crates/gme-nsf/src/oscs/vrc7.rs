// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2009 Shay Green (Game_Music_Emu)
// Rust adaptation of Nes_Vrc7_Apu.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! Konami VRC7 sound chip emulator.
//!
//! 1:1 transliteration of the vendored C++ `Nes_Vrc7_Apu` (Game_Music_Emu),
//! which drives the emu2413 OPLL core at one FM clock per 36 NES CPU clocks.

use gme_core::blip_buffer::{BLIP_MED_QUALITY, BlipBuffer, BlipEq, BlipSynth, BlipTime};

use super::emu2413::Opll;

pub const OSC_COUNT: usize = 6;

/// NES CPU clocks per FM clock.
const PERIOD: BlipTime = 36;

#[derive(Clone, Copy)]
struct Vrc7Osc {
    regs: [u8; 3],
    output: Option<*mut BlipBuffer>,
    last_amp: i32,
}

pub struct NesVrc7Apu {
    oscs: [Vrc7Osc; OSC_COUNT],
    inst: [u8; 8],
    opll: Opll,
    addr: i32,
    next_time: BlipTime,
    mono_output: Option<*mut BlipBuffer>,
    mono_last_amp: i32,
    synth: BlipSynth,
}

impl Default for NesVrc7Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesVrc7Apu {
    /// C++ constructor + `init()`: OPLL at 3579545 Hz, output rate clk / 72,
    /// VRC7 chip type and patch set; outputs cleared, volume 1.0, reset.
    pub fn new() -> Self {
        let mut opll = Opll::new(3579545, 3579545 / 72);
        opll.set_chip_type(1);
        opll.reset_patch(1);

        let mut apu = Self {
            oscs: [Vrc7Osc {
                regs: [0; 3],
                output: None,
                last_amp: 0,
            }; OSC_COUNT],
            inst: [0; 8],
            opll,
            addr: 0,
            next_time: 0,
            mono_output: None,
            mono_last_amp: 0,
            synth: BlipSynth::new(BLIP_MED_QUALITY),
        };
        apu.output_changed();
        apu.volume(1.0);
        apu.reset();
        apu
    }

    /// C++ `Nes_Vrc7_Apu::volume`: synth.volume(1.0 / 3 / 4096 * v)
    /// (DB2LIN_AMP_BITS == 11, * 2).
    pub fn volume(&mut self, v: f64) {
        self.synth.volume_unit(1.0 / 3.0 / 4096.0 * v);
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synth.treble_eq(eq);
    }

    /// Set output buffer for all oscillators.
    pub fn set_output(&mut self, buf: *mut BlipBuffer) {
        for osc in self.oscs.iter_mut() {
            osc.output = Some(buf);
        }
        self.output_changed();
    }

    /// Set output buffer for a specific oscillator.
    pub fn osc_output(&mut self, index: usize, buf: Option<*mut BlipBuffer>) {
        debug_assert!(index < OSC_COUNT);
        self.oscs[index].output = buf;
        self.output_changed();
    }

    /// C++ `output_changed`: detect whether all oscillators share one buffer
    /// (fast mono path) and fold per-osc amplitudes into the mono amp.
    fn output_changed(&mut self) {
        self.mono_output = self.oscs[0].output;
        for i in (1..OSC_COUNT).rev() {
            if self.mono_output != self.oscs[i].output {
                self.mono_output = None;
                break;
            }
        }

        if self.mono_output.is_some() {
            // Note: matches the C++ loop, which folds oscs[1..6] only.
            for i in (1..OSC_COUNT).rev() {
                self.mono_last_amp += self.oscs[i].last_amp;
                self.oscs[i].last_amp = 0;
            }
        }
    }

    pub fn reset(&mut self) {
        self.addr = 0;
        self.next_time = 0;
        self.mono_last_amp = 0;

        for osc in self.oscs.iter_mut().rev() {
            osc.last_amp = 0;
            osc.regs = [0; 3];
        }

        self.opll.reset();
    }

    /// Write to the register latch ($9010).
    pub fn write_reg(&mut self, data: u8) {
        self.addr = data as i32;
    }

    /// Write to the data port ($9030).
    pub fn write_data(&mut self, time: BlipTime, data: u8) {
        let reg_type = (self.addr >> 4) - 1;
        let chan = (self.addr & 15) as usize;
        if (reg_type as u32) < 3 && chan < OSC_COUNT {
            self.oscs[chan].regs[reg_type as usize] = data;
        }

        if self.addr < 0x08 {
            self.inst[self.addr as usize] = data;
        }

        if time > self.next_time {
            self.run_until(time);
        }

        self.opll.write_io(0, self.addr as u8);
        self.opll.write_io(1, data);
    }

    pub fn end_frame(&mut self, time: BlipTime) {
        if time > self.next_time {
            self.run_until(time);
        }

        self.next_time -= time;
        debug_assert!(self.next_time >= 0);

        for osc in self.oscs.iter().rev() {
            if let Some(output) = osc.output {
                unsafe { (*output).set_modified() };
            }
        }
    }

    fn run_until(&mut self, end_time: BlipTime) {
        debug_assert!(end_time > self.next_time);

        let mut time = self.next_time;
        let mut buffer = [0i32; 2];

        if let Some(mono_output) = self.mono_output {
            // optimal case
            let output = unsafe { &mut *mono_output };
            loop {
                self.opll.calc_stereo(&mut buffer);
                let amp = buffer[0] + buffer[1];
                let delta = amp - self.mono_last_amp;
                if delta != 0 {
                    self.mono_last_amp = amp;
                    self.synth.offset_at(time, delta, output);
                }
                time += PERIOD;
                if time >= end_time {
                    break;
                }
            }
        } else {
            self.mono_last_amp = 0;
            loop {
                self.opll.calc_stereo(&mut buffer);
                for i in 0..OSC_COUNT {
                    let osc = &self.oscs[i];
                    if let Some(output) = osc.output {
                        let amp = self.opll.ch_out(i) as i32;
                        let delta = amp - osc.last_amp;
                        if delta != 0 {
                            self.oscs[i].last_amp = amp;
                            self.synth.offset_at(time, delta, unsafe { &mut *output });
                        }
                    }
                }
                time += PERIOD;
                if time >= end_time {
                    break;
                }
            }
        }

        self.next_time = time;
    }
}
