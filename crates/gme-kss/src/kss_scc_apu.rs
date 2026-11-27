// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Kss_Scc_Apu.cpp (2026-06-09), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// Konami SCC sound chip emulator.
/// Port of C++ Kss_Scc_Apu.h / Kss_Scc_Apu.cpp.
use gme_core::blip_buffer::{BLIP_MED_QUALITY, BlipBuffer, BlipSynth, BlipTime};

pub const REG_COUNT: usize = 0x90;
pub const OSC_COUNT: usize = 5;
const WAVE_SIZE: usize = 0x20;
const AMP_RANGE: u32 = 0x8000;
const INAUDIBLE_FREQ: u32 = 16384;

struct Osc {
    delay: BlipTime,
    phase: usize,
    last_amp: i32,
    output: *mut BlipBuffer,
}

impl Osc {
    fn new() -> Self {
        Self {
            delay: 0,
            phase: 0,
            last_amp: 0,
            output: std::ptr::null_mut(),
        }
    }
}

pub struct SccApu {
    oscs: [Osc; OSC_COUNT],
    last_time: BlipTime,
    regs: [u8; REG_COUNT],
    synth: BlipSynth,
}

// SAFETY: SccApu only stores raw pointers to BlipBuffer which are owned by the emulator struct.
unsafe impl Send for SccApu {}

impl Default for SccApu {
    fn default() -> Self {
        Self::new()
    }
}

impl SccApu {
    pub fn new() -> Self {
        Self {
            oscs: [Osc::new(), Osc::new(), Osc::new(), Osc::new(), Osc::new()],
            last_time: 0,
            regs: [0; REG_COUNT],
            synth: BlipSynth::new(BLIP_MED_QUALITY),
        }
    }

    pub fn reset(&mut self) {
        self.last_time = 0;
        for osc in &mut self.oscs {
            osc.delay = 0;
            osc.phase = 0;
            osc.last_amp = 0;
        }
        self.regs.fill(0);
    }

    pub fn output(&mut self, buf: *mut BlipBuffer) {
        for osc in &mut self.oscs {
            osc.output = buf;
        }
    }

    pub fn osc_output(&mut self, index: usize, buf: *mut BlipBuffer) {
        if index < OSC_COUNT {
            self.oscs[index].output = buf;
        }
    }

    pub fn volume(&mut self, v: f64) {
        self.synth
            .volume_unit(0.43 / OSC_COUNT as f64 / AMP_RANGE as f64 * v);
    }

    pub fn treble_eq(&mut self, eq: &gme_core::blip_buffer::BlipEq) {
        self.synth.treble_eq(eq);
    }

    pub fn write(&mut self, time: BlipTime, addr: usize, data: u8) {
        debug_assert!(addr < REG_COUNT);
        self.run_until(time);
        self.regs[addr] = data;
    }

    pub fn end_frame(&mut self, end_time: BlipTime) {
        if end_time > self.last_time {
            self.run_until(end_time);
        }
        self.last_time -= end_time;
        debug_assert!(self.last_time >= 0);
    }

    fn run_until(&mut self, end_time: BlipTime) {
        for index in 0..OSC_COUNT {
            let osc = &mut self.oscs[index];
            let output = osc.output;
            if output.is_null() {
                continue;
            }
            unsafe {
                (*output).set_modified();
            }

            let period = ((self.regs[0x80 + index * 2 + 1] & 0x0F) as BlipTime) * 0x100
                + self.regs[0x80 + index * 2] as BlipTime
                + 1;

            let mut volume = 0i32;
            if self.regs[0x8F] & (1 << index) != 0 {
                let inaudible_period = (unsafe { (*output).clock_rate_value() }
                    + INAUDIBLE_FREQ * 32)
                    / (INAUDIBLE_FREQ * 16);
                if period > inaudible_period as BlipTime {
                    volume =
                        (self.regs[0x8A + index] as i32 & 0x0F) * (AMP_RANGE as i32 / 256 / 15);
                }
            }

            // Last two oscillators share the same waveform data
            let wave_base = if index == OSC_COUNT - 1 {
                (index - 1) * WAVE_SIZE
            } else {
                index * WAVE_SIZE
            };

            let amp = self.regs[wave_base + osc.phase] as i8 as i32 * volume;
            let delta = amp - osc.last_amp;
            if delta != 0 {
                osc.last_amp = amp;
                self.synth
                    .offset_at(self.last_time, delta, unsafe { &mut *output });
            }

            let mut time = self.last_time + osc.delay;
            if time < end_time {
                if volume == 0 {
                    // maintain phase
                    let count = (end_time - time + period - 1) / period;
                    osc.phase = (osc.phase + count as usize) & (WAVE_SIZE - 1);
                    time += count * period;
                } else {
                    let mut phase = osc.phase;
                    let mut last_wave = self.regs[wave_base + phase] as i8 as i32;
                    phase = (phase + 1) & (WAVE_SIZE - 1);

                    while time < end_time {
                        let amp = self.regs[wave_base + phase] as i8 as i32;
                        phase = (phase + 1) & (WAVE_SIZE - 1);
                        let d = amp - last_wave;
                        if d != 0 {
                            last_wave = amp;
                            self.synth
                                .offset_at(time, d * volume, unsafe { &mut *output });
                        }
                        time += period;
                    }

                    osc.phase = (phase + WAVE_SIZE - 1) & (WAVE_SIZE - 1); // undo pre-advance
                    osc.last_amp = self.regs[wave_base + osc.phase] as i8 as i32 * volume;
                }
            }
            osc.delay = time - end_time;
        }
        self.last_time = end_time;
    }
}
