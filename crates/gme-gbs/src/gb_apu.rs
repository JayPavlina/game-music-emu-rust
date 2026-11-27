// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Gb_Apu.cpp (2026-06-09 to 2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// Game Boy APU (Audio Processing Unit).
/// Matches C++ Gb_Apu.h/Gb_Apu.cpp.
use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipSynth, BlipTime};

/// Differential-test trace hook (mirrors the C++ GBS_TRACE define).
/// Writes APU events to /tmp/gbs_rust_apu.log when GME_GBS_TRACE is set.
#[doc(hidden)]
pub fn gbs_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("GME_GBS_TRACE").is_some())
}

#[doc(hidden)]
pub fn gbs_trace_line(args: std::fmt::Arguments) {
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    static FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
    let f =
        FILE.get_or_init(|| Mutex::new(std::fs::File::create("/tmp/gbs_rust_apu.log").unwrap()));
    let _ = writeln!(f.lock().unwrap(), "{args}");
}

macro_rules! gbs_trace {
    ($($arg:tt)*) => {
        if crate::gb_apu::gbs_trace_enabled() {
            crate::gb_apu::gbs_trace_line(format_args!($($arg)*));
        }
    };
}

use crate::gb_oscs::{GbNoise, GbSquare, GbWave, OSC_COUNT};

/// APU register range: $FF10-$FF3F (48 registers).
pub const START_ADDR: u16 = 0xFF10;
pub const END_ADDR: u16 = 0xFF3F;
pub const REGISTER_COUNT: usize = (END_ADDR - START_ADDR + 1) as usize;

/// Default sound register values written at reset.
/// Matches C++ sound_data array from Gbs_Emu.cpp.
pub const SOUND_DATA: [u8; REGISTER_COUNT] = [
    0x80, 0xBF, 0x00, 0x00, 0xBF, // square 1
    0x00, 0x3F, 0x00, 0x00, 0xBF, // square 2
    0x7F, 0xFF, 0x9F, 0x00, 0xBF, // wave
    0x00, 0xFF, 0x00, 0x00, 0xBF, // noise
    0x77, 0xF3, 0xF1, // vin/volume, status, power mode
    0, 0, 0, 0, 0, 0, 0, 0, 0, // unused
    0xAC, 0xDD, 0xDA, 0x48, 0x36, 0x02, 0xCF, 0x16, // waveform data
    0x2C, 0x04, 0xE5, 0x2C, 0xAC, 0xDD, 0xDA, 0x48,
];

const VOL_REG: u16 = 0xFF24;
const STATUS_REG: u16 = 0xFF26;

