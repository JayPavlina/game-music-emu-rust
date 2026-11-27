// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Nes_Oscs.cpp (2026-06-09 to 2026-06-10), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use super::NesEnvelope;
use gme_core::blip_buffer::{BlipSynth, BlipTime};

// 1:1 port of C++ Nes_Square (Nes_Oscs.cpp)

const NEGATE_FLAG: u8 = 0x08;
const SHIFT_MASK: u8 = 0x07;
pub const PHASE_RANGE: i32 = 8;

pub struct NesSquare {
    pub env: NesEnvelope,
    pub phase: i32,
    pub sweep_delay: i32,
}

impl Default for NesSquare {
    fn default() -> Self {
        Self::new()
    }
}

impl NesSquare {
    pub fn new() -> Self {
        Self {
            env: NesEnvelope::new(),
            phase: 0,
            sweep_delay: 0,
        }
    }

    /// C++ Nes_Square::reset (phase is intentionally not reset).
    pub fn reset(&mut self) {
        self.sweep_delay = 0;
        self.env.reset();
    }

    pub fn write_reg(&mut self, reg: usize, data: u8) {
        self.env.osc.regs[reg] = data;
        self.env.osc.reg_written[reg] = true;
    }

    /// C++ Nes_Square::clock_sweep.
    pub fn clock_sweep(&mut self, negative_adjust: i32) {
        let sweep = self.env.osc.regs[1];

        self.sweep_delay -= 1;
        if self.sweep_delay < 0 {
            self.env.osc.reg_written[1] = true;

            let period = self.env.osc.period();
            let shift = (sweep & SHIFT_MASK) as i32;
            if shift != 0 && sweep & 0x80 != 0 && period >= 8 {
                let mut offset = period >> shift;

                if sweep & NEGATE_FLAG != 0 {
                    offset = negative_adjust - offset;
                }

                if period + offset < 0x800 {
                    let period = period + offset;
                    // rewrite period
                    self.env.osc.regs[2] = (period & 0xFF) as u8;
                    self.env.osc.regs[3] = (self.env.osc.regs[3] & !7) | ((period >> 8) & 7) as u8;
                }
            }
        }

        if self.env.osc.reg_written[1] {
            self.env.osc.reg_written[1] = false;
            self.sweep_delay = ((sweep >> 4) & 7) as i32;
        }
    }

    /// C++ Nes_Square::maintain_phase.
    fn maintain_phase(
        &mut self,
        mut time: BlipTime,
        end_time: BlipTime,
        timer_period: BlipTime,
    ) -> BlipTime {
        let remain = end_time - time;
        if remain > 0 {
            let count = (remain + timer_period - 1) / timer_period;
            self.phase = (self.phase + count) & (PHASE_RANGE - 1);
            time += count * timer_period;
        }
        time
    }

    /// C++ Nes_Square::run.
    pub fn run(&mut self, mut time: BlipTime, end_time: BlipTime, synth: &BlipSynth) {
        let period = self.env.osc.period();
        let timer_period = (period + 1) * 2;

        let output = match self.env.osc.output {
            Some(o) => unsafe { &mut *o },
            None => {
                self.env.osc.delay =
                    self.maintain_phase(time + self.env.osc.delay, end_time, timer_period)
                        - end_time;
                return;
            }
        };

        output.set_modified();

        let mut offset = period >> (self.env.osc.regs[1] & SHIFT_MASK) as i32;
        if self.env.osc.regs[1] & NEGATE_FLAG != 0 {
            offset = 0;
        }

        let volume = self.env.volume();
        if volume == 0 || period < 8 || period + offset >= 0x800 {
            if self.env.osc.last_amp != 0 {
                synth.offset_at(time, -self.env.osc.last_amp, output);
                self.env.osc.last_amp = 0;
            }

            time += self.env.osc.delay;
            time = self.maintain_phase(time, end_time, timer_period);
        } else {
            // handle duty select
            let duty_select = ((self.env.osc.regs[0] >> 6) & 3) as i32;
            let mut duty = 1 << duty_select; // 1, 2, 4, 2
            let mut amp = 0;
            if duty_select == 3 {
                duty = 2; // negated 25%
                amp = volume;
            }
            if self.phase < duty {
                amp ^= volume;
            }

            {
                let delta = self.env.osc.update_amp(amp);
                if delta != 0 {
                    synth.offset_at(time, delta, output);
                }
            }

            time += self.env.osc.delay;
            if time < end_time {
                let mut delta = amp * 2 - volume;
                let mut phase = self.phase;

                loop {
                    phase = (phase + 1) & (PHASE_RANGE - 1);
                    if phase == 0 || phase == duty {
                        delta = -delta;
                        synth.offset_at(time, delta, output);
                    }
                    time += timer_period;
                    if time >= end_time {
                        break;
                    }
                }

                self.env.osc.last_amp = (delta + volume) >> 1;
                self.phase = phase;
            }
        }

        self.env.osc.delay = time - end_time;
    }
}
