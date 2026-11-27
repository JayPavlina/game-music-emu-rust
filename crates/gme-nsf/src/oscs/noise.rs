use super::NesEnvelope;
use gme_core::blip_buffer::{BlipSynth, BlipTime};

// 1:1 port of C++ Nes_Noise (Nes_Oscs.cpp)

/// Noise period table (NTSC).
const NOISE_PERIOD_TABLE: [i32; 16] = [
    0x004, 0x008, 0x010, 0x020, 0x040, 0x060, 0x080, 0x0A0, 0x0CA, 0x0FE, 0x17C, 0x1FC, 0x2FA,
    0x3F8, 0x7F2, 0xFE4,
];

const MODE_FLAG: u8 = 0x80;

pub struct NesNoise {
    pub env: NesEnvelope,
    pub noise: i32, // LFSR
}

impl Default for NesNoise {
    fn default() -> Self {
        Self::new()
    }
}

impl NesNoise {
    pub fn new() -> Self {
        Self {
            env: NesEnvelope::new(),
            noise: 1 << 14,
        }
    }

    pub fn reset(&mut self) {
        self.noise = 1 << 14;
        self.env.reset();
    }

    pub fn write_reg(&mut self, reg: usize, data: u8) {
        self.env.osc.regs[reg] = data;
        self.env.osc.reg_written[reg] = true;
    }

    pub fn timer_period(&self) -> i32 {
        NOISE_PERIOD_TABLE[(self.env.osc.regs[2] & 0x0F) as usize]
    }

    /// C++ Nes_Noise::run.
    pub fn run(&mut self, mut time: BlipTime, end_time: BlipTime, synth: &BlipSynth) {
        let period = NOISE_PERIOD_TABLE[(self.env.osc.regs[2] & 15) as usize];

        let output = match self.env.osc.output {
            Some(o) => unsafe { &mut *o },
            None => {
                // TODO: clean up (matches C++ comment)
                time += self.env.osc.delay;
                self.env.osc.delay =
                    time + (end_time - time + period - 1) / period * period - end_time;
                return;
            }
        };

        output.set_modified();

        let volume = self.env.volume();
        let amp = if self.noise & 1 != 0 { volume } else { 0 };
        {
            let delta = self.env.osc.update_amp(amp);
            if delta != 0 {
                synth.offset_at(time, delta, output);
            }
        }

        time += self.env.osc.delay;
        if time < end_time {
            if volume == 0 {
                // round to next multiple of period
                time += (end_time - time + period - 1) / period * period;

                // approximate noise cycling while muted, by shuffling up noise register
                // to do: precise muted noise cycling?
                if self.env.osc.regs[2] & MODE_FLAG == 0 {
                    let feedback = (self.noise << 13) ^ (self.noise << 14);
                    self.noise = (feedback & 0x4000) | (self.noise >> 1);
                }
            } else {
                // using resampled time avoids conversion in synth.offset()
                let rperiod = output.resampled_duration(period);
                let mut rtime = output.resampled_time(time);

                let mut noise = self.noise;
                let mut delta = amp * 2 - volume;
                let tap = if self.env.osc.regs[2] & MODE_FLAG != 0 {
                    8
                } else {
                    13
                };

                loop {
                    let feedback = (noise << tap) ^ (noise << 14);
                    time += period;

                    if (noise + 1) & 2 != 0 {
                        // bits 0 and 1 of noise differ
                        delta = -delta;
                        synth.offset_resampled(rtime, delta, output);
                    }

                    rtime = rtime.wrapping_add(rperiod);
                    noise = (feedback & 0x4000) | (noise >> 1);

                    if time >= end_time {
                        break;
                    }
                }

                self.env.osc.last_amp = (delta + volume) >> 1;
                self.noise = noise;
            }
        }

        self.env.osc.delay = time - end_time;
    }
}
