// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Sms_Apu.cpp (2026-06-09 to 2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::blip_buffer::{BLIP_GOOD_QUALITY, BLIP_MED_QUALITY, BlipBuffer, BlipSynth, BlipTime};

pub const OSC_COUNT: usize = 4;

const NOISE_PERIODS: [i32; 3] = [0x100, 0x200, 0x400];

const VOLUMES: [i32; 16] = [64, 50, 39, 31, 24, 19, 15, 12, 9, 7, 5, 4, 3, 2, 1, 0];

pub struct SmsOsc {
    pub outputs: [*mut BlipBuffer; 4],
    pub output: *mut BlipBuffer,
    pub output_select: i32,
    pub delay: i32,
    pub last_amp: i32,
    pub volume: i32,
}

impl Default for SmsOsc {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsOsc {
    pub fn new() -> Self {
        Self {
            outputs: [std::ptr::null_mut(); 4],
            output: std::ptr::null_mut(),
            output_select: 3,
            delay: 0,
            last_amp: 0,
            volume: 0,
        }
    }

    pub fn reset(&mut self) {
        self.delay = 0;
        self.last_amp = 0;
        self.volume = 0;
        self.output_select = 3;
        self.output = self.outputs[3];
    }
}

pub struct SmsSquare {
    pub base: SmsOsc,
    pub period: i32,
    pub phase: i32,
    pub synth: *const BlipSynth,
}

impl Default for SmsSquare {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsSquare {
    pub fn new() -> Self {
        Self {
            base: SmsOsc::new(),
            period: 0,
            phase: 0,
            synth: std::ptr::null(),
        }
    }

    pub fn reset(&mut self) {
        self.period = 0;
        self.phase = 0;
        self.base.reset();
    }

    pub fn run(&mut self, time: BlipTime, end_time: BlipTime) {
        let output = self.base.output;
        if output.is_null() {
            return;
        }

        if self.base.volume == 0 || self.period <= 128 {
            if self.base.last_amp != 0 {
                unsafe {
                    (*self.synth).offset_at(time, -self.base.last_amp, &mut *output);
                }
                self.base.last_amp = 0;
            }
            let mut time = time + self.base.delay;
            if self.period == 0 {
                time = end_time;
            } else if time < end_time {
                let count = (end_time - time + self.period - 1) / self.period;
                self.phase = (self.phase + count) & 1;
                time += count * self.period;
            }
            self.base.delay = time - end_time;
        } else {
            let amp = if self.phase != 0 {
                self.base.volume
            } else {
                -self.base.volume
            };
            let delta = amp - self.base.last_amp;
            if delta != 0 {
                self.base.last_amp = amp;
                unsafe {
                    (*self.synth).offset_at(time, delta, &mut *output);
                }
            }

            let mut time = time + self.base.delay;
            if time < end_time {
                let mut delta = amp * 2;
                unsafe {
                    let output_ref = &mut *output;
                    while time < end_time {
                        delta = -delta;
                        (*self.synth).offset_at(time, delta, output_ref);
                        time += self.period;
                        self.phase ^= 1;
                    }
                }
                self.base.last_amp = if self.phase != 0 {
                    self.base.volume
                } else {
                    -self.base.volume
                };
            }
            self.base.delay = time - end_time;
        }
    }
}

pub struct SmsNoise {
    pub base: SmsOsc,
    pub period: *const i32,
    pub shifter: u32,
    pub feedback: u32,
    pub synth: BlipSynth,
}

impl Default for SmsNoise {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsNoise {
    pub fn new() -> Self {
        Self {
            base: SmsOsc::new(),
            period: NOISE_PERIODS.as_ptr(),
            shifter: 0x8000,
            feedback: 0x9000,
            synth: BlipSynth::new(BLIP_MED_QUALITY),
        }
    }

    pub fn reset(&mut self) {
        self.period = NOISE_PERIODS.as_ptr();
        self.shifter = 0x8000;
        self.feedback = 0x9000;
        self.base.reset();
    }

    pub fn run(&mut self, time: BlipTime, end_time: BlipTime) {
        let output = self.base.output;
        if output.is_null() {
            return;
        }

        let mut amp = self.base.volume;
        if self.shifter & 1 != 0 {
            amp = -amp;
        }

        let delta = amp - self.base.last_amp;
        if delta != 0 {
            self.base.last_amp = amp;
            self.synth.offset_at(time, delta, unsafe { &mut *output });
        }

        let mut time = time + self.base.delay;
        if self.base.volume == 0 {
            time = end_time;
        }

        if time < end_time {
            let mut shifter = self.shifter;
            let mut delta = amp * 2;
            let mut period = unsafe { *self.period } * 2;
            if period == 0 {
                period = 16;
            }

            unsafe {
                let output_ref = &mut *output;
                while time < end_time {
                    let changed = shifter.wrapping_add(1);
                    let bit = shifter & 1;
                    shifter = (self.feedback & (0u32.wrapping_sub(bit))) ^ (shifter >> 1);
                    if changed & 2 != 0 {
                        delta = -delta;
                        self.synth.offset_at(time, delta, output_ref);
                    }
                    time += period;
                }
            }

            self.shifter = shifter;
            self.base.last_amp = delta >> 1;
        }
        self.base.delay = time - end_time;
    }
}

pub struct SmsApu {
    squares: [SmsSquare; 3],
    noise: SmsNoise,
    square_synth: BlipSynth,
    last_time: BlipTime,
    latch: i32,
    noise_feedback: u32,
    looped_feedback: u32,
}

impl Default for SmsApu {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsApu {
    pub fn new() -> Self {
        let square_synth = BlipSynth::new(BLIP_GOOD_QUALITY);
        let mut squares = [SmsSquare::new(), SmsSquare::new(), SmsSquare::new()];
        for sq in &mut squares {
            sq.synth = &square_synth as *const BlipSynth;
        }
        Self {
            squares,
            noise: SmsNoise::new(),
            square_synth,
            last_time: 0,
            latch: 0,
            noise_feedback: 0,
            looped_feedback: 0,
        }
    }

