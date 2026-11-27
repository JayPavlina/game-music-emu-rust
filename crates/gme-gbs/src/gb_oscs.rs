/// Game Boy oscillators used by GbApu.
/// Matches C++ Gb_Oscs.h/Gb_Oscs.cpp.
use gme_core::blip_buffer::{BlipBuffer, BlipSynth, BlipTime};

/// Output index constants (matching C++ output_select values).
/// outputs[0] = NULL, outputs[1] = right, outputs[2] = left, outputs[3] = center.
pub const OSC_COUNT: usize = 4;

/// Base oscillator state.
pub struct GbOsc {
    pub output: Option<*mut BlipBuffer>,
    pub outputs: [Option<*mut BlipBuffer>; 4],
    pub output_select: usize,
    /// Pointer into APU register array (5 registers per oscillator).
    pub regs: *mut u8,

    pub delay: i32,
    pub last_amp: i32,
    pub volume: i32,
    pub length: i32,
    pub enabled: bool,
}

impl Default for GbOsc {
    fn default() -> Self {
        Self::new()
    }
}

impl GbOsc {
    pub fn new() -> Self {
        Self {
            output: None,
            outputs: [None; 4],
            output_select: 3,
            regs: std::ptr::null_mut(),
            delay: 0,
            last_amp: 0,
            volume: 0,
            length: 0,
            enabled: false,
        }
    }

    pub fn reset(&mut self) {
        self.delay = 0;
        self.last_amp = 0;
        self.length = 0;
        self.output_select = 3;
        self.output = self.outputs[self.output_select];
    }

    /// Read a register from this oscillator's register block.
    pub fn read_reg(&self, idx: usize) -> u8 {
        debug_assert!(idx < 5);
        unsafe { *self.regs.add(idx) }
    }

    /// Write a register in this oscillator's register block.
    pub fn write_reg(&mut self, idx: usize, val: u8) {
        debug_assert!(idx < 5);
        unsafe {
            *self.regs.add(idx) = val;
        }
    }

    /// Frequency from regs[3] and regs[4].
    pub fn frequency(&self) -> i32 {
        ((self.read_reg(4) as i32 & 7) << 8) | self.read_reg(3) as i32
    }

    /// Clock the length counter.
    pub fn clock_length(&mut self) {
        if (self.read_reg(4) & 0x40 != 0) && self.length > 0 {
            self.length -= 1;
        }
    }

    /// Check if oscillator should be playing.
    pub fn is_playing(&self) -> bool {
        self.enabled && self.volume != 0 && ((self.read_reg(4) & 0x40 == 0) || self.length > 0)
    }
}

pub const TRIGGER: u8 = 0x80;
pub const LEN_ENABLED_MASK: u8 = 0x40;

/// Envelope oscillator (extends GbOsc).
pub struct GbEnv {
    pub osc: GbOsc,
    pub env_delay: i32,
}

impl Default for GbEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl GbEnv {
    pub fn new() -> Self {
        Self {
            osc: GbOsc::new(),
            env_delay: 0,
        }
    }

    pub fn reset(&mut self) {
        self.env_delay = 0;
        self.osc.reset();
    }

    /// Clock the envelope.
    pub fn clock_envelope(&mut self) {
        if self.env_delay > 0 {
            self.env_delay -= 1;
            if self.env_delay == 0 {
                self.env_delay = (self.osc.read_reg(2) & 7) as i32;
                let v = self.osc.volume - 1 + ((self.osc.read_reg(2) as i32 >> 2) & 2);
                if (v as u32) < 15 {
                    self.osc.volume = v;
                }
            }
        }
    }

    /// Write a register. Returns true on trigger.
    pub fn write_register(&mut self, reg: usize, data: u8) -> bool {
        match reg {
            1 => {
                self.osc.length = 64 - (data & 0x3F) as i32;
                false
            }
            2 => {
                if data >> 4 == 0 {
                    self.osc.enabled = false;
                }
                false
            }
            4 if data & TRIGGER != 0 => {
                self.env_delay = (self.osc.read_reg(2) & 7) as i32;
                self.osc.volume = (self.osc.read_reg(2) >> 4) as i32;
                self.osc.enabled = true;
                if self.osc.length == 0 {
                    self.osc.length = 64;
                }
                true
            }
            4 => false,
            _ => false,
        }
    }
}

/// Square wave oscillator (extends GbEnv).
pub struct GbSquare {
    pub env: GbEnv,
    pub synth: *const BlipSynth,
    pub sweep_delay: i32,
    pub sweep_freq: i32,
    pub phase: i32,
}

impl Default for GbSquare {
    fn default() -> Self {
        Self::new()
    }
}

