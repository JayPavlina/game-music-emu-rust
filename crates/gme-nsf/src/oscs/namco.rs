// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Nes_Namco_Apu.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use gme_core::blip_buffer::{BLIP_GOOD_QUALITY, BlipBuffer, BlipEq, BlipSynth, BlipTime};

pub const OSC_COUNT: usize = 8;
const REG_COUNT: usize = 0x80;

/// Namco 106/163 wavetable sound chip.
/// Transliterated from C++ Nes_Namco_Apu (Nes_Namco_Apu.cpp/.h).
pub struct NamcoOsc {
    pub delay: i32,
    pub output: Option<*mut BlipBuffer>,
    pub last_amp: i32,
    pub wave_pos: i32,
}

impl NamcoOsc {
    fn new() -> Self {
        Self {
            delay: 0,
            output: None,
            last_amp: 0,
            wave_pos: 0,
        }
    }
}

pub struct NesNamcoApu {
    oscs: [NamcoOsc; OSC_COUNT],
    last_time: BlipTime,
    addr_reg: i32,
    reg: [u8; REG_COUNT],
    synth: BlipSynth,
}

impl Default for NesNamcoApu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesNamcoApu {
    pub const DATA_REG_ADDR: u16 = 0x4800;
    pub const ADDR_REG_ADDR: u16 = 0xF800;

    pub fn new() -> Self {
        let mut apu = Self {
            oscs: [
                NamcoOsc::new(),
                NamcoOsc::new(),
                NamcoOsc::new(),
                NamcoOsc::new(),
                NamcoOsc::new(),
                NamcoOsc::new(),
                NamcoOsc::new(),
                NamcoOsc::new(),
            ],
            last_time: 0,
            addr_reg: 0,
            reg: [0; REG_COUNT],
            synth: BlipSynth::new(BLIP_GOOD_QUALITY),
        };
        apu.volume(1.0);
        apu.reset();
        apu
    }

    pub fn reset(&mut self) {
        self.last_time = 0;
        self.addr_reg = 0;
        self.reg = [0; REG_COUNT];
        for osc in &mut self.oscs {
            osc.delay = 0;
            osc.last_amp = 0;
            osc.wave_pos = 0;
        }
    }

    // C++ Blip_Synth<blip_good_quality,15>
    pub fn volume(&mut self, v: f64) {
        self.synth.volume(0.10 / OSC_COUNT as f64 * v, 15);
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synth.treble_eq(eq);
    }

    pub fn osc_output(&mut self, i: usize, buf: Option<*mut BlipBuffer>) {
        self.oscs[i].output = buf;
    }

    pub fn set_output(&mut self, buf: *mut BlipBuffer) {
        let buf = if buf.is_null() { None } else { Some(buf) };
        for i in 0..OSC_COUNT {
            self.osc_output(i, buf);
        }
    }

    fn access(&mut self) -> &mut u8 {
        let addr = (self.addr_reg & 0x7F) as usize;
        if self.addr_reg & 0x80 != 0 {
            self.addr_reg = ((addr as i32 + 1) & 0x7F) | 0x80;
        }
        &mut self.reg[addr]
    }

    pub fn write_addr(&mut self, v: u8) {
        self.addr_reg = v as i32;
    }

    pub fn read_data(&mut self) -> u8 {
        *self.access()
    }

    pub fn write_data(&mut self, time: BlipTime, data: u8) {
        self.run_until(time);
        *self.access() = data;
    }

    pub fn end_frame(&mut self, time: BlipTime) {
        if time > self.last_time {
            self.run_until(time);
        }
        debug_assert!(self.last_time >= time);
        self.last_time -= time;
    }

    fn run_until(&mut self, nes_end_time: BlipTime) {
        let active_oscs = ((self.reg[0x7F] as i32 >> 4) & 7) + 1;
        for i in (OSC_COUNT - active_oscs as usize)..OSC_COUNT {
            let Some(output_ptr) = self.oscs[i].output else {
                continue;
            };
            let output = unsafe { &mut *output_ptr };
            output.set_modified();

            let mut time = output
                .resampled_time(self.last_time)
                .wrapping_add(self.oscs[i].delay as u32);
            let end_time = output.resampled_time(nes_end_time);
            self.oscs[i].delay = 0;
            if time < end_time {
                let osc_reg_base = i * 8 + 0x40;

                if self.reg[osc_reg_base + 4] & 0xE0 == 0 {
                    continue;
                }

                let volume = (self.reg[osc_reg_base + 7] & 15) as i32;
                if volume == 0 {
                    continue;
                }

                let freq: i32 = (self.reg[osc_reg_base + 4] as i32 & 3) * 0x10000
                    + self.reg[osc_reg_base + 2] as i32 * 0x100
                    + self.reg[osc_reg_base] as i32;
                if freq < 64 * active_oscs {
                    continue; // prevent low frequencies from excessively delaying freq changes
                }
                let period = output.resampled_duration(983040) / freq as u32 * active_oscs as u32;

                let wave_size = 32 - ((self.reg[osc_reg_base + 4] as i32 >> 2) & 7) * 4;
                if wave_size == 0 {
                    continue;
                }

                let mut last_amp = self.oscs[i].last_amp;
                let mut wave_pos = self.oscs[i].wave_pos;

                loop {
                    // read wave sample (mask keeps the C++ OOB-read case in range)
                    let addr = wave_pos + self.reg[osc_reg_base + 6] as i32;
                    let sample = self.reg[(addr >> 1) as usize & 0x7F] as i32 >> ((addr << 2) & 4);
                    wave_pos += 1;
                    let sample = (sample & 15) * volume;

                    // output impulse if amplitude changed
                    let delta = sample - last_amp;
                    if delta != 0 {
                        last_amp = sample;
                        self.synth.offset_resampled(time, delta, output);
                    }

                    // next sample
                    time = time.wrapping_add(period);
                    if wave_pos >= wave_size {
                        wave_pos = 0;
                    }

                    if time >= end_time {
                        break;
                    }
                }

                self.oscs[i].wave_pos = wave_pos;
                self.oscs[i].last_amp = last_amp;
            }
            self.oscs[i].delay = time.wrapping_sub(end_time) as i32;
        }

        self.last_time = nes_end_time;
    }
}
