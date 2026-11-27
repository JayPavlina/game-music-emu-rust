use super::oscs::LENGTH_TABLE;
use super::oscs::dmc::{NO_IRQ, NesDmc};
use super::oscs::noise::NesNoise;
use super::oscs::square::{self, NesSquare};
use super::oscs::triangle::NesTriangle;
use gme_core::blip_buffer::{BlipEq, BlipSynth, BlipTime};

// 1:1 port of C++ Nes_Apu (Nes_Apu.cpp)

const AMP_RANGE: f64 = 15.0;

/// Synth instances owned by the APU. C++ uses blip_good_quality (12) for the
/// shared square synth and blip_med_quality (8) for triangle, noise and dmc.
pub struct ApuSynths {
    pub square: BlipSynth,
    pub triangle: BlipSynth,
    pub noise: BlipSynth,
    pub dmc: BlipSynth,
}

impl Default for ApuSynths {
    fn default() -> Self {
        Self::new()
    }
}

impl ApuSynths {
    pub fn new() -> Self {
        let mut square = BlipSynth::new(12);
        square.volume_unit(0.1128 / AMP_RANGE);
        let mut triangle = BlipSynth::new(8);
        triangle.volume_unit(0.12765 / AMP_RANGE);
        let mut noise = BlipSynth::new(8);
        noise.volume_unit(0.0741 / AMP_RANGE);
        let mut dmc = BlipSynth::new(8);
        dmc.volume_unit(0.42545 / 127.0);
        Self {
            square,
            triangle,
            noise,
            dmc,
        }
    }
}

/// NES APU (2A03). Contains 5 oscillators and a frame counter.
pub struct NesApu {
    pub square1: NesSquare,
    pub square2: NesSquare,
    pub triangle: NesTriangle,
    pub noise: NesNoise,
    pub dmc: NesDmc,
    pub synths: ApuSynths,

    pub last_time: BlipTime,
    pub last_dmc_time: BlipTime,
    pub earliest_irq: BlipTime,
    pub next_irq: BlipTime,

    pub frame_period: i32,
    pub frame_delay: i32,
    pub frame: i32,
    pub osc_enables: i32,
    pub frame_mode: i32,
    pub irq_flag: bool,
    pub pal_mode: bool,

    pub tempo: f64,
}

impl Default for NesApu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesApu {
    pub fn new() -> Self {
        Self {
            square1: NesSquare::new(),
            square2: NesSquare::new(),
            triangle: NesTriangle::new(),
            noise: NesNoise::new(),
            dmc: NesDmc::new(),
            synths: ApuSynths::new(),

            last_time: 0,
            last_dmc_time: 0,
            earliest_irq: NO_IRQ,
            next_irq: NO_IRQ,

            frame_period: 7458,
            frame_delay: 7458,
            frame: 0,
            osc_enables: 0,
            frame_mode: 0,
            irq_flag: false,
            pal_mode: false,

            tempo: 1.0,
        }
    }

    /// C++ Nes_Apu::set_tempo.
    pub fn set_tempo(&mut self, t: f64) {
        self.tempo = t;
        self.frame_period = if self.pal_mode { 8314 } else { 7458 };
        if t != 1.0 {
            self.frame_period = (self.frame_period as f64 / t) as i32 & !1; // must be even
        }
    }

    /// C++ Nes_Apu::reset up to (but not including) the register write sequence.
    pub fn reset(&mut self) {
        self.dmc.pal_mode = self.pal_mode;
        self.set_tempo(self.tempo);

        self.square1.reset();
        self.square2.reset();
        self.triangle.reset();
        self.noise.reset();
        self.dmc.reset();

        self.last_time = 0;
        self.last_dmc_time = 0;
        self.osc_enables = 0;
        self.irq_flag = false;
        self.earliest_irq = NO_IRQ;
        self.frame_delay = 1;
    }

    /// C++ Nes_Apu::reset (full, including the register init writes).
    pub fn reset_and_init(&mut self, initial_dmc_dac: u8) {
        self.reset();

        self.write_register(0, 0x4017, 0x00);
        self.write_register(0, 0x4015, 0x00);

        for addr in 0x4000u16..=0x4013 {
            let data = if addr & 3 != 0 { 0x00 } else { 0x10 };
            self.write_register(0, addr, data);
        }

        self.dmc.dac = initial_dmc_dac as i32;
        if !self.dmc.nonlinear {
            self.triangle.osc.last_amp = 15;
            self.dmc.osc.last_amp = initial_dmc_dac as i32; // prevent output transition
        }
    }

