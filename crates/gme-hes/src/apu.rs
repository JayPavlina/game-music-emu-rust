// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Hes_Apu.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! Turbo Grafx 16 (PC Engine) PSG sound chip emulator.
//! Port of C++ Hes_Apu.h / Hes_Apu.cpp.

use std::ptr::null_mut;

use gme_core::blip_buffer::{BLIP_MED_QUALITY, BlipBuffer, BlipEq, BlipSynth, BlipTime};

pub const OSC_COUNT: usize = 6;
pub const START_ADDR: u16 = 0x0800;
pub const END_ADDR: u16 = 0x0809;
pub const AMP_RANGE: i32 = 0x8000;

const CENTER_WAVES: bool = true; // reduces asymmetry and clamping when starting notes

pub struct HesOsc {
    pub wave: [u8; 32],
    pub volume: [i16; 2],
    pub last_amp: [i32; 2],
    pub delay: i32,
    pub period: i32,
    pub noise: u8,
    pub phase: u8,
    pub balance: u8,
    pub dac: u8,
    pub last_time: BlipTime,

    /// Active output buffers (center or left/right); null when none.
    /// Pointers into EffectsBuffer's heap-allocated buffer list, so they
    /// stay valid when the emulator struct is moved/boxed.
    pub outputs: [*mut BlipBuffer; 2],
    pub chans: [*mut BlipBuffer; 3],
    pub noise_lfsr: u32,
    pub control: u8,
}

impl HesOsc {
    fn new() -> Self {
        Self {
            wave: [0; 32],
            volume: [0; 2],
            last_amp: [0; 2],
            delay: 0,
            period: 0,
            noise: 0,
            phase: 0,
            balance: 0,
            dac: 0,
            last_time: 0,
            outputs: [null_mut(); 2],
            chans: [null_mut(); 3],
            noise_lfsr: 1,
            control: 0x40,
        }
    }

    /// C++ memset( osc, 0, offsetof (Hes_Osc,outputs) ) + explicit fields.
    fn reset(&mut self) {
        self.wave = [0; 32];
        self.volume = [0; 2];
        self.last_amp = [0; 2];
        self.delay = 0;
        self.period = 0;
        self.noise = 0;
        self.phase = 0;
        self.balance = 0;
        self.dac = 0;
        self.last_time = 0;
        // outputs/chans preserved (after the memset boundary in C++)
        self.noise_lfsr = 1;
        self.control = 0x40;
        self.balance = 0xFF;
    }
}

/// C++ Hes_Apu::balance_changed log table, generated exactly as the C++
/// static initializer: short( factor * amp_range / 31.0 + 0.5 ).
fn log_table() -> &'static [i16; 32] {
    use std::sync::OnceLock;
    static TABLE: OnceLock<[i16; 32]> = OnceLock::new();
    TABLE.get_or_init(|| {
        // Digits verbatim from the C++ table (0.707107 is intentionally the
        // rounded constant from the source, not FRAC_1_SQRT_2).
        #[allow(clippy::approx_constant)]
        const FACTORS: [f64; 32] = [
            0.000000, 0.005524, 0.006570, 0.007813, 0.009291, 0.011049, 0.013139, 0.015625,
            0.018581, 0.022097, 0.026278, 0.031250, 0.037163, 0.044194, 0.052556, 0.062500,
            0.074325, 0.088388, 0.105112, 0.125000, 0.148651, 0.176777, 0.210224, 0.250000,
            0.297302, 0.353553, 0.420448, 0.500000, 0.594604, 0.707107, 0.840896, 1.000000,
        ];
        let mut t = [0i16; 32];
        for (i, &f) in FACTORS.iter().enumerate() {
            t[i] = (f * AMP_RANGE as f64 / 31.0 + 0.5) as i16;
        }
        t
    })
}

pub struct HesApu {
    pub oscs: [HesOsc; OSC_COUNT],
    latch: i32,
    balance: i32,
    synth: BlipSynth,
}

impl Default for HesApu {
    fn default() -> Self {
        Self::new()
    }
}

impl HesApu {
    pub fn new() -> Self {
        let mut apu = Self {
            oscs: [
                HesOsc::new(),
                HesOsc::new(),
                HesOsc::new(),
                HesOsc::new(),
                HesOsc::new(),
                HesOsc::new(),
            ],
            latch: 0,
            balance: 0xFF,
            synth: BlipSynth::new(BLIP_MED_QUALITY),
        };
        apu.reset();
        apu
    }

