// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Nes_Fds_Apu.cpp (2026-06-09 to 2026-06-10), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipSynth, BlipTime};

const WAVE_SIZE: usize = 0x40;
const IO_ADDR: u16 = 0x4040;
const IO_SIZE: usize = 0x53;
const FRACT_RANGE: i32 = 65536;
const WAVE_SAMPLE_MAX: i32 = 0x3F;
const VOL_MAX: i32 = 0x20;
const MASTER_VOL_MAX: i32 = 10;
const LFO_BASE_TEMPO: i32 = 8;

/// FDS wavetable channel with FM modulation.
pub struct NesFdsApu {
    pub(crate) regs: [u8; IO_SIZE],
    pub(crate) mod_wave: [u8; WAVE_SIZE],

    pub(crate) lfo_tempo: i32,

    pub(crate) env_delay: i32,
    pub(crate) env_speed: i32,
    pub(crate) env_gain: i32,

    pub(crate) sweep_delay: i32,
    pub(crate) sweep_speed: i32,
    pub(crate) sweep_gain: i32,

    pub(crate) wave_pos: usize,
    pub(crate) last_amp: i32,
    pub(crate) wave_fract: i32,

    pub(crate) mod_fract: i32,
    pub(crate) mod_pos: usize,
    pub(crate) mod_write_pos: usize,

    pub(crate) last_time: BlipTime,
    pub(crate) output: Option<*mut BlipBuffer>,
    pub(crate) synth: BlipSynth,
}

impl Default for NesFdsApu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesFdsApu {
    pub const OSC_COUNT: usize = 1;

    pub fn new() -> Self {
        let mut synth = BlipSynth::new(8);
        synth.volume_unit(0.14 / MASTER_VOL_MAX as f64 / VOL_MAX as f64 / WAVE_SAMPLE_MAX as f64);

        Self {
            regs: [0; IO_SIZE],
            mod_wave: [0; WAVE_SIZE],
            lfo_tempo: LFO_BASE_TEMPO,
            env_delay: 0,
            env_speed: 0,
            env_gain: 0,
            sweep_delay: 0,
            sweep_speed: 0,
            sweep_gain: 0,
            wave_pos: 0,
            last_amp: 0,
            wave_fract: FRACT_RANGE,
            mod_fract: FRACT_RANGE,
            mod_pos: 0,
            mod_write_pos: 0,
            last_time: 0,
            output: None,
            synth,
        }
    }

    pub fn reset(&mut self) {
        self.regs.fill(0);
        self.mod_wave.fill(0);
        self.last_time = 0;
        self.env_delay = 0;
        self.sweep_delay = 0;
        self.wave_pos = 0;
        self.last_amp = 0;
        self.wave_fract = FRACT_RANGE;
        self.mod_fract = FRACT_RANGE;
        self.mod_pos = 0;
        self.mod_write_pos = 0;

        // Initialize registers to default state
        let initial_regs: [u8; 0x0B] = [
            0x80, // disable envelope
            0, 0, 0xC0, // disable wave and lfo
            0x80, // disable sweep
            0, 0, 0x80, // disable modulation
            0, 0, 0xFF, // LFO period
        ];
        for (i, _) in initial_regs.iter().enumerate() {
            self.write_(IO_ADDR + WAVE_SIZE as u16 + i as u16, 0);
            self.write_(IO_ADDR + WAVE_SIZE as u16 + i as u16, initial_regs[i]);
        }
    }

    pub fn set_output(&mut self, buf: Option<*mut BlipBuffer>) {
        self.output = buf;
    }

    pub fn set_tempo(&mut self, t: f64) {
        self.lfo_tempo = LFO_BASE_TEMPO;
        if t != 1.0 {
            self.lfo_tempo = (LFO_BASE_TEMPO as f64 / t + 0.5) as i32;
            if self.lfo_tempo <= 0 {
                self.lfo_tempo = 1;
            }
        }
    }

    pub fn volume(&mut self, v: f64) {
        self.synth.volume_unit(
            0.14 / MASTER_VOL_MAX as f64 / VOL_MAX as f64 / WAVE_SAMPLE_MAX as f64 * v,
        );
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synth.treble_eq(eq);
    }

    fn reg(&self, addr: u16) -> u8 {
        self.regs[(addr - IO_ADDR) as usize]
    }

    #[allow(dead_code)]
    fn reg_mut(&mut self, addr: u16) -> &mut u8 {
        &mut self.regs[(addr - IO_ADDR) as usize]
    }

    pub fn write(&mut self, time: BlipTime, addr: u16, data: u8) {
        self.run_until(time);
        self.write_(addr, data);
    }

