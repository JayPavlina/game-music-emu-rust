// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Nes_Fme7_Apu.cpp (2026-06-09), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipSynth, BlipTime};

const OSC_COUNT: usize = 3;
const REG_COUNT: usize = 14;
const AMP_RANGE: i32 = 192;

/// Logarithmic amplitude table approximating the AY-3-8910's 4-bit DAC.
#[allow(clippy::cast_possible_truncation)]
const AMP_TABLE: [u8; 16] = {
    let mut table = [0u8; 16];
    let entries: [f64; 16] = [
        0.0000, 0.0078, 0.0110, 0.0156, 0.0221, 0.0312, 0.0441, 0.0624, 0.0883, 0.1249, 0.1766,
        0.2498, 0.3534, 0.4998, 0.7070, 1.0000,
    ];
    let mut i = 0;
    while i < 16 {
        table[i] = (entries[i] * AMP_RANGE as f64 + 0.5) as u8;
        i += 1;
    }
    table
};

/// Sunsoft FME-7 sound chip: 3 square wave oscillators.
///
/// Based on the AY-3-8910 PSG. Supports tone-only mode (noise and
/// hardware envelope are not implemented, matching the C++ reference).
pub struct NesFme7Apu {
    regs: [u8; REG_COUNT],
    phases: [u8; OSC_COUNT],
    latch: u8,
    delays: [u16; OSC_COUNT],
    osc_outputs: [Option<*mut BlipBuffer>; OSC_COUNT],
    osc_last_amp: [i32; OSC_COUNT],
    last_time: BlipTime,
    synth: BlipSynth,
}

impl Default for NesFme7Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesFme7Apu {
    pub fn new() -> Self {
        let mut synth = BlipSynth::new(12);
        synth.volume_unit(0.38 / AMP_RANGE as f64);

        Self {
            regs: [0; REG_COUNT],
            phases: [0; OSC_COUNT],
            latch: 0,
            delays: [0; OSC_COUNT],
            osc_outputs: [None; OSC_COUNT],
            osc_last_amp: [0; OSC_COUNT],
            last_time: 0,
            synth,
        }
    }

    pub fn reset(&mut self) {
        self.last_time = 0;
        self.osc_last_amp = [0; OSC_COUNT];
        self.regs = [0; REG_COUNT];
        self.phases = [0; OSC_COUNT];
        self.latch = 0;
        self.delays = [0; OSC_COUNT];
    }

    /// Set volume (gain) for the FME-7 synth, matching C++ Nes_Fme7_Apu::volume().
    pub fn volume(&mut self, v: f64) {
        self.synth.volume_unit(0.38 / AMP_RANGE as f64 * v);
    }

    /// Set treble equalization (matches C++ Nes_Fme7_Apu::treble_eq).
    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synth.treble_eq(eq);
    }

    /// Set output buffer for all oscillators.
    pub fn set_output(&mut self, buf: *mut BlipBuffer) {
        self.osc_outputs.fill(Some(buf));
    }

    /// Set output buffer for a specific oscillator.
    pub fn osc_output(&mut self, index: usize, buf: Option<*mut BlipBuffer>) {
        if index < OSC_COUNT {
            self.osc_outputs[index] = buf;
        }
    }

    /// Write to the latch register ($C000).
    pub fn write_latch(&mut self, data: u8) {
        self.latch = data;
    }

    /// Write data to the selected register ($E000).
    pub fn write_data(&mut self, time: BlipTime, data: u8) {
        if self.latch as usize >= REG_COUNT {
            return;
        }
        self.run_until(time);
        self.regs[self.latch as usize] = data;
    }

    fn run_until(&mut self, end_time: BlipTime) {
        for index in 0..OSC_COUNT {
            let mode = self.regs[7] >> index;
            let vol_mode = self.regs[8 + index];
            let mut volume = AMP_TABLE[(vol_mode & 0x0F) as usize] as i32;

            let osc_output = match self.osc_outputs[index] {
                Some(o) => unsafe { &mut *o },
                None => {
                    continue;
                }
            };
            osc_output.set_modified();

            // Check for unsupported mode (noise + envelope)
            if (mode & 0x01) | (vol_mode & 0x10) != 0 {
                volume = 0;
            }

            // Period calculation
            let period_factor = 16;
            let mut period = (self.regs[index * 2 + 1] & 0x0F) as i32 * 0x100 * period_factor
                + self.regs[index * 2] as i32 * period_factor;
            if period < 50 {
                volume = 0;
                if period == 0 {
                    period = period_factor;
                }
            }

            // Current amplitude
            let amp = if self.phases[index] != 0 { volume } else { 0 };
            {
                let delta = amp - self.osc_last_amp[index];
                if delta != 0 {
                    self.osc_last_amp[index] = amp;
                    self.synth.offset_at(self.last_time, delta, osc_output);
                }
            }

            let mut time = self.last_time + self.delays[index] as BlipTime;
            if time < end_time {
                let mut delta = amp * 2 - volume;
                if volume != 0 {
                    loop {
                        delta = -delta;
                        self.synth.offset_at(time, delta, osc_output);
                        time += period;
                        if time >= end_time {
                            break;
                        }
                    }
                    self.osc_last_amp[index] = (delta + volume) >> 1;
                    self.phases[index] = if delta > 0 { 1 } else { 0 };
                } else {
                    // Maintain phase when silent
                    let count = (end_time - time + period - 1) / period;
                    self.phases[index] ^= (count & 1) as u8;
                    time += count * period;
                }
            }

            self.delays[index] = (time - end_time) as u16;
        }

        self.last_time = end_time;
    }

    pub fn end_frame(&mut self, time: BlipTime) {
        if time > self.last_time {
            self.run_until(time);
        }
        self.last_time -= time;
    }
}
