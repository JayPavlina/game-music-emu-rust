use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipSynth, BlipTime};

/// VRC6 pulse oscillator.
struct Vrc6Pulse {
    regs: [u8; 3],
    output: Option<*mut BlipBuffer>,
    delay: i32,
    last_amp: i32,
    phase: i32,
}

impl Vrc6Pulse {
    fn new() -> Self {
        Self {
            regs: [0; 3],
            output: None,
            delay: 0,
            last_amp: 0,
            phase: 1,
        }
    }

    fn reset(&mut self) {
        self.regs = [0; 3];
        self.delay = 0;
        self.last_amp = 0;
        self.phase = 1;
    }

    fn period(&self) -> i32 {
        ((self.regs[2] & 0x0F) as i32) * 0x100 + self.regs[1] as i32 + 1
    }

    fn run(&mut self, time: BlipTime, end_time: BlipTime, synth: &BlipSynth) {
        let output = match self.output {
            Some(o) => unsafe { &mut *o },
            None => return,
        };
        output.set_modified();

        let volume = self.regs[0] & 0x0F;
        let volume = if self.regs[2] & 0x80 != 0 { volume } else { 0 };

        let gate = self.regs[0] & 0x80;
        let duty = ((self.regs[0] >> 4) & 7) as i32 + 1;

        let amp = if gate != 0 || self.phase < duty {
            volume
        } else {
            0
        };
        let delta = amp as i32 - self.last_amp;
        let mut time = time;
        if delta != 0 {
            self.last_amp += delta;
            synth.offset_at(time, delta, output);
        }

        time += self.delay;
        self.delay = 0;
        let period = self.period();

        if volume != 0 && gate == 0 && period > 4 {
            if time < end_time {
                let mut phase = self.phase;
                loop {
                    phase += 1;
                    if phase == 16 {
                        phase = 0;
                        self.last_amp = volume as i32;
                        synth.offset_at(time, volume as i32, output);
                    }
                    if phase == duty {
                        self.last_amp = 0;
                        synth.offset_at(time, -(volume as i32), output);
                    }
                    time += period;
                    if time >= end_time {
                        break;
                    }
                }
                self.phase = phase;
            }
            self.delay = time - end_time;
        }
    }
}

/// VRC6 sawtooth oscillator.
struct Vrc6Saw {
    regs: [u8; 3],
    output: Option<*mut BlipBuffer>,
    delay: i32,
    last_amp: i32,
    phase: i32,
    amp: i32,
}

impl Vrc6Saw {
    fn new() -> Self {
        Self {
            regs: [0; 3],
            output: None,
            delay: 0,
            last_amp: 0,
            phase: 1,
            amp: 0,
        }
    }

    fn reset(&mut self) {
        self.regs = [0; 3];
        self.delay = 0;
        self.last_amp = 0;
        self.phase = 1;
        self.amp = 0;
    }

    fn period(&self) -> i32 {
        ((self.regs[2] & 0x0F) as i32) * 0x100 + self.regs[1] as i32 + 1
    }

    fn run(&mut self, time: BlipTime, end_time: BlipTime, synth: &BlipSynth) {
        let output = match self.output {
            Some(o) => unsafe { &mut *o },
            None => return,
        };
        output.set_modified();

        let amp_step = (self.regs[0] & 0x3F) as i32;
        let mut time = time;
        let mut last_amp = self.last_amp;
        let mut amp = self.amp;

        if self.regs[2] & 0x80 == 0 || (amp_step | amp) == 0 {
            self.delay = 0;
            let delta = (amp >> 3) - last_amp;
            last_amp = amp >> 3;
            synth.offset_at(time, delta, output);
        } else {
            time += self.delay;
            if time < end_time {
                let period = self.period() * 2;
                let mut phase = self.phase;

                loop {
                    phase -= 1;
                    if phase == 0 {
                        phase = 7;
                        amp = 0;
                    }

                    let delta = (amp >> 3) - last_amp;
                    if delta != 0 {
                        last_amp = amp >> 3;
                        synth.offset_at(time, delta, output);
                    }

                    time += period;
                    amp = (amp + amp_step) & 0xFF;

                    if time >= end_time {
                        break;
                    }
                }

                self.phase = phase;
                self.amp = amp;
            }

            self.delay = time - end_time;
        }

        self.last_amp = last_amp;
    }
}

/// VRC6 sound chip: 2 pulse oscillators + 1 sawtooth.
pub struct NesVrc6Apu {
    pulse1: Vrc6Pulse,
    pulse2: Vrc6Pulse,
    saw: Vrc6Saw,
    pub last_time: BlipTime,
    square_synth: BlipSynth,
    saw_synth: BlipSynth,
}

impl Default for NesVrc6Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesVrc6Apu {
    pub const OSC_COUNT: usize = 3;
    pub const BASE_ADDR: u16 = 0x9000;
    pub const ADDR_STEP: u16 = 0x1000;
    pub const REG_COUNT: usize = 3;

    pub fn new() -> Self {
        let mut square_synth = BlipSynth::new(12);
        square_synth.volume_unit(0.0967 * 2.0 * 0.5 / 15.0);
        let mut saw_synth = BlipSynth::new(8);
        saw_synth.volume_unit(0.0967 * 2.0 / 31.0);

        Self {
            pulse1: Vrc6Pulse::new(),
            pulse2: Vrc6Pulse::new(),
            saw: Vrc6Saw::new(),
            last_time: 0,
            square_synth,
            saw_synth,
        }
    }

    pub fn reset(&mut self) {
        self.pulse1.reset();
        self.pulse2.reset();
        self.saw.reset();
        self.last_time = 0;
    }

    /// Set volume (gain) for all VRC6 synths, matching C++ Nes_Vrc6_Apu::volume().
    pub fn volume(&mut self, v: f64) {
        let factor = 0.0967 * 2.0;
        self.square_synth.volume_unit(factor * 0.5 / 15.0 * v);
        self.saw_synth.volume_unit(factor / 31.0 * v);
    }

    /// Set treble equalization (matches C++ Nes_Vrc6_Apu::treble_eq).
    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.square_synth.treble_eq(eq);
        self.saw_synth.treble_eq(eq);
    }

    /// Set output buffer for all oscillators.
    pub fn set_output(&mut self, buf: *mut BlipBuffer) {
        self.pulse1.output = Some(buf);
        self.pulse2.output = Some(buf);
        self.saw.output = Some(buf);
    }

    /// Set output buffer for a specific oscillator.
    pub fn osc_output(&mut self, index: usize, buf: Option<*mut BlipBuffer>) {
        match index {
            0 => self.pulse1.output = buf,
            1 => self.pulse2.output = buf,
            2 => self.saw.output = buf,
            _ => {}
        }
    }

    /// Write to a VRC6 oscillator register.
    pub fn write_osc(&mut self, time: BlipTime, osc: usize, reg: usize, data: u8) {
        self.run_until(time);
        match osc {
            0 => self.pulse1.regs[reg] = data,
            1 => self.pulse2.regs[reg] = data,
            2 => self.saw.regs[reg] = data,
            _ => {}
        }
    }

    fn run_until(&mut self, time: BlipTime) {
        self.pulse1.run(self.last_time, time, &self.square_synth);
        self.pulse2.run(self.last_time, time, &self.square_synth);
        self.saw.run(self.last_time, time, &self.saw_synth);
        self.last_time = time;
    }

    pub fn end_frame(&mut self, time: BlipTime) {
        if time > self.last_time {
            self.run_until(time);
        }
        self.last_time -= time;
    }
}
