/// Atari POKEY sound chip emulator.
/// Matches C++ Sap_Apu.h / Sap_Apu.cpp.
use gme_core::blip_buffer::{BLIP_GOOD_QUALITY, BlipBuffer, BlipSynth, BlipTime};

pub const OSC_COUNT: usize = 4;
pub const START_ADDR: u32 = 0xD200;
pub const END_ADDR: u32 = 0xD209;

const POLY4_LEN: usize = (1 << 4) - 1;
const POLY9_LEN: usize = (1 << 9) - 1;
const POLY17_LEN: usize = (1 << 17) - 1;

const MAX_FREQUENCY: i32 = 12000;

/// Precomputed poly5 constant (matches C++).
const POLY5: u32 = 0x167C6EA1;
const POLY5_LEN: usize = (1 << 5) - 1;
const POLY5_MASK: u32 = (1 << POLY5_LEN) - 1;

/// Run poly5 rotation.
#[inline]
fn run_poly5(in_val: u32, shift: usize) -> u32 {
    ((in_val << shift) & POLY5_MASK) | (in_val >> (POLY5_LEN - shift))
}

/// Generate polynomial sequence using Galois LFSR configuration.
fn gen_poly(mask: u32, count: usize) -> Vec<u8> {
    let mut out = vec![0u8; count];
    let mut n: u32 = 1;
    for o in out.iter_mut().take(count) {
        let mut bits: u8 = 0;
        let mut b = 0;
        loop {
            bits |= ((n & 1) << b) as u8;
            n = (n >> 1) ^ (mask & (0u32.wrapping_sub(n & 1)));
            if b >= 7 {
                break;
            }
            b += 1;
        }
        *o = bits;
    }
    out
}

/// POLY_MASK macro from C++.
macro_rules! poly_mask {
    ($width:expr, $tap1:expr, $tap2:expr) => {
        (1u32 << ($width - 1 - $tap1)) | (1u32 << ($width - 1 - $tap2))
    };
}

pub struct Osc {
    pub regs: [u8; 2],
    phase: u8,
    invert: u8,
    last_amp: i32,
    delay: BlipTime,
    period: BlipTime,
    output: Option<*mut BlipBuffer>,
}

impl Osc {
    fn new() -> Self {
        Self {
            regs: [0; 2],
            phase: 0,
            invert: 0,
            last_amp: 0,
            delay: 0,
            period: 0,
            output: None,
        }
    }
}

/// Shared tables and Blip_Synth that can be shared among multiple SapApu objects.
pub struct SapApuImpl {
    pub synth: BlipSynth,
    poly4: Vec<u8>,
    poly9: Vec<u8>,
    poly17: Vec<u8>,
}

impl Default for SapApuImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl SapApuImpl {
    pub fn new() -> Self {
        let poly4 = gen_poly(poly_mask!(4, 1, 0), POLY4_LEN / 8 + 1);
        let poly9 = gen_poly(poly_mask!(9, 5, 0), POLY9_LEN / 8 + 1);
        let poly17 = gen_poly(poly_mask!(17, 5, 0), POLY17_LEN / 8 + 1);
        Self {
            synth: BlipSynth::new(BLIP_GOOD_QUALITY),
            poly4,
            poly9,
            poly17,
        }
    }

    pub fn volume(&mut self, gain: f64) {
        self.synth.volume(1.0 / OSC_COUNT as f64 / 30.0 * gain, 1);
    }
}

/// POKEY sound chip.
pub struct SapApu {
    pub oscs: [Osc; OSC_COUNT],
    impl_ref: Option<*mut SapApuImpl>,
    pub last_time: BlipTime,
    poly5_pos: usize,
    poly4_pos: usize,
    polym_pos: usize,
    control: u8,
}

// SAFETY: SapApu's raw pointers (impl_ref, osc output) point to data owned by SapEmu.
unsafe impl Send for SapApu {}

impl Default for SapApu {
    fn default() -> Self {
        Self::new()
    }
}

impl SapApu {
    pub fn new() -> Self {
        let mut oscs = [Osc::new(), Osc::new(), Osc::new(), Osc::new()];
        for osc in &mut oscs {
            osc.output = None;
        }
        Self {
            oscs,
            impl_ref: None,
            last_time: 0,
            poly5_pos: 0,
            poly4_pos: 0,
            polym_pos: 0,
            control: 0,
        }
    }