    /// C++ Nes_Apu::volume.
    pub fn volume(&mut self, v: f64) {
        self.dmc.nonlinear = false;
        self.synths.square.volume_unit(0.1128 / AMP_RANGE * v);
        self.synths.triangle.volume_unit(0.12765 / AMP_RANGE * v);
        self.synths.noise.volume_unit(0.0741 / AMP_RANGE * v);
        self.synths.dmc.volume_unit(0.42545 / 127.0 * v);
    }

    /// C++ Nes_Apu::treble_eq.
    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synths.square.treble_eq(eq);
        self.synths.triangle.treble_eq(eq);
        self.synths.noise.treble_eq(eq);
        self.synths.dmc.treble_eq(eq);
    }

    /// C++ Nes_Apu::irq_changed (no notifier in the NSF player).
    fn irq_changed(&mut self) {
        let mut new_irq = self.dmc.next_irq;
        if self.dmc.irq_flag | self.irq_flag {
            new_irq = 0;
        } else if new_irq > self.next_irq {
            new_irq = self.next_irq;
        }
        self.earliest_irq = new_irq;
    }

    /// C++ fill_buffer mutates apu state via back-pointer; consume the
    /// substitute flags immediately after any dmc.run()/start()/write.
    fn consume_dmc_flags(&mut self) {
        if self.dmc.pending_disable {
            self.dmc.pending_disable = false;
            self.osc_enables &= !0x10;
        }
        if self.dmc.irq_dirty {
            self.dmc.irq_dirty = false;
            self.irq_changed();
        }
    }

    /// C++ Nes_Apu::run_until_.
    pub fn run_until(&mut self, end_time: BlipTime) {
        if end_time <= self.last_time {
            return;
        }

        if self.last_dmc_time < end_time {
            let start = self.last_dmc_time;
            self.last_dmc_time = end_time;
            self.dmc.run(start, end_time, &self.synths.dmc);
            self.consume_dmc_flags();
        }

        loop {
            // earlier of next frame time or end time
            let mut time = self.last_time + self.frame_delay;
            if time > end_time {
                time = end_time;
            }
            self.frame_delay -= time - self.last_time;

            // run oscs to present
            self.square1.run(self.last_time, time, &self.synths.square);
            self.square2.run(self.last_time, time, &self.synths.square);
            self.triangle
                .run(self.last_time, time, &self.synths.triangle);
            self.noise.run(self.last_time, time, &self.synths.noise);
            self.last_time = time;

            if time == end_time {
                break; // no more frames to run
            }

            // take frame-specific actions
            self.frame_delay = self.frame_period;
            let current_frame = self.frame;
            self.frame += 1;
            match current_frame {
                0 | 2 => {
                    if current_frame == 0 && self.frame_mode & 0xC0 == 0 {
                        self.next_irq = time + self.frame_period * 4 + 2;
                        self.irq_flag = true;
                    }

                    // clock length and sweep on frames 0 and 2
                    self.square1.env.osc.clock_length(0x20);
                    self.square2.env.osc.clock_length(0x20);
                    self.noise.env.osc.clock_length(0x20);
                    self.triangle.osc.clock_length(0x80); // different halt flag bit on triangle

                    self.square1.clock_sweep(-1);
                    self.square2.clock_sweep(0);

                    // frame 2 is slightly shorter in mode 1
                    if self.pal_mode && self.frame == 3 {
                        self.frame_delay -= 2;
                    }
                }
                // frame 1 is slightly shorter in mode 0
                1 if !self.pal_mode => {
                    self.frame_delay -= 2;
                }
                3 => {
                    self.frame = 0;

                    // frame 3 is almost twice as long in mode 1
                    if self.frame_mode & 0x80 != 0 {
                        self.frame_delay += self.frame_period - if self.pal_mode { 2 } else { 6 };
                    }
                }
                _ => {}
            }

            // clock envelopes and linear counter every frame
            self.triangle.clock_linear_counter();
            self.square1.env.clock_envelope();
            self.square2.env.clock_envelope();
            self.noise.env.clock_envelope();
        }
    }

    /// C++ Nes_Apu::write_register.
    pub fn write_register(&mut self, time: BlipTime, addr: u16, data: u8) {
        // Differential-trace hook (NSF_APU_TRACE_RS=<path>), mirrors the C++ hook
        #[cfg(feature = "trace")]
        crate::apu_trace(time, addr, data);

        // Ignore addresses outside range
        if !(0x4000..=0x4017).contains(&addr) {
            return;
        }

        self.run_until(time);

        if addr < 0x4014 {
            // Write to channel
            let osc_index = ((addr - 0x4000) >> 2) as usize;
            let reg = (addr & 3) as usize;

            match osc_index {
                0 => self.square1.write_reg(reg, data),
                1 => self.square2.write_reg(reg, data),
                2 => self.triangle.write_reg(reg, data),
                3 => self.noise.write_reg(reg, data),
                _ => {
                    self.dmc.osc.regs[reg] = data;
                    self.dmc.osc.reg_written[reg] = true;
                }
            }

            if osc_index == 4 {
                // handle DMC specially
                let last_dmc_time = self.last_dmc_time;
                self.dmc.write_register(reg, data, last_dmc_time);
                self.consume_dmc_flags();
            } else if reg == 3 {
                // load length counter
                if (self.osc_enables >> osc_index) & 1 != 0 {
                    let lc = LENGTH_TABLE[(data as usize >> 3) & 0x1F] as i32;
                    match osc_index {
                        0 => self.square1.env.osc.length_counter = lc,
                        1 => self.square2.env.osc.length_counter = lc,
                        2 => self.triangle.osc.length_counter = lc,
                        _ => self.noise.env.osc.length_counter = lc,
                    }
                }

                // reset square phase
                if osc_index == 0 {
                    self.square1.phase = square::PHASE_RANGE - 1;
                } else if osc_index == 1 {
                    self.square2.phase = square::PHASE_RANGE - 1;
                }
            }
        } else if addr == 0x4015 {
            // Channel enables
            if data & 0x01 == 0 {
                self.square1.env.osc.length_counter = 0;
            }
            if data & 0x02 == 0 {
                self.square2.env.osc.length_counter = 0;
            }
            if data & 0x04 == 0 {
                self.triangle.osc.length_counter = 0;
            }
            if data & 0x08 == 0 {
                self.noise.env.osc.length_counter = 0;
            }
            if data & 0x10 == 0 {
                self.dmc.osc.length_counter = 0;
            }

            let mut recalc_irq = self.dmc.irq_flag;
            self.dmc.irq_flag = false;

            let old_enables = self.osc_enables;
            self.osc_enables = data as i32;
            if data & 0x10 == 0 {
                self.dmc.next_irq = NO_IRQ;
                recalc_irq = true;
            } else if old_enables & 0x10 == 0 {
                // dmc just enabled
                let last_dmc_time = self.last_dmc_time;
                self.dmc.start(last_dmc_time);
                self.consume_dmc_flags();
            }

            if recalc_irq {
                self.irq_changed();
            }
        } else if addr == 0x4017 {
            // Frame mode
            self.frame_mode = data as i32;

            let irq_enabled = data & 0x40 == 0;
            self.irq_flag &= irq_enabled;
            self.next_irq = NO_IRQ;

            // mode 1
            self.frame_delay &= 1;
            self.frame = 0;

            if data & 0x80 == 0 {
                // mode 0
                self.frame = 1;
                self.frame_delay += self.frame_period;
                if irq_enabled {
                    self.next_irq = time + self.frame_delay + self.frame_period * 3 + 1;
                }
            }

            self.irq_changed();
        }
    }

    /// C++ Nes_Apu::read_status.
    pub fn read_status(&mut self, time: BlipTime) -> u8 {
        self.run_until(time - 1);

        let mut result = ((self.dmc.irq_flag as u8) << 7) | ((self.irq_flag as u8) << 6);

        if self.square1.env.osc.length_counter != 0 {
            result |= 0x01;
        }
        if self.square2.env.osc.length_counter != 0 {
            result |= 0x02;
        }
        if self.triangle.osc.length_counter != 0 {
            result |= 0x04;
        }
        if self.noise.env.osc.length_counter != 0 {
            result |= 0x08;
        }
        if self.dmc.osc.length_counter != 0 {
            result |= 0x10;
        }

        self.run_until(time);

        if self.irq_flag {
            result |= 0x40;
            self.irq_flag = false;
            self.irq_changed();
        }

        result
    }

    /// C++ Nes_Apu::end_frame. The nonlinear zero_apu_osc path is omitted
    /// (the NSF player always uses linear mixing).
    pub fn end_frame(&mut self, end_time: BlipTime) {
        if end_time > self.last_time {
            self.run_until(end_time);
        }

        // make times relative to new frame
        self.last_time -= end_time;
        self.last_dmc_time -= end_time;

        if self.next_irq != NO_IRQ {
            self.next_irq -= end_time;
        }
        if self.dmc.next_irq != NO_IRQ {
            self.dmc.next_irq -= end_time;
        }
        if self.earliest_irq != NO_IRQ {
            self.earliest_irq -= end_time;
            if self.earliest_irq < 0 {
                self.earliest_irq = 0;
            }
        }
    }
}