impl GbSquare {
    pub const PERIOD_MASK: u8 = 0x70;
    pub const SHIFT_MASK: u8 = 0x07;

    pub fn new() -> Self {
        Self {
            env: GbEnv::new(),
            synth: std::ptr::null(),
            sweep_delay: 0,
            sweep_freq: 0,
            phase: 0,
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0;
        self.sweep_freq = 0;
        self.sweep_delay = 0;
        self.env.reset();
    }

    /// Clock the sweep unit.
    pub fn clock_sweep(&mut self) {
        let sweep_period = ((self.env.osc.read_reg(0) & Self::PERIOD_MASK) >> 4) as i32;
        if sweep_period != 0 && self.sweep_delay > 0 {
            self.sweep_delay -= 1;
            if self.sweep_delay == 0 {
                self.sweep_delay = sweep_period;
                // Update frequency registers from sweep_freq
                self.env.osc.write_reg(3, (self.sweep_freq & 0xFF) as u8);
                let r4 = self.env.osc.read_reg(4);
                self.env
                    .osc
                    .write_reg(4, (r4 & !0x07) | ((self.sweep_freq >> 8) & 0x07) as u8);

                let shift = (self.env.osc.read_reg(0) & Self::SHIFT_MASK) as i32;
                let mut offset = self.sweep_freq >> shift;
                if self.env.osc.read_reg(0) & 0x08 != 0 {
                    offset = -offset;
                }
                self.sweep_freq += offset;

                if self.sweep_freq < 0 {
                    // C++ does NOT clear sweep_delay here
                    self.sweep_freq = 0;
                } else if self.sweep_freq >= 2048 {
                    self.sweep_delay = 0;
                    self.sweep_freq = 2048;
                }
            }
        }
    }

    /// Run the square wave oscillator from time to end_time.
    pub fn run(&mut self, time: BlipTime, end_time: BlipTime, playing: bool) {
        let mut playing = playing;
        if self.sweep_freq == 2048 {
            playing = false;
        }

        let duty_table: [i32; 4] = [1, 2, 4, 6];
        let duty = duty_table[(self.env.osc.read_reg(1) >> 6) as usize];

        let mut amp = if playing { self.env.osc.volume } else { 0 };
        if self.phase >= duty {
            amp = -amp;
        }

        let frequency = self.env.osc.frequency();
        if (frequency as u32).wrapping_sub(1) > 2040 {
            amp = self.env.osc.volume >> 1;
            playing = false;
        }

        let delta = amp - self.env.osc.last_amp;
        if delta != 0 {
            self.env.osc.last_amp = amp;
            if let Some(out) = self.env.osc.output {
                unsafe {
                    (*self.synth).offset_resampled((*out).resampled_time(time), delta, &mut *out);
                }
            }
        }

        let mut time = time + self.env.osc.delay;
        if !playing {
            time = end_time;
        }

        if time < end_time {
            let period = (2048 - frequency) * 4;
            if let Some(out) = self.env.osc.output {
                let mut phase = self.phase;
                let mut delta = amp * 2;
                unsafe {
                    let out_ref = &mut *out;
                    while time < end_time {
                        phase = (phase + 1) & 7;
                        if phase == 0 || phase == duty {
                            delta = -delta;
                            (*self.synth).offset_resampled(
                                out_ref.resampled_time(time),
                                delta,
                                out_ref,
                            );
                        }
                        time += period;
                    }
                }
                self.phase = phase;
                self.env.osc.last_amp = delta >> 1;
            }
        }
        self.env.osc.delay = time - end_time;
    }
}

/// Noise oscillator (extends GbEnv).
pub struct GbNoise {
    pub env: GbEnv,
    pub synth: *const BlipSynth,
    pub bits: u32,
}

impl Default for GbNoise {
    fn default() -> Self {
        Self::new()
    }
}

impl GbNoise {
    pub fn new() -> Self {
        Self {
            env: GbEnv::new(),
            synth: std::ptr::null(),
            bits: 1,
        }
    }

    pub fn reset(&mut self) {
        self.bits = 1;
        self.env.reset();
    }

