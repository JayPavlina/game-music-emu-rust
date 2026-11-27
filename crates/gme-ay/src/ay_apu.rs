// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Ay_Apu.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// AY-3-8910 sound chip emulator.
/// Port of C++ Ay_Apu.h / Ay_Apu.cpp.
use gme_core::blip_buffer::{BLIP_GOOD_QUALITY, BlipBuffer, BlipEq, BlipSynth, BlipTime};

pub const REG_COUNT: usize = 16;
pub const OSC_COUNT: usize = 3;
pub const AMP_RANGE: i32 = 255;
const PERIOD_FACTOR: BlipTime = 16;
const INAUDIBLE_FREQ: u32 = 16384;

// ENTRY( n ) = byte (n * amp_range + 0.5), truncating like C++ float->byte
const AMP_TABLE: [u8; 16] = [0, 2, 3, 4, 6, 8, 11, 16, 23, 32, 45, 64, 90, 128, 180, 255];

// MODE( a0,a1, b0,b1, c0,c1 ) = a0 | a1<<1 | b0<<2 | b1<<3 | c0<<4 | c1<<5
const MODES: [u8; 8] = [
    0x15, // MODE( 1,0, 1,0, 1,0 )
    0x01, // MODE( 1,0, 0,0, 0,0 )
    0x19, // MODE( 1,0, 0,1, 1,0 )
    0x3D, // MODE( 1,0, 1,1, 1,1 )
    0x2A, // MODE( 0,1, 0,1, 0,1 )
    0x3E, // MODE( 0,1, 1,1, 1,1 )
    0x26, // MODE( 0,1, 1,0, 0,1 )
    0x02, // MODE( 0,1, 0,0, 0,0 )
];

const NOISE_OFF: i32 = 0x08;
const TONE_OFF: i32 = 0x01;

struct Osc {
    period: BlipTime,
    delay: BlipTime,
    last_amp: i16,
    phase: i16,
    output: *mut BlipBuffer,
}

impl Osc {
    fn new() -> Self {
        Self {
            period: PERIOD_FACTOR,
            delay: 0,
            last_amp: 0,
            phase: 0,
            output: std::ptr::null_mut(),
        }
    }
}

struct Noise {
    delay: BlipTime,
    lfsr: u32,
}

struct Envelope {
    delay: BlipTime,
    /// Flat offset into `modes` of the row pointed to by C++ `env.wave`.
    /// C++ indexes this pointer with negative positions (-48..-1), so the
    /// effective data read is `modes[wave_offset + pos]`.
    wave_offset: i32,
    pos: i32,
    /// Flat equivalent of C++ `byte modes [8] [48]`.
    modes: [u8; 8 * 48],
}

pub struct AyApu {
    oscs: [Osc; OSC_COUNT],
    last_time: BlipTime,
    regs: [u8; REG_COUNT],
    noise: Noise,
    env: Envelope,
    pub synth: BlipSynth,
}

// SAFETY: Raw pointers only point to data owned by the emulator struct.
unsafe impl Send for AyApu {}

impl Default for AyApu {
    fn default() -> Self {
        Self::new()
    }
}

impl AyApu {
    pub fn new() -> Self {
        let mut apu = Self {
            oscs: [Osc::new(), Osc::new(), Osc::new()],
            last_time: 0,
            regs: [0; REG_COUNT],
            noise: Noise { delay: 0, lfsr: 1 },
            env: Envelope {
                delay: 0,
                wave_offset: 0,
                pos: 0,
                modes: [0u8; 8 * 48],
            },
            synth: BlipSynth::new(BLIP_GOOD_QUALITY),
        };

        // Build full table of the upper 8 envelope waveforms (1:1 with C++ ctor)
        for (m, &mode_flags) in MODES.iter().enumerate() {
            let mut out = m * 48;
            let mut flags = mode_flags as i32;
            for _x in 0..3 {
                let mut amp = flags & 1;
                let end = (flags >> 1) & 1;
                let step = end - amp;
                amp *= 15;
                for _y in 0..16 {
                    apu.env.modes[out] = AMP_TABLE[amp as usize];
                    out += 1;
                    amp += step;
                }
                flags >>= 2;
            }
        }

        apu.output(std::ptr::null_mut());
        apu.volume(1.0);
        apu.reset();
        apu
    }

    pub fn reset(&mut self) {
        self.last_time = 0;
        self.noise.delay = 0;
        self.noise.lfsr = 1;

        for osc in &mut self.oscs {
            osc.period = PERIOD_FACTOR;
            osc.delay = 0;
            osc.last_amp = 0;
            osc.phase = 0;
        }

        self.regs.fill(0);
        self.regs[7] = 0xFF;
        self.write_data(13, 0);
    }