    fn write_(&mut self, addr: u16, data: u8) {
        let reg = (addr - IO_ADDR) as usize;
        if reg >= IO_SIZE {
            return;
        }

        if reg < WAVE_SIZE {
            // Wave table write — only allowed when $4089 bit 7 is set
            if self.regs[0x4089 - IO_ADDR as usize] & 0x80 != 0 {
                self.regs[reg] = data & WAVE_SAMPLE_MAX as u8;
            }
        } else {
            self.regs[reg] = data;
            match addr {
                0x4080 => {
                    if data & 0x80 != 0 {
                        self.env_gain = (data & 0x3F) as i32;
                    } else {
                        self.env_speed = ((data & 0x3F) + 1) as i32;
                    }
                }
                0x4084 => {
                    if data & 0x80 != 0 {
                        self.sweep_gain = (data & 0x3F) as i32;
                    } else {
                        self.sweep_speed = ((data & 0x3F) + 1) as i32;
                    }
                }
                0x4085 => {
                    self.mod_pos = self.mod_write_pos;
                    self.regs[0x4085 - IO_ADDR as usize] = data & 0x7F;
                }
                0x4088 if self.regs[0x4087 - IO_ADDR as usize] & 0x80 != 0 => {
                    let pos = self.mod_write_pos;
                    let val = data & 0x07;
                    self.mod_wave[pos] = val;
                    self.mod_wave[pos + 1] = val;
                    self.mod_write_pos = (pos + 2) & (WAVE_SIZE - 1);
                    self.mod_pos = (self.mod_pos + 2) & (WAVE_SIZE - 1);
                }
                _ => {}
            }
        }
    }

    pub fn read(&mut self, time: BlipTime, addr: u16) -> u8 {
        self.run_until(time);

        match addr {
            0x4090 => (self.env_gain as u8) | 0x40,
            0x4092 => (self.sweep_gain as u8) | 0x40,
            _ => {
                let i = (addr - IO_ADDR) as usize;
                if i < WAVE_SIZE {
                    self.regs[i] | 0x40
                } else {
                    0xFF
                }
            }
        }
    }