    pub fn volume(&mut self, vol: f64) {
        let vol = vol * 0.85 / (OSC_COUNT as f64 * 64.0 * 2.0);
        self.square_synth.volume(vol, 1);
        self.noise.synth.volume(vol, 1);
    }

    pub fn treble_eq(&mut self, eq: &gme_core::blip_buffer::BlipEq) {
        self.square_synth.treble_eq(eq);
        self.noise.synth.treble_eq(eq);
    }

    pub fn output(
        &mut self,
        center: *mut BlipBuffer,
        left: *mut BlipBuffer,
        right: *mut BlipBuffer,
    ) {
        for i in 0..OSC_COUNT {
            self.osc_output(i, center, left, right);
        }
    }

    pub fn output_mono(&mut self, buf: *mut BlipBuffer) {
        self.output(buf, buf, buf);
    }

    pub fn reinit_pointers(&mut self) {
        let synth_ptr = &self.square_synth as *const BlipSynth;
        for sq in &mut self.squares {
            sq.synth = synth_ptr;
        }
    }

    pub fn osc_output(
        &mut self,
        index: usize,
        center: *mut BlipBuffer,
        left: *mut BlipBuffer,
        right: *mut BlipBuffer,
    ) {
        if index < 3 {
            self.squares[index].base.outputs[1] = right;
            self.squares[index].base.outputs[2] = left;
            self.squares[index].base.outputs[3] = center;
            self.squares[index].base.output =
                self.squares[index].base.outputs[self.squares[index].base.output_select as usize];
        } else if index == 3 {
            self.noise.base.outputs[1] = right;
            self.noise.base.outputs[2] = left;
            self.noise.base.outputs[3] = center;
            self.noise.base.output =
                self.noise.base.outputs[self.noise.base.output_select as usize];
        }
    }

    pub fn reset(&mut self, feedback: u16, noise_width: i32) {
        self.last_time = 0;
        self.latch = 0;

        let mut feedback = feedback as u32;
        let mut noise_width = noise_width;
        if feedback == 0 || noise_width == 0 {
            feedback = 0x0009;
            noise_width = 16;
        }

        self.looped_feedback = 1 << (noise_width - 1);
        self.noise_feedback = 0;
        let mut fb = feedback;
        let mut w = noise_width;
        while w > 0 {
            self.noise_feedback = (self.noise_feedback << 1) | (fb & 1);
            fb >>= 1;
            w -= 1;
        }

        for sq in &mut self.squares {
            sq.reset();
        }
        self.noise.reset();
    }

    fn run_until(&mut self, end_time: BlipTime) {
        if end_time <= self.last_time {
            return;
        }

        for i in 0..3 {
            if !self.squares[i].base.output.is_null() {
                unsafe {
                    (*self.squares[i].base.output).set_modified();
                }
                self.squares[i].run(self.last_time, end_time);
            }
        }

        if !self.noise.base.output.is_null() {
            unsafe {
                (*self.noise.base.output).set_modified();
            }
            self.noise.run(self.last_time, end_time);
        }

        self.last_time = end_time;
    }

    pub fn end_frame(&mut self, end_time: BlipTime) {
        if end_time > self.last_time {
            self.run_until(end_time);
        }
        self.last_time -= end_time;
    }

    pub fn write_ggstereo(&mut self, time: BlipTime, data: i32) {
        self.run_until(time);

        for i in 0..OSC_COUNT {
            let osc: &mut SmsOsc = if i < 3 {
                &mut self.squares[i].base
            } else {
                &mut self.noise.base
            };
            let flags = data >> i;
            let old_output = osc.output;
            osc.output_select = ((flags >> 3) & 2) | (flags & 1);
            osc.output = osc.outputs[osc.output_select as usize];
            if osc.output != old_output && osc.last_amp != 0 {
                if !old_output.is_null() {
                    unsafe {
                        (*old_output).set_modified();
                        self.square_synth
                            .offset_at(time, -osc.last_amp, &mut *old_output);
                    }
                }
                osc.last_amp = 0;
            }
        }
    }

    pub fn write_data(&mut self, time: BlipTime, data: i32) {
        self.run_until(time);

        if data & 0x80 != 0 {
            self.latch = data;
        }

        let index = ((self.latch >> 5) & 3) as usize;
        if self.latch & 0x10 != 0 {
            if index < 3 {
                self.squares[index].base.volume = VOLUMES[(data & 15) as usize];
            } else {
                self.noise.base.volume = VOLUMES[(data & 15) as usize];
            }
        } else if index < 3 {
            let sq = &mut self.squares[index];
            if data & 0x80 != 0 {
                sq.period = (sq.period & 0xFF00) | ((data << 4) & 0x00FF);
            } else {
                sq.period = (sq.period & 0x00FF) | ((data << 8) & 0x3F00);
            }
        } else {
            let select = data & 3;
            if select < 3 {
                self.noise.period = unsafe { NOISE_PERIODS.as_ptr().add(select as usize) };
            } else {
                self.noise.period = &self.squares[2].period as *const i32;
            }
            self.noise.feedback = if data & 0x04 != 0 {
                self.noise_feedback
            } else {
                self.looped_feedback
            };
            self.noise.shifter = 0x8000;
        }
    }
}