    pub fn output(&mut self, buf: *mut BlipBuffer) {
        for osc in &mut self.oscs {
            osc.output = buf;
        }
    }

    pub fn osc_output(&mut self, i: usize, buf: *mut BlipBuffer) {
        if i < OSC_COUNT {
            self.oscs[i].output = buf;
        }
    }

    pub fn volume(&mut self, v: f64) {
        self.synth
            .volume(0.7 / OSC_COUNT as f64 / AMP_RANGE as f64 * v, 1);
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synth.treble_eq(eq);
    }

    pub fn write(&mut self, time: BlipTime, addr: usize, data: u8) {
        self.run_until(time);
        self.write_data(addr, data);
    }

    pub fn end_frame(&mut self, time: BlipTime) {
        if time > self.last_time {
            self.run_until(time);
        }
        debug_assert!(self.last_time >= time);
        self.last_time -= time;
    }

    /// Read the envelope wave like C++ `env.wave [pos]` where `pos` is in
    /// -48..-1 and `env.wave` points at `modes [row]`.
    #[inline]
    fn env_wave(&self, pos: i32) -> u8 {
        let idx = self.env.wave_offset + pos;
        // For row 0 (envelope shape 15 in the vendored C++), the C++ code reads
        // out of bounds before the modes array (undefined garbage). We return 0.
        if idx >= 0 {
            self.env.modes[idx as usize]
        } else {
            0
        }
    }

    fn write_data(&mut self, addr: usize, data: u8) {
        debug_assert!(addr < REG_COUNT);
        let mut data = data as i32;

        // envelope mode
        if addr == 13 {
            if data & 8 == 0 {
                // convert modes 0-7 to proper equivalents
                data = if data & 4 != 0 { 15 } else { 9 };
            }
            self.env.wave_offset = ((data - 7) & 0x07) * 48;
            self.env.pos = -48;
            self.env.delay = 0; // will get set to envelope period in run_until()
        }
        self.regs[addr] = data as u8;

        // handle period changes accurately
        let i = addr >> 1;
        if i < OSC_COUNT {
            let mut period = (self.regs[i * 2 + 1] as BlipTime & 0x0F) * (0x100 * PERIOD_FACTOR)
                + self.regs[i * 2] as BlipTime * PERIOD_FACTOR;
            if period == 0 {
                period = PERIOD_FACTOR;
            }

            // adjust time of next timer expiration based on change in period
            let osc = &mut self.oscs[i];
            osc.delay += period - osc.period;
            if osc.delay < 0 {
                osc.delay = 0;
            }
            osc.period = period;
        }
    }