    pub fn reset(&mut self) {
        self.latch = 0;
        self.balance = 0xFF;
        for osc in self.oscs.iter_mut().rev() {
            osc.reset();
        }
    }

    pub fn volume(&mut self, v: f64) {
        self.synth
            .volume(1.8 / OSC_COUNT as f64 / AMP_RANGE as f64 * v, 1);
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synth.treble_eq(eq);
    }

    /// C++ Hes_Apu::osc_output.
    pub fn osc_output(
        &mut self,
        index: usize,
        center: *mut BlipBuffer,
        left: *mut BlipBuffer,
        right: *mut BlipBuffer,
    ) {
        debug_assert!(index < OSC_COUNT);
        self.oscs[index].chans[0] = center;
        self.oscs[index].chans[1] = left;
        self.oscs[index].chans[2] = right;

        let balance = self.balance;
        for i in (0..OSC_COUNT).rev() {
            Self::balance_changed(balance, &mut self.oscs[i]);
        }
    }

    /// C++ Hes_Apu::balance_changed.
    fn balance_changed(apu_balance: i32, osc: &mut HesOsc) {
        let table = log_table();

        let vol = (osc.control as i32 & 0x1F) - 0x1E * 2;

        let mut left = vol + ((osc.balance as i32) >> 3 & 0x1E) + (apu_balance >> 3 & 0x1E);
        if left < 0 {
            left = 0;
        }

        let mut right = vol + ((osc.balance as i32) << 1 & 0x1E) + (apu_balance << 1 & 0x1E);
        if right < 0 {
            right = 0;
        }

        let left = table[left as usize] as i32;
        let right = table[right as usize] as i32;

        // optimizing for the common case of being centered also allows easy
        // panning using Effects_Buffer
        osc.outputs[0] = osc.chans[0]; // center
        osc.outputs[1] = null_mut();
        if left != right {
            osc.outputs[0] = osc.chans[1]; // left
            osc.outputs[1] = osc.chans[2]; // right
        }

        if CENTER_WAVES {
            osc.last_amp[0] += (left - osc.volume[0] as i32) * 16;
            osc.last_amp[1] += (right - osc.volume[1] as i32) * 16;
        }

        osc.volume[0] = left as i16;
        osc.volume[1] = right as i16;
    }

