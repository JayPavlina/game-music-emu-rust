pub const GAIN_UNIT: i32 = 0x100;
pub const BASS_NONE: i32 = 0;
pub const BASS_NORM: i32 = 8;
pub const BASS_MAX: i32 = 31;
const GAIN_BITS: i32 = 8;

#[derive(Clone, Copy)]
struct ChannelState {
    p1: i32,
    pp1: i32,
    sum: i32,
}

impl ChannelState {
    const fn new() -> Self {
        Self {
            p1: 0,
            pp1: 0,
            sum: 0,
        }
    }
}

pub struct SpcFilter {
    enabled: bool,
    gain: i32,
    bass: i32,
    ch: [ChannelState; 2],
}

impl Default for SpcFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl SpcFilter {
    pub fn new() -> Self {
        Self {
            enabled: true,
            gain: GAIN_UNIT,
            bass: BASS_NORM,
            ch: [ChannelState::new(); 2],
        }
    }

    pub fn clear(&mut self) {
        self.ch = [ChannelState::new(); 2];
    }

    pub fn enable(&mut self, b: bool) {
        self.enabled = b;
    }

    pub fn set_gain(&mut self, g: i32) {
        self.gain = g;
    }

    pub fn set_bass(&mut self, b: i32) {
        self.bass = b;
    }

    pub fn run(&mut self, io: &mut [i16]) {
        let count = io.len();
        debug_assert!(count & 1 == 0); // must be even (stereo)

        let gain = self.gain;
        if self.enabled {
            let bass = self.bass;
            // Process both channels (left=0, right=1)
            for ch_idx in (0..2).rev() {
                let c = &mut self.ch[ch_idx];
                let mut sum = c.sum;
                let mut pp1 = c.pp1;
                let mut p1 = c.p1;

                let mut i = ch_idx;
                while i < count {
                    // Low-pass filter (two point FIR with coeffs 0.25, 0.75)
                    let f = io[i] as i32 + p1;
                    p1 = io[i] as i32 * 3;

                    // High-pass filter ("leaky integrator")
                    let delta = f - pp1;
                    pp1 = f;
                    let s = sum >> (GAIN_BITS + 2);
                    sum += (delta * gain) - (sum >> bass);

                    // Clamp to 16 bits
                    let out = if s as i16 as i32 != s {
                        (s >> 31) ^ 0x7FFF
                    } else {
                        s
                    };

                    io[i] = out as i16;
                    i += 2;
                }

                c.p1 = p1;
                c.pp1 = pp1;
                c.sum = sum;
            }
        } else if gain != GAIN_UNIT {
            for sample in io.iter_mut() {
                let s = (*sample as i32 * gain) >> GAIN_BITS;
                *sample = if s as i16 as i32 != s {
                    ((s >> 31) ^ 0x7FFF) as i16
                } else {
                    s as i16
                };
            }
        }
    }
}