    fn run_until(&mut self, final_end_time: BlipTime) {
        debug_assert!(final_end_time >= self.last_time);

        // noise period and initial values
        let noise_period_factor = PERIOD_FACTOR * 2; // verified
        let mut noise_period = (self.regs[6] as BlipTime & 0x1F) * noise_period_factor;
        if noise_period == 0 {
            noise_period = noise_period_factor;
        }
        let old_noise_delay = self.noise.delay;
        let old_noise_lfsr = self.noise.lfsr;

        // envelope period
        let env_period_factor = PERIOD_FACTOR * 2; // verified
        let mut env_period =
            (self.regs[12] as BlipTime * 0x100 + self.regs[11] as BlipTime) * env_period_factor;
        if env_period == 0 {
            env_period = env_period_factor; // same as period 1 on my AY chip
        }
        if self.env.delay == 0 {
            self.env.delay = env_period;
        }

        // run each osc separately
        for index in 0..OSC_COUNT {
            let mut osc_mode: i32 = (self.regs[7] >> index) as i32;

            // output
            let osc_output = self.oscs[index].output;
            if osc_output.is_null() {
                continue;
            }
            let out = unsafe { &mut *osc_output };
            out.set_modified();

            // period
            let mut half_vol = 0;
            let inaudible_period = (out.clock_rate_value() + INAUDIBLE_FREQ) / (INAUDIBLE_FREQ * 2);
            if self.oscs[index].period <= inaudible_period as BlipTime && (osc_mode & TONE_OFF) == 0
            {
                half_vol = 1; // Actually around 60%, but 50% is close enough
                osc_mode |= TONE_OFF;
            }

            // envelope
            let mut start_time = self.last_time;
            let mut end_time = final_end_time;
            let vol_mode = self.regs[0x08 + index] as i32;
            let mut volume = (AMP_TABLE[(vol_mode & 0x0F) as usize] as i32) >> half_vol;
            let mut osc_env_pos = self.env.pos;
            if vol_mode & 0x10 != 0 {
                volume = (self.env_wave(osc_env_pos) as i32) >> half_vol;
                // use envelope only if it's a repeating wave or a ramp that hasn't finished
                if (self.regs[13] & 1) == 0 || osc_env_pos < -32 {
                    end_time = start_time + self.env.delay;
                    if end_time >= final_end_time {
                        end_time = final_end_time;
                    }
                } else if volume == 0 {
                    osc_mode = NOISE_OFF | TONE_OFF;
                }
            } else if volume == 0 {
                osc_mode = NOISE_OFF | TONE_OFF;
            }

            // tone time
            let period = self.oscs[index].period;
            let mut time = start_time + self.oscs[index].delay;
            if osc_mode & TONE_OFF != 0 {
                // maintain tone's phase when off
                let count = (final_end_time - time + period - 1) / period;
                time += count * period;
                self.oscs[index].phase ^= (count & 1) as i16;
            }

            // noise time
            let mut ntime = final_end_time;
            let mut noise_lfsr: u32 = 1;
            if (osc_mode & NOISE_OFF) == 0 {
                ntime = start_time + old_noise_delay;
                noise_lfsr = old_noise_lfsr;
            }

            // This loop only runs one iteration if envelope is disabled.
            loop {
                // current amplitude
                let mut amp = 0;
                if ((osc_mode | self.oscs[index].phase as i32)
                    & 1
                    & ((osc_mode >> 3) | noise_lfsr as i32))
                    != 0
                {
                    amp = volume;
                }
                {
                    let delta = amp - self.oscs[index].last_amp as i32;
                    if delta != 0 {
                        self.oscs[index].last_amp = amp as i16;
                        self.synth.offset_at(start_time, delta, out);
                    }
                }

                // Run wave and noise interleved with each catching up to the other.
                if ntime < end_time || time < end_time {
                    // Since amplitude was updated above, delta will always be +/- volume.
                    let mut delta = amp * 2 - volume;
                    let delta_non_zero = (delta != 0) as i32;
                    let mut phase = self.oscs[index].phase as i32 | (osc_mode & TONE_OFF);

                    loop {
                        // run noise
                        let mut end = end_time;
                        if end_time > time {
                            end = time;
                        }
                        if (phase & delta_non_zero) != 0 {
                            while ntime <= end {
                                // must advance *past* time to avoid hang
                                let changed = noise_lfsr.wrapping_add(1);
                                noise_lfsr =
                                    ((noise_lfsr & 1).wrapping_neg() & 0x12000) ^ (noise_lfsr >> 1);
                                if changed & 2 != 0 {
                                    delta = -delta;
                                    self.synth.offset_at(ntime, delta, out);
                                }
                                ntime += noise_period;
                            }
                        } else {
                            // 20 or more noise periods on average for some music
                            let remain = end - ntime;
                            let count = remain / noise_period;
                            if remain >= 0 {
                                ntime += noise_period + count * noise_period;
                            }
                        }

                        // run tone
                        end = end_time;
                        if end_time > ntime {
                            end = ntime;
                        }
                        if (noise_lfsr as i32 & delta_non_zero) != 0 {
                            while time < end {
                                delta = -delta;
                                self.synth.offset_at(time, delta, out);
                                time += period;
                            }
                            phase = ((-delta) as u32 >> 31) as i32; // (delta > 0)
                        } else {
                            // loop usually runs less than once
                            while time < end {
                                time += period;
                                phase ^= 1;
                            }
                        }

                        if !(time < end_time || ntime < end_time) {
                            break;
                        }
                    }

                    self.oscs[index].last_amp = ((delta + volume) >> 1) as i16;
                    if (osc_mode & TONE_OFF) == 0 {
                        self.oscs[index].phase = phase as i16;
                    }
                }

                if end_time >= final_end_time {
                    break; // breaks first time when envelope is disabled
                }

                // next envelope step
                osc_env_pos += 1;
                if osc_env_pos >= 0 {
                    osc_env_pos -= 32;
                }
                volume = (self.env_wave(osc_env_pos) as i32) >> half_vol;

                start_time = end_time;
                end_time += env_period;
                if end_time > final_end_time {
                    end_time = final_end_time;
                }
            }
            self.oscs[index].delay = time - final_end_time;

            if (osc_mode & NOISE_OFF) == 0 {
                self.noise.delay = ntime - final_end_time;
                self.noise.lfsr = noise_lfsr;
            }
        }

        // maintain envelope phase
        let mut remain = final_end_time - self.last_time - self.env.delay;
        if remain >= 0 {
            let count = (remain + env_period) / env_period;
            self.env.pos += count;
            if self.env.pos >= 0 {
                self.env.pos = (self.env.pos & 31) - 32;
            }
            remain -= count * env_period;
            debug_assert!(-remain <= env_period);
        }
        self.env.delay = -remain;
        debug_assert!(self.env.delay > 0);
        debug_assert!(self.env.pos < 0);

        self.last_time = final_end_time;
    }
}