    pub fn reset(&mut self, apu_impl: *mut SapApuImpl) {
        self.impl_ref = Some(apu_impl);
        self.last_time = 0;
        self.poly5_pos = 0;
        self.poly4_pos = 0;
        self.polym_pos = 0;
        self.control = 0;
        for osc in &mut self.oscs {
            osc.regs = [0; 2];
            osc.phase = 0;
            osc.invert = 0;
            osc.last_amp = 0;
            osc.delay = 0;
            osc.period = 0;
            // Keep output pointer
        }
    }

    pub fn osc_output(&mut self, index: usize, buf: Option<*mut BlipBuffer>) {
        debug_assert!(index < OSC_COUNT);
        self.oscs[index].output = buf;
    }

    fn calc_periods(&mut self) {
        let divider = if self.control & 1 != 0 { 114 } else { 28 };
        let fast_bits: [u8; OSC_COUNT] = [1 << 6, 1 << 4, 1 << 5, 1 << 3];

        for i in 0..OSC_COUNT {
            let osc_reload = self.oscs[i].regs[0] as i32;
            let mut period = (osc_reload + 1) * divider;

            if self.control & fast_bits[i] != 0 {
                period = osc_reload + 4;
                if i & 1 != 0 {
                    period = osc_reload * 0x100 + self.oscs[i - 1].regs[0] as i32 + 7;
                    if self.control & fast_bits[i - 1] == 0 {
                        period = (period - 6) * divider;
                    }
                }
            }
            self.oscs[i].period = period;
        }
    }