/// Register values written when the APU is powered off.
/// Matches C++ powerup_regs[0x20] from Gb_Apu.cpp.
const POWERUP_REGS: [u8; 0x20] = [
    0x80, 0x3F, 0x00, 0xFF, 0xBF, // square 1
    0xFF, 0x3F, 0x00, 0xFF, 0xBF, // square 2
    0x7F, 0xFF, 0x9F, 0xFF, 0xBF, // wave
    0xFF, 0xFF, 0x00, 0x00, 0xBF, // noise
    0x00, // left/right enables
    0x77, // master volume
    0x80, // power
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

/// Game Boy APU.
pub struct GbApu {
    pub regs: [u8; REGISTER_COUNT],
    pub square1: GbSquare,
    pub square2: GbSquare,
    pub wave: GbWave,
    pub noise: GbNoise,
    pub square_synth: BlipSynth,
    pub other_synth: BlipSynth,

    next_frame_time: BlipTime,
    last_time: BlipTime,
    frame_period: BlipTime,
    frame_count: i32,
    volume_unit: f64,
}

impl Default for GbApu {
    fn default() -> Self {
        Self::new()
    }
}

impl GbApu {
    pub fn new() -> Self {
        let mut apu = Self {
            regs: [0; REGISTER_COUNT],
            square1: GbSquare::new(),
            square2: GbSquare::new(),
            wave: GbWave::new(),
            noise: GbNoise::new(),
            square_synth: BlipSynth::new(gme_core::blip_buffer::BLIP_GOOD_QUALITY),
            other_synth: BlipSynth::new(gme_core::blip_buffer::BLIP_MED_QUALITY),
            next_frame_time: 0,
            last_time: 0,
            frame_period: 0,
            frame_count: 0,
            volume_unit: 0.0,
        };

        // Set up synth pointers
        apu.square1.synth = &apu.square_synth as *const BlipSynth;
        apu.square2.synth = &apu.square_synth as *const BlipSynth;
        apu.wave.synth = &apu.other_synth as *const BlipSynth;
        apu.noise.synth = &apu.other_synth as *const BlipSynth;

        // Set up register pointers
        let regs_ptr = apu.regs.as_mut_ptr();
        apu.square1.env.osc.regs = regs_ptr;
        apu.square2.env.osc.regs = unsafe { regs_ptr.add(5) };
        apu.wave.osc.regs = unsafe { regs_ptr.add(10) };
        apu.noise.env.osc.regs = unsafe { regs_ptr.add(15) };

        apu.set_tempo(1.0);
        apu.volume(1.0);
        apu.reset();
        apu
    }

    /// Re-initialize oscillator register pointers.
    ///
    /// Must be called after the APU is moved to its final location, because
    /// the oscillators hold raw pointers into `self.regs`.
    pub fn reinit_osc_pointers(&mut self) {
        let regs_ptr = self.regs.as_mut_ptr();
        self.square1.env.osc.regs = regs_ptr;
        self.square2.env.osc.regs = unsafe { regs_ptr.add(5) };
        self.wave.osc.regs = unsafe { regs_ptr.add(10) };
        self.noise.env.osc.regs = unsafe { regs_ptr.add(15) };

        // Re-init synth pointers (oscillators hold raw pointers to synth instances)
        let sq_synth = &self.square_synth as *const BlipSynth;
        let other_synth = &self.other_synth as *const BlipSynth;
        self.square1.synth = sq_synth;
        self.square2.synth = sq_synth;
        self.wave.synth = other_synth;
        self.noise.synth = other_synth;
    }

    /// Set overall volume (1.0 = full).
    pub fn volume(&mut self, vol: f64) {
        self.volume_unit = 0.60 / OSC_COUNT as f64 / 15.0 / 2.0 / 8.0 * vol;
        self.update_volume();
    }

    /// Set treble equalization.
    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.square_synth.treble_eq(eq);
        self.other_synth.treble_eq(eq);
    }

    pub fn volume_unit_value(&self) -> f64 {
        self.volume_unit
    }

    pub fn frame_period_value(&self) -> BlipTime {
        self.frame_period
    }

    pub fn square_delta_factor(&self) -> i32 {
        self.square_synth.delta_factor
    }

    pub fn other_delta_factor(&self) -> i32 {
        self.other_synth.delta_factor
    }

    /// Wire all oscillator outputs to the given buffers.
    pub fn output(
        &mut self,
        center: Option<*mut BlipBuffer>,
        left: Option<*mut BlipBuffer>,
        right: Option<*mut BlipBuffer>,
    ) {
        for i in 0..OSC_COUNT {
            self.osc_output(i, center, left, right);
        }
    }

    /// Wire a single oscillator output.
    /// Matches C++ Gb_Apu::osc_output(index, center, left, right).
    pub fn osc_output(
        &mut self,
        index: usize,
        center: Option<*mut BlipBuffer>,
        left: Option<*mut BlipBuffer>,
        right: Option<*mut BlipBuffer>,
    ) {
        let osc = match index {
            0 => &mut self.square1.env.osc,
            1 => &mut self.square2.env.osc,
            2 => &mut self.wave.osc,
            3 => &mut self.noise.env.osc,
            _ => return,
        };
        osc.outputs[1] = right;
        osc.outputs[2] = left;
        osc.outputs[3] = center;
        osc.output = osc.outputs[osc.output_select];
    }

    /// Reset APU state.
    pub fn reset(&mut self) {
        self.next_frame_time = 0;
        self.last_time = 0;
        self.frame_count = 0;

        self.square1.reset();
        self.square2.reset();
        self.wave.reset();
        self.noise.reset();
        self.noise.bits = 1;
        self.wave.wave_pos = 0;

        // Avoid click at beginning
        self.regs[(VOL_REG - START_ADDR) as usize] = 0x77;
        self.update_volume();

        self.regs[(STATUS_REG - START_ADDR) as usize] = 0x01; // force power
        self.write_register(0, STATUS_REG, 0x00);

        // Initial wave table. C++ memcpy's the PACKED bytes into the first
        // 16 entries of wave.wave (it does not unpack nibbles), leaving the
        // remaining 16 entries untouched.
        let initial_wave: [u8; 16] = [
            0x84, 0x40, 0x43, 0xAA, 0x2D, 0x78, 0x92, 0x3C, 0x60, 0x59, 0x59, 0xB0, 0x34, 0xB8,
            0x2E, 0xDA,
        ];
        self.wave.wave[..16].copy_from_slice(&initial_wave);
    }

    /// Set tempo (1.0 = normal).
    pub fn set_tempo(&mut self, t: f64) {
        self.frame_period = 4194304 / 256; // 256 Hz
        if t != 1.0 {
            self.frame_period = (self.frame_period as f64 / t) as i32;
        }
    }

    /// Write to an APU register.
    pub fn write_register(&mut self, time: BlipTime, addr: u16, data: u8) {
        let reg = (addr - START_ADDR) as usize;
        if reg >= REGISTER_COUNT {
            return;
        }

        gbs_trace!("W {} {:04X} {:02X}", time, addr, data);

        self.run_until(time);

        let old_reg = self.regs[reg];
        self.regs[reg] = data;

        if addr < VOL_REG {
            // Oscillator register write
            self.write_osc(reg / 5, reg, data);
        } else if addr == VOL_REG && data != old_reg {
            // Global volume change
            // Return all oscs to 0
            for i in 0..OSC_COUNT {
                let osc = match i {
                    0 => &mut self.square1.env.osc as &mut crate::gb_oscs::GbOsc,
                    1 => &mut self.square2.env.osc,
                    2 => &mut self.wave.osc,
                    3 => &mut self.noise.env.osc,
                    _ => unreachable!(),
                };
                let amp = osc.last_amp;
                osc.last_amp = 0;
                if amp != 0
                    && osc.enabled
                    && let Some(out) = osc.output
                {
                    unsafe {
                        self.other_synth.offset_resampled(
                            (*out).resampled_time(time),
                            -amp,
                            &mut *out,
                        );
                    }
                }
            }

            // Add DC offset for wave channel
            if self.wave.osc.outputs[3].is_some() {
                let out = self.wave.osc.outputs[3].unwrap();
                unsafe {
                    self.other_synth
                        .offset_resampled((*out).resampled_time(time), 30, &mut *out);
                }
            }

            self.update_volume();

            if self.wave.osc.outputs[3].is_some() {
                let out = self.wave.osc.outputs[3].unwrap();
                unsafe {
                    self.other_synth
                        .offset_resampled((*out).resampled_time(time), -30, &mut *out);
                }
            }
        } else if addr == 0xFF25 || addr == STATUS_REG {
            // Status / panning
            let mask = if self.regs[(STATUS_REG - START_ADDR) as usize] & 0x80 != 0 {
                0xFFu8
            } else {
                0
            };
            let flags = self.regs[(0xFF25 - START_ADDR) as usize] & mask;

            for i in 0..OSC_COUNT {
                let osc = match i {
                    0 => &mut self.square1.env.osc as &mut crate::gb_oscs::GbOsc,
                    1 => &mut self.square2.env.osc,
                    2 => &mut self.wave.osc,
                    3 => &mut self.noise.env.osc,
                    _ => unreachable!(),
                };
                osc.enabled &= mask != 0;
                let bits = flags >> i;
                let old_output = osc.output;
                osc.output_select = ((bits >> 3) & 2) as usize | (bits & 1) as usize;
                osc.output = osc.outputs[osc.output_select];
                if osc.output != old_output {
                    let amp = osc.last_amp;
                    osc.last_amp = 0;
                    if amp != 0
                        && let Some(out) = old_output
                    {
                        unsafe {
                            self.other_synth.offset_resampled(
                                (*out).resampled_time(time),
                                -amp,
                                &mut *out,
                            );
                        }
                    }
                }
            }

            // Power on/off
            if addr == STATUS_REG && data != old_reg && data & 0x80 == 0 {
                // Power off — write powerup regs (C++ writes exactly
                // sizeof powerup_regs == 0x20 registers, skipping status)
                for (i, &value) in POWERUP_REGS.iter().enumerate() {
                    if i != (STATUS_REG - START_ADDR) as usize {
                        self.write_register(time, i as u16 + START_ADDR, value);
                    }
                }
            }
        } else if addr >= 0xFF30 {
            // Wave table write
            let index = (addr & 0x0F) as usize * 2;
            self.wave.wave[index] = data >> 4;
            self.wave.wave[index + 1] = data & 0x0F;
        }
    }

    /// Read from an APU register.
    pub fn read_register(&mut self, time: BlipTime, addr: u16) -> i32 {
        self.run_until(time);

        let index = (addr - START_ADDR) as usize;
        if index >= REGISTER_COUNT {
            return 0xFF;
        }

        let mut data = self.regs[index] as i32;

        if addr == STATUS_REG {
            data = (data & 0x80) | 0x70;
            for i in 0..OSC_COUNT {
                let osc = match i {
                    0 => &self.square1.env.osc as &crate::gb_oscs::GbOsc,
                    1 => &self.square2.env.osc,
                    2 => &self.wave.osc,
                    3 => &self.noise.env.osc,
                    _ => unreachable!(),
                };
                if osc.enabled && (osc.length > 0 || (osc.read_reg(4) & 0x40 == 0)) {
                    data |= 1 << i;
                }
            }
        }

        data
    }

    /// End the current time frame.
    pub fn end_frame(&mut self, end_time: BlipTime) {
        gbs_trace!("F {}", end_time);
        if end_time > self.last_time {
            self.run_until(end_time);
        }

        debug_assert!(self.next_frame_time >= end_time);
        self.next_frame_time -= end_time;

        debug_assert!(self.last_time >= end_time);
        self.last_time -= end_time;
    }

    /// Run oscillators and frame sequencer until end_time.
    fn run_until(&mut self, end_time: BlipTime) {
        debug_assert!(end_time >= self.last_time);
        if end_time == self.last_time {
            return;
        }

        loop {
            let time = if self.next_frame_time > end_time {
                end_time
            } else {
                self.next_frame_time
            };

            // Run oscillators. C++ skips an oscillator ENTIRELY (including
            // its state updates) when its output buffer is null.
            if let Some(out) = self.square1.env.osc.output {
                unsafe {
                    (*out).set_modified();
                }
                let playing = self.square1.env.osc.is_playing();
                {
                    let o = &self.square1.env.osc;
                    gbs_trace!(
                        "R 0 {} {} {} {} {} {} {} {} {}",
                        self.last_time,
                        time,
                        playing as i32,
                        o.volume,
                        o.last_amp,
                        o.delay,
                        o.length,
                        o.enabled as i32,
                        self.square1.phase + self.square1.sweep_freq * 16
                    );
                }
                self.square1.run(self.last_time, time, playing);
            }
            if let Some(out) = self.square2.env.osc.output {
                unsafe {
                    (*out).set_modified();
                }
                let playing = self.square2.env.osc.is_playing();
                {
                    let o = &self.square2.env.osc;
                    gbs_trace!(
                        "R 1 {} {} {} {} {} {} {} {} {}",
                        self.last_time,
                        time,
                        playing as i32,
                        o.volume,
                        o.last_amp,
                        o.delay,
                        o.length,
                        o.enabled as i32,
                        self.square2.phase
                    );
                }
                self.square2.run(self.last_time, time, playing);
            }
            if let Some(out) = self.wave.osc.output {
                unsafe {
                    (*out).set_modified();
                }
                let playing = self.wave.osc.is_playing();
                {
                    let o = &self.wave.osc;
                    gbs_trace!(
                        "R 2 {} {} {} {} {} {} {} {} {}",
                        self.last_time,
                        time,
                        playing as i32,
                        o.volume,
                        o.last_amp,
                        o.delay,
                        o.length,
                        o.enabled as i32,
                        self.wave.wave_pos
                    );
                }
                self.wave.run(self.last_time, time, playing);
            }
            if let Some(out) = self.noise.env.osc.output {
                unsafe {
                    (*out).set_modified();
                }
                let playing = self.noise.env.osc.is_playing();
                {
                    let o = &self.noise.env.osc;
                    gbs_trace!(
                        "R 3 {} {} {} {} {} {} {} {} {}",
                        self.last_time,
                        time,
                        playing as i32,
                        o.volume,
                        o.last_amp,
                        o.delay,
                        o.length,
                        o.enabled as i32,
                        self.noise.bits as i32
                    );
                }
                self.noise.run(self.last_time, time, playing);
            }

            self.last_time = time;

            if time == end_time {
                break;
            }

            self.next_frame_time += self.frame_period;

            // 256 Hz: length counters
            self.square1.env.osc.clock_length();
            self.square2.env.osc.clock_length();
            self.wave.osc.clock_length();
            self.noise.env.osc.clock_length();

            self.frame_count = (self.frame_count + 1) & 3;
            if self.frame_count == 0 {
                // 64 Hz: envelope
                self.square1.env.clock_envelope();
                self.square2.env.clock_envelope();
                self.noise.env.clock_envelope();
            }

            if self.frame_count & 1 != 0 {
                // 128 Hz: sweep
                self.square1.clock_sweep();
            }
        }
    }

    fn update_volume(&mut self) {
        let data = self.regs[(VOL_REG - START_ADDR) as usize];
        let vol = (std::cmp::max(data & 7, (data >> 4) & 7) as f64 + 1.0) * self.volume_unit;
        self.square_synth.volume(vol, 1);
        self.other_synth.volume(vol, 1);
    }

    fn write_osc(&mut self, index: usize, reg: usize, data: u8) {
        let osc_reg = reg - index * 5;
        match index {
            0 => {
                let sq = &mut self.square1;
                if sq.env.write_register(osc_reg, data) && index == 0 {
                    sq.sweep_freq = sq.env.osc.frequency();
                    if (self.regs[0] & GbSquare::PERIOD_MASK != 0)
                        && (self.regs[0] & GbSquare::SHIFT_MASK != 0)
                    {
                        sq.sweep_delay = 1;
                        sq.clock_sweep();
                    }
                }
            }
            1 => {
                self.square2.env.write_register(osc_reg, data);
            }
            2 => {
                self.wave.write_register(osc_reg, data);
            }
            3 if self.noise.env.write_register(osc_reg, data) => {
                self.noise.bits = 0x7FFF;
            }
            _ => {}
        }
    }
}
