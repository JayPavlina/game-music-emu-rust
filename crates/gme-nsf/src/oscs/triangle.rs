use super::NesOsc;
use gme_core::blip_buffer::{BlipSynth, BlipTime};

// 1:1 port of C++ Nes_Triangle (Nes_Oscs.cpp)

pub const PHASE_RANGE: i32 = 16;

pub struct NesTriangle {
    pub osc: NesOsc,
    /// Phase domain is 1..=32 counting down (C++ semantics).
    pub phase: i32,
    pub linear_counter: i32,
}

impl Default for NesTriangle {
    fn default() -> Self {
        Self::new()
    }
}

impl NesTriangle {
    pub fn new() -> Self {
        Self {
            osc: NesOsc::new(),
            phase: 1,
            linear_counter: 0,
        }
    }

    pub fn reset(&mut self) {
        self.linear_counter = 0;
        self.phase = 1;
        self.osc.reset();
    }

    pub fn write_reg(&mut self, reg: usize, data: u8) {
        self.osc.regs[reg] = data;
        self.osc.reg_written[reg] = true;
    }

    /// C++ Nes_Triangle::clock_linear_counter.
    pub fn clock_linear_counter(&mut self) {
        if self.osc.reg_written[3] {
            self.linear_counter = (self.osc.regs[0] & 0x7F) as i32;
        } else if self.linear_counter != 0 {
            self.linear_counter -= 1;
        }

        if self.osc.regs[0] & 0x80 == 0 {
            self.osc.reg_written[3] = false;
        }
    }

    pub fn timer_period(&self) -> i32 {
        self.osc.period() + 1
    }

    /// C++ Nes_Triangle::calc_amp.
    pub fn calc_amp(&self) -> i32 {
        let mut amp = PHASE_RANGE - self.phase;
        if amp < 0 {
            amp = self.phase - (PHASE_RANGE + 1);
        }
        amp
    }

    /// C++ Nes_Triangle::maintain_phase.
    fn maintain_phase(
        &mut self,
        mut time: BlipTime,
        end_time: BlipTime,
        timer_period: BlipTime,
    ) -> BlipTime {
        let remain = end_time - time;
        if remain > 0 {
            let count = (remain + timer_period - 1) / timer_period;
            self.phase = ((self.phase as u32 + 1).wrapping_sub(count as u32)
                & (PHASE_RANGE as u32 * 2 - 1)) as i32;
            self.phase += 1;
            time += count * timer_period;
        }
        time
    }

    /// C++ Nes_Triangle::run.
    pub fn run(&mut self, mut time: BlipTime, end_time: BlipTime, synth: &BlipSynth) {
        let timer_period = self.osc.period() + 1;
        let output = match self.osc.output {
            Some(o) => unsafe { &mut *o },
            None => {
                time += self.osc.delay;
                self.osc.delay = 0;
                if self.osc.length_counter != 0 && self.linear_counter != 0 && timer_period >= 3 {
                    self.osc.delay = self.maintain_phase(time, end_time, timer_period) - end_time;
                }
                return;
            }
        };

        output.set_modified();

        // to do: track phase when period < 3 (matches C++ comment)

        let delta = self.osc.update_amp(self.calc_amp());
        if delta != 0 {
            synth.offset_at(time, delta, output);
        }

        time += self.osc.delay;
        if self.osc.length_counter == 0 || self.linear_counter == 0 || timer_period < 3 {
            time = end_time;
        } else if time < end_time {
            let mut phase = self.phase;
            let mut volume = 1;
            if phase > PHASE_RANGE {
                phase -= PHASE_RANGE;
                volume = -volume;
            }

            loop {
                phase -= 1;
                if phase == 0 {
                    phase = PHASE_RANGE;
                    volume = -volume;
                } else {
                    synth.offset_at(time, volume, output);
                }

                time += timer_period;
                if time >= end_time {
                    break;
                }
            }

            if volume < 0 {
                phase += PHASE_RANGE;
            }
            self.phase = phase;
            self.osc.last_amp = self.calc_amp();
        }
        self.osc.delay = time - end_time;
    }
}