    /// Run the noise oscillator from time to end_time.
    pub fn run(&mut self, time: BlipTime, end_time: BlipTime, playing: bool) {
        let mut amp = if playing { self.env.osc.volume } else { 0 };
        let tap = 13 - ((self.env.osc.read_reg(3) & 8) as i32);
        if (self.bits >> tap) & 2 != 0 {
            amp = -amp;
        }

        let delta = amp - self.env.osc.last_amp;
        if delta != 0 {
            self.env.osc.last_amp = amp;
            if let Some(out) = self.env.osc.output {
                unsafe {
                    (*self.synth).offset_resampled((*out).resampled_time(time), delta, &mut *out);
                }
            }
        }

        let mut time = time + self.env.osc.delay;
        if !playing {
            time = end_time;
        }

        if time < end_time {
            let period_table: [i32; 8] = [8, 16, 32, 48, 64, 80, 96, 112];
            let period = period_table[(self.env.osc.read_reg(3) & 7) as usize]
                << (self.env.osc.read_reg(3) >> 4);

            if let Some(out) = self.env.osc.output {
                unsafe {
                    let out_ref = &mut *out;
                    let resampled_period = out_ref.resampled_duration(period);
                    let mut resampled_time = out_ref.resampled_time(time);
                    let mut bits = self.bits;
                    let mut delta = amp * 2;

                    while time < end_time {
                        let changed = (bits >> tap) + 1;
                        time += period;
                        bits <<= 1;
                        if changed & 2 != 0 {
                            delta = -delta;
                            bits |= 1;
                            (*self.synth).offset_resampled(resampled_time, delta, out_ref);
                        }
                        resampled_time += resampled_period;
                    }

                    self.bits = bits;
                    self.env.osc.last_amp = delta >> 1;
                }
            }
        }
        self.env.osc.delay = time - end_time;
    }
}

/// Wave oscillator (extends GbOsc).
pub struct GbWave {
    pub osc: GbOsc,
    pub synth: *const BlipSynth,
    pub wave_pos: i32,
    pub wave: [u8; 32],
}

impl Default for GbWave {
    fn default() -> Self {
        Self::new()
    }
}

impl GbWave {
    pub const WAVE_SIZE: usize = 32;

    pub fn new() -> Self {
        Self {
            osc: GbOsc::new(),
            synth: std::ptr::null(),
            wave_pos: 0,
            wave: [0; 32],
        }
    }

    pub fn reset(&mut self) {
        self.wave_pos = 0;
        self.osc.reset();
    }

    /// Write a register.
    pub fn write_register(&mut self, reg: usize, data: u8) {
        match reg {
            0 if data & 0x80 == 0 => {
                self.osc.enabled = false;
            }
            0 => {}
            1 => {
                self.osc.length = 256 - data as i32;
            }
            2 => {
                self.osc.volume = (data as i32 >> 5) & 3;
            }
            4 if data & TRIGGER != 0 && self.osc.read_reg(0) & 0x80 != 0 => {
                self.wave_pos = 0;
                self.osc.enabled = true;
                if self.osc.length == 0 {
                    self.osc.length = 256;
                }
            }
            4 => {}
            _ => {}
        }
    }

    /// Run the wave oscillator from time to end_time.
    pub fn run(&mut self, time: BlipTime, end_time: BlipTime, playing: bool) {
        let volume_shift = ((self.osc.volume - 1) & 7) as u32; // volume=0 causes shift=7
        let mut playing = playing;

        let mut amp = (self.wave[self.wave_pos as usize] >> volume_shift) as i32
            * if playing { 2 } else { 0 };

        let frequency = self.osc.frequency();
        if (frequency as u32).wrapping_sub(1) > 2044 {
            amp = (30 >> volume_shift) * if playing { 1 } else { 0 };
            playing = false;
        }

        let delta = amp - self.osc.last_amp;
        if delta != 0 {
            self.osc.last_amp = amp;
            if let Some(out) = self.osc.output {
                unsafe {
                    (*self.synth).offset_resampled((*out).resampled_time(time), delta, &mut *out);
                }
            }
        }

        let mut time = time + self.osc.delay;
        if !playing {
            time = end_time;
        }

        if time < end_time {
            let period = (2048 - frequency) * 2;
            let mut wave_pos = (self.wave_pos + 1) & (Self::WAVE_SIZE as i32 - 1);

            if let Some(out) = self.osc.output {
                unsafe {
                    let out_ref = &mut *out;
                    while time < end_time {
                        let amp = (self.wave[wave_pos as usize] >> volume_shift) as i32 * 2;
                        wave_pos = (wave_pos + 1) & (Self::WAVE_SIZE as i32 - 1);
                        let delta = amp - self.osc.last_amp;
                        if delta != 0 {
                            self.osc.last_amp = amp;
                            (*self.synth).offset_resampled(
                                out_ref.resampled_time(time),
                                delta,
                                out_ref,
                            );
                        }
                        time += period;
                    }
                }
                self.wave_pos = (wave_pos - 1) & (Self::WAVE_SIZE as i32 - 1);
            }
        }
        self.osc.delay = time - end_time;
    }
}