    fn run_until(&mut self, end_time: BlipTime) {
        self.calc_periods();
        let apu_impl = match self.impl_ref {
            Some(p) => unsafe { &mut *p },
            None => return,
        };

        // 17/9-bit poly selection
        let (polym, polym_len) = if self.control & 0x80 != 0 {
            (&apu_impl.poly9[..], POLY9_LEN)
        } else {
            (&apu_impl.poly17[..], POLY17_LEN)
        };
        // C++ stores the reduced value back: polym_pos %= polym_len;
        // (matters if the 9/17-bit select bit changes between calls)
        self.polym_pos %= polym_len;
        let polym_pos = self.polym_pos;

        for i in 0..OSC_COUNT {
            let mut time = self.last_time + self.oscs[i].delay;
            let period = self.oscs[i].period;

            if let Some(output) = self.oscs[i].output {
                unsafe {
                    (*output).set_modified();
                }

                let osc_control = self.oscs[i].regs[1];
                let mut volume = ((osc_control & 0x0F) * 2) as i32;

                let is_silent = volume == 0
                    || osc_control & 0x10 != 0
                    || ((osc_control & 0xA0) == 0xA0 && period < 1789773 / 2 / MAX_FREQUENCY);

                if is_silent {
                    // Silent, DAC mode, or inaudible frequency
                    if osc_control & 0x10 == 0 {
                        volume >>= 1; // inaudible = half volume
                    }
                    let delta = volume - self.oscs[i].last_amp;
                    if delta != 0 {
                        self.oscs[i].last_amp = volume;
                        unsafe {
                            apu_impl
                                .synth
                                .offset_at(self.last_time, delta, &mut *output);
                        }
                    }
                } else {
                    // Active oscillator - generate audio
                    // High pass
                    let hipass_bits: [u8; OSC_COUNT] = [1 << 2, 1 << 1, 0, 0];
                    let mut period2: BlipTime = 0;
                    let mut time2 = end_time;
                    if self.control & hipass_bits[i] != 0 {
                        period2 = self.oscs[2].period;
                        time2 = self.last_time + self.oscs[2].delay;
                        if self.oscs[i].invert != 0 {
                            self.oscs[i].last_amp -= volume;
                            volume = -volume;
                        }
                    }

                    if time < end_time || time2 < end_time {
                        // Poly source
                        static POLY1: [u8; 2] = [0x55, 0x55];
                        let mut poly = &POLY1[..];
                        let mut poly_len = 8 * POLY1.len();
                        let mut poly_pos = (self.oscs[i].phase & 1) as usize;
                        let mut poly_inc = 1isize;
                        if osc_control & 0x20 == 0 {
                            poly = polym;
                            poly_len = polym_len;
                            poly_pos = polym_pos;
                            let p4 = &apu_impl.poly4[..];
                            let p4_len = POLY4_LEN;
                            let p4_pos = self.poly4_pos;
                            if osc_control & 0x40 != 0 {
                                poly = p4;
                                poly_len = p4_len;
                                poly_pos = p4_pos;
                            }
                            poly_inc = (period as usize % poly_len) as isize;
                            poly_pos = (poly_pos + self.oscs[i].delay as usize) % poly_len;
                        }
                        poly_inc -= poly_len as isize;

                        // Square/poly5 wave
                        let mut wave = POLY5;
                        let mut poly5_inc = 0usize;
                        if osc_control & 0x80 == 0 {
                            wave = run_poly5(
                                wave,
                                (self.oscs[i].delay as usize + self.poly5_pos) % POLY5_LEN,
                            );
                            poly5_inc = period as usize % POLY5_LEN;
                        }

                        // Run wave and high pass interleaved
                        let mut osc_last_amp = self.oscs[i].last_amp;
                        loop {
                            // Run high pass
                            if time2 < time {
                                let mut delta = -osc_last_amp;
                                if volume < 0 {
                                    delta += volume;
                                }
                                if delta != 0 {
                                    osc_last_amp += delta - volume;
                                    volume = -volume;
                                    unsafe {
                                        apu_impl.synth.offset_at(time2, delta, &mut *output);
                                    }
                                }
                            }
                            while time2 <= time {
                                time2 += period2;
                            }

                            // Run wave
                            let mut end = end_time;
                            if end > time2 {
                                end = time2;
                            }
                            while time < end {
                                if wave & 1 != 0 {
                                    let amp =
                                        volume & -(get_poly_bit(poly, poly_pos, poly_len) as i32);
                                    let new_poly_pos = (poly_pos as isize).wrapping_add(poly_inc);
                                    if new_poly_pos < 0 {
                                        poly_pos = (new_poly_pos + poly_len as isize) as usize;
                                    } else {
                                        poly_pos = new_poly_pos as usize;
                                    }
                                    let delta = amp - osc_last_amp;
                                    if delta != 0 {
                                        osc_last_amp = amp;
                                        unsafe {
                                            apu_impl.synth.offset_at(time, delta, &mut *output);
                                        }
                                    }
                                }
                                wave = run_poly5(wave, poly5_inc);
                                time += period;
                            }
                            if time >= end_time && time2 >= end_time {
                                break;
                            }
                        }

                        self.oscs[i].phase = poly_pos as u8;
                        self.oscs[i].last_amp = osc_last_amp;
                    }

                    self.oscs[i].invert = 0;
                    if volume < 0 {
                        self.oscs[i].last_amp -= volume;
                        self.oscs[i].invert = 1;
                    }
                }
            }

            // Maintain divider
            let remain = end_time - time;
            if remain > 0 {
                let count = (remain + period - 1) / period;
                self.oscs[i].phase ^= count as u8;
                time += count * period;
            }
            self.oscs[i].delay = time - end_time;
        }

        // Advance polies
        let duration = end_time - self.last_time;
        self.last_time = end_time;
        self.poly4_pos = (self.poly4_pos + duration as usize) % POLY4_LEN;
        self.poly5_pos = (self.poly5_pos + duration as usize) % POLY5_LEN;
        self.polym_pos += duration as usize;
    }

    pub fn write_data(&mut self, time: BlipTime, addr: u32, data: u8) {
        self.run_until(time);
        let i = ((addr ^ 0xD200) >> 1) as usize;
        if i < OSC_COUNT {
            self.oscs[i].regs[(addr & 1) as usize] = data;
        } else if addr == 0xD208 {
            self.control = data;
        } else if addr == 0xD209 {
            for osc in &mut self.oscs {
                osc.delay = 0;
            }
        }
    }

    pub fn read_reg(&self, reg: usize) -> u8 {
        if reg < OSC_COUNT * 2 {
            self.oscs[reg / 2].regs[reg % 2]
        } else if reg == 8 {
            self.control
        } else {
            0
        }
    }

    pub fn end_frame(&mut self, end_time: BlipTime) {
        if end_time > self.last_time {
            self.run_until(end_time);
        }
        self.last_time -= end_time;
    }
}

/// Get a bit from a poly sequence at a given position.
#[inline]
fn get_poly_bit(poly: &[u8], pos: usize, _len: usize) -> u8 {
    (poly[pos >> 3] >> (pos & 7)) & 1
}