    /// C++ Hes_Osc::run_until.
    fn osc_run_until(synth: &BlipSynth, osc: &mut HesOsc, end_time: BlipTime) {
        let out0_ptr = osc.outputs[0];
        if !out0_ptr.is_null() && osc.control & 0x80 != 0 {
            let out0 = unsafe { &mut *out0_ptr };
            let mut dac = osc.dac as i32;

            let volume_0 = osc.volume[0] as i32;
            {
                let delta = dac * volume_0 - osc.last_amp[0];
                if delta != 0 {
                    synth.offset_at(osc.last_time, delta, out0);
                }
                out0.set_modified();
            }

            let out1_ptr = osc.outputs[1];
            let volume_1 = osc.volume[1] as i32;
            if !out1_ptr.is_null() {
                let out1 = unsafe { &mut *out1_ptr };
                let delta = dac * volume_1 - osc.last_amp[1];
                if delta != 0 {
                    synth.offset_at(osc.last_time, delta, out1);
                }
                out1.set_modified();
            }

            let mut time = osc.last_time + osc.delay;
            if time < end_time {
                if osc.noise & 0x80 != 0 {
                    if (volume_0 | volume_1) != 0 {
                        // noise
                        let period = (32 - (osc.noise as i32 & 0x1F)) * 64;
                        let mut noise_lfsr = osc.noise_lfsr;
                        loop {
                            let new_dac = (0x1F & 0u32.wrapping_sub(noise_lfsr >> 1 & 1)) as i32;
                            // "Galios configuration"
                            noise_lfsr =
                                (noise_lfsr >> 1) ^ (0xE008 & 0u32.wrapping_sub(noise_lfsr & 1));
                            let delta = new_dac - dac;
                            if delta != 0 {
                                dac = new_dac;
                                synth.offset_at(time, delta * volume_0, out0);
                                if !out1_ptr.is_null() {
                                    synth.offset_at(time, delta * volume_1, unsafe {
                                        &mut *out1_ptr
                                    });
                                }
                            }
                            time += period;
                            if time >= end_time {
                                break;
                            }
                        }
                        osc.noise_lfsr = noise_lfsr;
                        debug_assert!(noise_lfsr != 0);
                    }
                } else if osc.control & 0x40 == 0 {
                    // wave
                    let mut phase = (osc.phase as i32 + 1) & 0x1F; // pre-advance
                    let mut period = osc.period * 2;
                    if period >= 14 && (volume_0 | volume_1) != 0 {
                        loop {
                            let new_dac = osc.wave[phase as usize] as i32;
                            phase = (phase + 1) & 0x1F;
                            let delta = new_dac - dac;
                            if delta != 0 {
                                dac = new_dac;
                                synth.offset_at(time, delta * volume_0, out0);
                                if !out1_ptr.is_null() {
                                    synth.offset_at(time, delta * volume_1, unsafe {
                                        &mut *out1_ptr
                                    });
                                }
                            }
                            time += period;
                            if time >= end_time {
                                break;
                            }
                        }
                    } else {
                        if period == 0 {
                            // TODO in C++: Gekisha Boy assumes period = 0 silences wave
                            period = 1;
                        }
                        // maintain phase when silent
                        let count = (end_time - time + period - 1) / period;
                        phase += count;
                        time += count * period;
                    }
                    osc.phase = ((phase - 1) & 0x1F) as u8; // undo pre-advance
                }
            }
            time -= end_time;
            if time < 0 {
                time = 0;
            }
            osc.delay = time;

            osc.dac = dac as u8;
            osc.last_amp[0] = dac * volume_0;
            osc.last_amp[1] = dac * volume_1;
        }
        osc.last_time = end_time;
    }

    /// C++ Hes_Apu::write_data.
    pub fn write_data(&mut self, time: BlipTime, addr: i32, data: i32) {
        if addr == 0x800 {
            self.latch = data & 7;
        } else if addr == 0x801 {
            if self.balance != data {
                self.balance = data;

                let balance = self.balance;
                for i in (0..OSC_COUNT).rev() {
                    Self::osc_run_until(&self.synth, &mut self.oscs[i], time);
                    // C++ quirk: balance_changed( *oscs ) operates on oscs[0]
                    // every iteration, not the loop variable.
                    Self::balance_changed(balance, &mut self.oscs[0]);
                }
            }
        } else if (self.latch as usize) < OSC_COUNT {
            let balance = self.balance;
            let latch = self.latch;
            let osc = &mut self.oscs[latch as usize];
            Self::osc_run_until(&self.synth, osc, time);
            match addr {
                0x802 => {
                    osc.period = (osc.period & 0xF00) | data;
                }
                0x803 => {
                    osc.period = (osc.period & 0x0FF) | ((data & 0x0F) << 8);
                }
                0x804 => {
                    if (osc.control as i32 & 0x40 & !data) != 0 {
                        osc.phase = 0;
                    }
                    osc.control = data as u8;
                    Self::balance_changed(balance, osc);
                }
                0x805 => {
                    osc.balance = data as u8;
                    Self::balance_changed(balance, osc);
                }
                0x806 => {
                    let data = data & 0x1F;
                    if osc.control & 0x40 == 0 {
                        osc.wave[osc.phase as usize] = data as u8;
                        osc.phase = (osc.phase + 1) & 0x1F;
                    } else if osc.control & 0x80 != 0 {
                        osc.dac = data as u8;
                    }
                }
                0x807 if latch >= 4 => {
                    osc.noise = data as u8;
                }
                _ => {
                    // 0x809: LFO (unsupported; debug print only in C++)
                }
            }
        }
    }

    /// C++ Hes_Apu::end_frame.
    pub fn end_frame(&mut self, end_time: BlipTime) {
        for i in (0..OSC_COUNT).rev() {
            let osc = &mut self.oscs[i];
            if end_time > osc.last_time {
                Self::osc_run_until(&self.synth, osc, end_time);
            }
            debug_assert!(osc.last_time >= end_time);
            osc.last_time -= end_time;
        }
    }
}