    fn run_until(&mut self, final_end_time: BlipTime) {
        let wave_freq = ((self.reg(0x4083) & 0x0F) as i32) * 0x100 + self.reg(0x4082) as i32;
        let output = match self.output {
            Some(o) => unsafe { &mut *o },
            None => {
                self.last_time = final_end_time;
                return;
            }
        };

        if wave_freq != 0 && (self.reg(0x4089) | self.reg(0x4083)) & 0x80 == 0 {
            output.set_modified();
            let master_volumes: [i32; 4] = [
                (MASTER_VOL_MAX * 100 + 50) / 100,
                (MASTER_VOL_MAX * 67 + 50) / 100,
                (MASTER_VOL_MAX * 50 + 50) / 100,
                (MASTER_VOL_MAX * 40 + 50) / 100,
            ];
            let master_volume = master_volumes[(self.reg(0x4089) & 0x03) as usize];

            let lfo_period = self.reg(0x408A) as i32 * self.lfo_tempo;
            let lfo_period = if self.reg(0x4083) & 0x40 != 0 {
                0
            } else {
                lfo_period
            };

            let mut sweep_time = self.last_time + self.sweep_delay;
            let sweep_period = lfo_period * self.sweep_speed;
            if sweep_period == 0 || self.reg(0x4084) & 0x80 != 0 {
                sweep_time = final_end_time;
            }

            let mut env_time = self.last_time + self.env_delay;
            let env_period = lfo_period * self.env_speed;
            if env_period == 0 || self.reg(0x4080) & 0x80 != 0 {
                env_time = final_end_time;
            }

            let mut mod_freq = 0i32;
            if self.reg(0x4087) & 0x80 == 0 {
                mod_freq = ((self.reg(0x4087) & 0x0F) as i32) * 0x100 + self.reg(0x4086) as i32;
            }

            let mut end_time = self.last_time;
            loop {
                // Sweep
                if sweep_time <= end_time {
                    sweep_time += sweep_period;
                    let mode = (self.reg(0x4084) >> 5) as i32 & 2;
                    let new_sweep_gain = self.sweep_gain + mode - 1;
                    if (new_sweep_gain as u32) <= (0x80u32 >> mode) {
                        self.sweep_gain = new_sweep_gain;
                    } else {
                        self.regs[0x4084 - IO_ADDR as usize] |= 0x80;
                    }
                }

                // Envelope
                if env_time <= end_time {
                    env_time += env_period;
                    let mode = (self.reg(0x4080) >> 5) as i32 & 2;
                    let new_env_gain = self.env_gain + mode - 1;
                    if (new_env_gain as u32) <= (0x80u32 >> mode) {
                        self.env_gain = new_env_gain;
                    } else {
                        self.regs[0x4080 - IO_ADDR as usize] |= 0x80;
                    }
                }

                // New end_time
                let start_time = end_time;
                end_time = final_end_time;
                if end_time > env_time {
                    end_time = env_time;
                }
                if end_time > sweep_time {
                    end_time = sweep_time;
                }

                // Frequency modulation
                let mut freq = wave_freq;
                if mod_freq != 0 {
                    let mod_time = start_time + (self.mod_fract + mod_freq - 1) / mod_freq;
                    if end_time > mod_time {
                        end_time = mod_time;
                    }

                    let sweep_bias = self.reg(0x4085) as i32;
                    self.mod_fract -= (end_time - start_time) * mod_freq;
                    if self.mod_fract <= 0 {
                        self.mod_fract += FRACT_RANGE;

                        let mod_table: [i32; 8] = [0, 1, 2, 4, 0, -4, -2, -1];
                        let m = self.mod_wave[self.mod_pos] as usize;
                        self.mod_pos = (self.mod_pos + 1) & (WAVE_SIZE - 1);
                        let mut new_sweep_bias = (sweep_bias + mod_table[m]) & 0x7F;
                        if m == 4 {
                            new_sweep_bias = 0;
                        }
                        self.regs[0x4085 - IO_ADDR as usize] = new_sweep_bias as u8;
                    }

                    // Apply frequency modulation
                    let sweep_bias = (sweep_bias ^ 0x40) - 0x40;
                    let mut factor = sweep_bias * self.sweep_gain;
                    let extra = factor & 0x0F;
                    factor >>= 4;
                    if extra != 0 {
                        factor -= 1;
                        if sweep_bias >= 0 {
                            factor += 3;
                        }
                    }
                    if factor > 193 {
                        factor -= 258;
                    }
                    if factor < -64 {
                        factor += 256;
                    }
                    freq += (freq * factor) >> 6;
                }

                // Wave
                if freq > 0 {
                    let mut wave_fract = self.wave_fract;
                    let mut delay = (wave_fract + freq - 1) / freq;
                    let mut time = start_time + delay;

                    if time <= end_time {
                        let min_delay = FRACT_RANGE / freq;
                        let mut wave_pos = self.wave_pos;

                        let mut volume = self.env_gain;
                        if volume > VOL_MAX {
                            volume = VOL_MAX;
                        }
                        volume *= master_volume;

                        let min_fract = min_delay * freq;

                        loop {
                            let amp = self.regs[wave_pos] as i32 * volume;
                            wave_pos = (wave_pos + 1) & (WAVE_SIZE - 1);
                            let delta = amp - self.last_amp;
                            if delta != 0 {
                                self.last_amp = amp;
                                self.synth.offset_at(time, delta, output);
                            }

                            wave_fract += FRACT_RANGE - delay * freq;

                            delay = min_delay;
                            if wave_fract > min_fract {
                                delay += 1;
                            }

                            time += delay;
                            if time > end_time {
                                break;
                            }
                        }

                        self.wave_pos = wave_pos;
                    }
                    self.wave_fract = wave_fract - (end_time - (time - delay)) * freq;
                }

                if end_time >= final_end_time {
                    break;
                }
            }

            self.env_delay = env_time - final_end_time;
            self.sweep_delay = sweep_time - final_end_time;
        }
        self.last_time = final_end_time;
    }

    pub fn end_frame(&mut self, time: BlipTime) {
        if time > self.last_time {
            self.run_until(time);
        }
        self.last_time -= time;
    }

    #[cfg(test)]
    pub fn debug_state(&self) {
        eprintln!("\n=== Rust FDS Register State ===");
        for (i, &val) in self.regs[..IO_SIZE].iter().enumerate() {
            let addr = IO_ADDR + i as u16;
            if val != 0 || i < 0x10 || (0x40..=0x4A).contains(&i) {
                eprintln!("  ${:04X} (reg[{:02X}]) = ${:02X}", addr, i, val);
            }
        }
        eprintln!(
            "  wave_pos={}, last_amp={}, wave_fract={}",
            self.wave_pos, self.last_amp, self.wave_fract
        );
        eprintln!(
            "  env_gain={}, env_speed={}, env_delay={}",
            self.env_gain, self.env_speed, self.env_delay
        );
        eprintln!(
            "  sweep_gain={}, sweep_speed={}, sweep_delay={}",
            self.sweep_gain, self.sweep_speed, self.sweep_delay
        );
        eprintln!(
            "  mod_pos={}, mod_write_pos={}, mod_fract={}",
            self.mod_pos, self.mod_write_pos, self.mod_fract
        );
        eprintln!("  lfo_tempo={}", self.lfo_tempo);
        eprintln!("  mod_wave (first 16): {:?}", &self.mod_wave[0..16]);
    }
}
