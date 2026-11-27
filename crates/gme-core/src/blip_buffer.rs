// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Blip_Buffer.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use std::f64::consts::PI;

pub type BlipLong = i32;
pub type BlipTime = i32;
pub type BlipSample = i16;
pub type BlipResampledTime = u32;

pub const BLIP_SAMPLE_MAX: i16 = 32767;
pub const BLIP_SAMPLE_BITS: i32 = 30;
pub const BLIP_BUFFER_ACCURACY: i32 = 16;
pub const BLIP_PHASE_BITS: i32 = 6;
pub const BLIP_RES: usize = 1 << BLIP_PHASE_BITS;
pub const BLIP_WIDEST_IMPULSE: usize = 16;
pub const BLIP_BUFFER_EXTRA: usize = BLIP_WIDEST_IMPULSE + 2;

pub const BLIP_MED_QUALITY: usize = 8;
pub const BLIP_GOOD_QUALITY: usize = 12;
pub const BLIP_HIGH_QUALITY: usize = 16;

pub const BLIP_MAX_LENGTH: i32 = 0;
pub const BLIP_DEFAULT_LENGTH: i32 = 250;
pub const BLIP_READER_DEFAULT_BASS: i32 = 9;

pub struct BlipBuffer {
    pub buffer: Vec<i32>,
    pub buffer_size: i32,
    pub factor: u32,
    pub offset: u32,
    pub read_pos: i32,
    pub bass_shift: i32,
    pub reader_accum: i32,
    sample_rate: i32,
    clock_rate_val: u32,
    bass_freq_val: i32,
    length: i32,
    modified: i32,
}

impl Default for BlipBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl BlipBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            buffer_size: 0,
            factor: u32::MAX / 2,
            offset: 0,
            read_pos: 0,
            bass_shift: 0,
            reader_accum: 0,
            sample_rate: 0,
            clock_rate_val: 0,
            bass_freq_val: 16,
            length: 0,
            modified: 0,
        }
    }

    pub fn set_sample_rate(&mut self, samples_per_sec: i32, msec: i32) -> Result<(), &'static str> {
        let max_size = (u32::MAX >> BLIP_BUFFER_ACCURACY) as i32 - BLIP_BUFFER_EXTRA as i32 - 64;
        let mut new_size = max_size;
        if msec != BLIP_MAX_LENGTH {
            let s = (samples_per_sec as i64 * (msec as i64 + 1) + 999) / 1000;
            if s < new_size as i64 {
                new_size = s as i32;
            }
        }

        // Allocate double the buffer size to handle writes that span frame boundaries.
        // C++ relies on undefined behavior here; Rust bounds-checks, so we need extra space.
        self.buffer
            .resize((new_size * 2 + BLIP_BUFFER_EXTRA as i32) as usize, 0);
        self.buffer_size = new_size;

        self.sample_rate = samples_per_sec;
        self.length = new_size * 1000 / samples_per_sec - 1;
        if self.clock_rate_val != 0 {
            self.clock_rate(self.clock_rate_val);
        }
        self.bass_freq(self.bass_freq_val);
        self.clear(true);
        Ok(())
    }

    pub fn clock_rate(&mut self, rate: u32) {
        self.clock_rate_val = rate;
        self.factor = self.clock_rate_factor(rate);
    }

    #[inline]
    pub fn clock_rate_value(&self) -> u32 {
        self.clock_rate_val
    }

    #[inline]
    pub fn end_frame(&mut self, time: BlipTime) {
        let advance = (time as u64).wrapping_mul(self.factor as u64);
        self.offset = self.offset.wrapping_add(advance as u32);
    }

    pub fn read_samples(&mut self, dest: &mut [i16], max_samples: usize) -> usize {
        let avail = self.samples_avail() as usize;
        let count = avail.min(max_samples);
        if count > 0 {
            for (d, &s) in dest.iter_mut().zip(self.buffer.iter()).take(count) {
                *d = s as i16;
            }
            self.remove_samples(count as i64);
        }
        count
    }

    pub fn bass_freq(&mut self, freq: i32) {
        self.bass_freq_val = freq;
        let mut shift = 31;
        if freq > 0 {
            shift = 13;
            let f = ((freq as i64) << 16) / self.sample_rate as i64;
            let mut f = f;
            // Match C++ bass_freq: decrement shift AFTER right-shifting f
            loop {
                f >>= 1;
                if f == 0 || shift == 0 {
                    break;
                }
                shift -= 1;
            }
        }
        self.bass_shift = shift;
    }

    pub fn output_latency(&self) -> i32 {
        BLIP_WIDEST_IMPULSE as i32 / 2
    }

    pub fn clear(&mut self, entire_buffer: bool) {
        self.offset = 0;
        self.read_pos = 0;
        self.modified = 0;
        self.reader_accum = 0;
        if !self.buffer.is_empty() {
            let count = if entire_buffer {
                self.buffer_size as usize
            } else {
                self.samples_avail() as usize
            };
            self.buffer[..count + BLIP_BUFFER_EXTRA].fill(0);
        }
    }

    #[inline]
    pub fn samples_avail(&self) -> i64 {
        // Match C++: offset is adjusted on removal so it stays bounded
        let samples = (self.offset >> BLIP_BUFFER_ACCURACY) as i32 as i64;
        if samples <= self.buffer_size as i64 {
            samples
        } else {
            0
        }
    }

    pub fn remove_samples(&mut self, count: i64) {
        if count > 0 {
            // Match C++: adjust offset (like remove_silence), then shift buffer
            self.remove_silence(count);
            let remain = self.samples_avail() as usize + BLIP_BUFFER_EXTRA;
            let c = count as usize;
            self.buffer.copy_within(c..c + remain, 0);
            for i in remain..remain + c {
                if i < self.buffer.len() {
                    self.buffer[i] = 0;
                }
            }
        }
    }

    pub fn remove_silence(&mut self, count: i64) {
        // Match C++ Blip_Buffer::remove_silence: adjust offset
        self.offset = self
            .offset
            .wrapping_sub((count as u32) << BLIP_BUFFER_ACCURACY);
    }

    pub fn sample_rate(&self) -> i32 {
        self.sample_rate
    }
    pub fn length_ms(&self) -> i32 {
        self.length
    }

    #[inline]
    pub fn set_modified(&mut self) {
        self.modified = 1;
    }
    #[inline]
    pub fn clear_modified(&mut self) -> i32 {
        let b = self.modified;
        self.modified = 0;
        b
    }

    #[inline]
    pub fn resampled_duration(&self, t: i32) -> u32 {
        t as u32 * self.factor
    }
    #[inline]
    pub fn resampled_time(&self, t: BlipTime) -> u32 {
        t as u32 * self.factor + self.offset
    }

    pub fn count_clocks(&self, count: i64) -> BlipTime {
        if self.factor == 0 {
            return 0;
        }
        // C++ does this math in 32-bit unsigned with wrapping
        let count = count.min(self.buffer_size as i64) as u32;
        let time = count << BLIP_BUFFER_ACCURACY;
        (time
            .wrapping_sub(self.offset)
            .wrapping_add(self.factor.wrapping_sub(1))
            / self.factor) as BlipTime
    }

    pub fn count_samples(&self, t: BlipTime) -> i64 {
        let last = self.resampled_time(t) >> BLIP_BUFFER_ACCURACY;
        let first = self.offset >> BLIP_BUFFER_ACCURACY;
        (last - first) as i64
    }

    pub fn mix_samples(&mut self, input: &[i16]) {
        if self.buffer.is_empty() {
            return;
        }
        let mut idx = (self.offset >> BLIP_BUFFER_ACCURACY) as usize + BLIP_WIDEST_IMPULSE / 2;
        let shift = BLIP_SAMPLE_BITS - 16;
        let mut prev = 0i32;
        for &s in input {
            let s = (s as i32) << shift;
            self.buffer[idx] += s - prev;
            prev = s;
            idx += 1;
        }
        self.buffer[idx] -= prev;
    }

    pub fn clock_rate_factor(&self, rate: u32) -> u32 {
        if rate == 0 || self.sample_rate == 0 {
            return 0;
        }
        let ratio = self.sample_rate as f64 / rate as f64;
        (ratio * (1u64 << BLIP_BUFFER_ACCURACY) as f64 + 0.5) as u32
    }
}

pub struct SilentBlipBuffer {
    _buf: [i32; BLIP_BUFFER_EXTRA + 1],
}

impl Default for SilentBlipBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl SilentBlipBuffer {
    pub fn new() -> Self {
        Self {
            _buf: [0; BLIP_BUFFER_EXTRA + 1],
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlipEq {
    treble: f64,
    rolloff_freq: i64,
    sample_rate: i64,
    cutoff_freq: i64,
}

impl BlipEq {
    pub fn new(treble_db: f64) -> Self {
        Self {
            treble: treble_db,
            rolloff_freq: 0,
            sample_rate: 44100,
            cutoff_freq: 0,
        }
    }

    pub fn with_params(treble: f64, rolloff_freq: i64, sample_rate: i64, cutoff_freq: i64) -> Self {
        Self {
            treble,
            rolloff_freq,
            sample_rate,
            cutoff_freq,
        }
    }

    pub fn generate(&self, out: &mut [f64]) {
        let count = out.len();
        let oversample = if self.cutoff_freq > 0 {
            self.sample_rate as f64 * 0.5 / self.cutoff_freq as f64
        } else {
            BLIP_RES as f64 * 2.25 / count as f64 + 0.85
        };
        let half_rate = self.sample_rate as f64 * 0.5;
        let cutoff = self.rolloff_freq as f64 * oversample / half_rate;

        // Use f32 buffers to match C++ Blip_Buffer.cpp exactly.
        let mut f32_out = vec![0.0f32; count];
        gen_sinc_f32(
            &mut f32_out,
            count,
            BLIP_RES as f64 * oversample,
            self.treble,
            cutoff,
        );
        let to_fraction = PI / (count as f64 - 1.0);
        for (i, f) in f32_out.iter_mut().enumerate().take(count) {
            // C++ does: 0.54f - 0.46f * (float) cos(i * to_fraction)
            let val = 0.54f32 - 0.46f32 * ((i as f64) * to_fraction).cos() as f32;
            *f *= val;
        }
        for (o, &f) in out.iter_mut().zip(f32_out.iter()).take(count) {
            *o = f as f64;
        }
    }
}

fn gen_sinc_f32(out: &mut [f32], count: usize, oversample: f64, treble: f64, cutoff: f64) {
    let cutoff = if cutoff >= 0.999 { 0.999 } else { cutoff };
    let treble = treble.clamp(-300.0, 5.0);
    let maxh = 4096.0;
    let rolloff = 10.0f64.powf(1.0 / (maxh * 20.0) * treble / (1.0 - cutoff));
    let pow_a_n = rolloff.powf(maxh - maxh * cutoff);
    let to_angle = PI / 2.0 / maxh / oversample;

    for (i, o) in out.iter_mut().enumerate().take(count) {
        let angle = ((i as f64 - count as f64) * 2.0 + 1.0) * to_angle;
        let angle_maxh = angle * maxh;
        let angle_maxh_mid = angle_maxh * cutoff;
        let mut y = maxh;
        if angle_maxh_mid != 0.0 {
            y *= angle_maxh_mid.sin() / angle_maxh_mid;
        }
        let cosa = angle.cos();
        let den = 1.0 + rolloff * (rolloff - cosa - cosa);
        if den > 1e-13 {
            let num = ((angle_maxh - angle).cos() * rolloff - angle_maxh.cos()) * pow_a_n
                - (angle_maxh_mid - angle).cos() * rolloff
                + angle_maxh_mid.cos();
            y = y * cutoff + num / den;
        }
        *o = y as f32;
    }
}

/// Band-limited waveform synthesizer.
pub struct BlipSynth {
    pub buf: Option<*mut BlipBuffer>,
    pub last_amp: i32,
    pub delta_factor: i32,
    volume_unit_val: f64,
    kernel_unit: i32,
    impulses: Vec<i16>,
    quality: usize,
}

impl BlipSynth {
    pub fn new(quality: usize) -> Self {
        let impulses_size = BLIP_RES * (quality / 2) + 1;
        Self {
            buf: None,
            last_amp: 0,
            delta_factor: 0,
            volume_unit_val: 0.0,
            kernel_unit: 0,
            impulses: vec![0; impulses_size],
            quality,
        }
    }

    pub fn volume(&mut self, v: f64, range: i32) {
        let range = if range < 0 { -range } else { range };
        self.volume_unit(v * (1.0 / range as f64));
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        let width = self.quality;
        let half_size = BLIP_RES / 2 * (width - 1);
        let impulses_size = BLIP_RES * (width / 2) + 1;

        let mut fimpulse = vec![0.0f64; BLIP_RES + half_size + BLIP_RES];
        eq.generate(&mut fimpulse[BLIP_RES..BLIP_RES + half_size]);

        for i in 0..BLIP_RES {
            fimpulse[BLIP_RES + half_size + i] = fimpulse[BLIP_RES + half_size - 1 - i];
        }
        for f in fimpulse.iter_mut().take(BLIP_RES) {
            *f = 0.0;
        }

        let mut total = 0.0f64;
        for i in 0..half_size {
            total += fimpulse[BLIP_RES + i];
        }

        let base_unit = 32768.0f64;
        let rescale = base_unit / 2.0 / total;
        self.kernel_unit = base_unit as i32;

        let mut sum = 0.0f64;
        let mut next = 0.0f64;
        for i in 0..impulses_size {
            self.impulses[i] = ((next - sum) * rescale + 0.5).floor() as i16;
            sum += fimpulse[i];
            next += fimpulse[i + BLIP_RES];
        }
        self.adjust_impulse(impulses_size);

        let vol = self.volume_unit_val;
        if vol != 0.0 {
            self.volume_unit_val = 0.0;
            self.volume_unit(vol);
        }
    }

    fn adjust_impulse(&mut self, impulses_size: usize) {
        // Sum pairs for each phase and add error correction to end of first half.
        // C++ runs p from blip_res-1 down to blip_res/2 - 1 inclusive; p2 may be -1
        // (indexing impulses[i - 1], valid since i starts at 1).
        let mut p = BLIP_RES as i32 - 1;
        while p >= BLIP_RES as i32 / 2 - 1 {
            let p2 = BLIP_RES as i32 - 2 - p;
            let mut error = self.kernel_unit as i64;
            let mut i = 1i32;
            while (i as usize) < impulses_size {
                error -= self.impulses[(i + p) as usize] as i64;
                error -= self.impulses[(i + p2) as usize] as i64;
                i += BLIP_RES as i32;
            }
            if p == p2 {
                error /= 2; // phase = 0.5 impulse uses same half for both sides
            }
            let idx = impulses_size - BLIP_RES + p as usize;
            self.impulses[idx] = self.impulses[idx].wrapping_add(error as i16);
            p -= 1;
        }
    }

    pub fn volume_unit(&mut self, new_unit: f64) {
        if new_unit == self.volume_unit_val {
            return;
        }
        if self.kernel_unit == 0 {
            self.treble_eq(&BlipEq::new(-8.0));
        }
        self.volume_unit_val = new_unit;
        let mut factor = new_unit * (1i64 << BLIP_SAMPLE_BITS) as f64 / self.kernel_unit as f64;
        if factor > 0.0 {
            let mut shift = 0;
            while factor < 2.0 {
                shift += 1;
                factor *= 2.0;
            }
            if shift > 0 {
                self.kernel_unit >>= shift;
                let offset = 0x8000 + (1 << (shift - 1));
                let offset2 = 0x8000 >> shift;
                let sz = self.impulses.len();
                for impulse in &mut self.impulses {
                    *impulse = (((*impulse as i32 + offset) >> shift) - offset2) as i16;
                }
                self.adjust_impulse(sz);
            }
        }
        self.delta_factor = (factor + 0.5).floor() as i32;
    }

    #[inline]
    pub fn set_output(&mut self, buf: Option<&mut BlipBuffer>) {
        self.buf = buf.map(|b| b as *mut BlipBuffer);
        self.last_amp = 0;
    }

    #[inline]
    pub fn update(&mut self, time: BlipTime, amp: i32) {
        let delta = amp - self.last_amp;
        self.last_amp = amp;
        if let Some(buf_ptr) = self.buf {
            unsafe {
                let buf = &mut *buf_ptr;
                let resampled = time as u32 * buf.factor + buf.offset;
                self.offset_resampled_impl(resampled, delta, buf);
            }
        }
    }

    #[inline]
    pub fn offset_resampled(&self, time: u32, delta: i32, buf: &mut BlipBuffer) {
        let delta = delta * self.delta_factor;
        // With offset adjusted on removal (C++ approach), buf_offset = time >> ACCURACY directly
        let buf_offset = (time >> BLIP_BUFFER_ACCURACY) as usize;
        let phase =
            ((time >> (BLIP_BUFFER_ACCURACY - BLIP_PHASE_BITS)) & (BLIP_RES as u32 - 1)) as usize;
        let quality = self.quality;
        let fwd = (BLIP_WIDEST_IMPULSE - quality) / 2;
        let rev = fwd + quality - 2;
        let mid = quality / 2 - 1;
        let imp_base = &self.impulses;
        let mut imp_off = BLIP_RES - phase;

        // SAFETY: buf_offset is within the buffer (time is resampled from a valid
        // clock), and the impulse indices are bounded by quality <= BLIP_WIDEST_IMPULSE.
        // The buffer is allocated with 2*buffer_size + EXTRA elements.
        unsafe {
            let buf_ptr = buf.buffer.as_mut_ptr().add(buf_offset);

            macro_rules! add_imp {
                ($out:expr, $in:expr) => {
                    *buf_ptr.add($out) = (*buf_ptr.add($out)).wrapping_add(
                        *imp_base.get_unchecked(imp_off + BLIP_RES * $in) as i32 * delta,
                    );
                };
            }
            macro_rules! blip_fwd {
                ($i:expr) => {
                    add_imp!(fwd + $i, $i);
                    add_imp!(fwd + 1 + $i, $i + 1);
                };
            }
            macro_rules! blip_rev {
                ($r:expr) => {
                    add_imp!(rev - $r, $r + 1);
                    add_imp!(rev + 1 - $r, $r);
                };
            }

            blip_fwd!(0);
            if quality > 8 {
                blip_fwd!(2);
            }
            if quality > 12 {
                blip_fwd!(4);
            }
            {
                add_imp!(fwd + mid - 1, mid - 1);
                add_imp!(fwd + mid, mid);
                imp_off = phase;
            }
            if quality > 12 {
                blip_rev!(6);
            }
            if quality > 8 {
                blip_rev!(4);
            }
            blip_rev!(2);
            add_imp!(rev, 1);
            add_imp!(rev + 1, 0);
        }
    }

    #[inline]
    fn offset_resampled_impl(&self, time: u32, delta: i32, buf: &mut BlipBuffer) {
        self.offset_resampled(time, delta, buf);
    }

    /// Add amplitude transition at given time. Converts to resampled time internally.
    #[inline]
    pub fn offset_at(&self, time: BlipTime, delta: i32, buf: &mut BlipBuffer) {
        let resampled = (time as u32)
            .wrapping_mul(buf.factor)
            .wrapping_add(buf.offset);
        self.offset_resampled(resampled, delta, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blip_buffer_basic() {
        let mut buf = BlipBuffer::new();
        buf.set_sample_rate(44100, 1000).unwrap();
        buf.clock_rate(1000000);
        assert_eq!(buf.sample_rate(), 44100);
        assert_eq!(buf.clock_rate_value(), 1000000);
    }

    #[test]
    fn test_blip_buffer_samples_avail() {
        let mut buf = BlipBuffer::new();
        buf.set_sample_rate(44100, 1000).unwrap();
        buf.clock_rate(44100);
        assert_eq!(buf.samples_avail(), 0);
        buf.end_frame(44100);
        assert_eq!(buf.samples_avail(), 44100);
    }

    #[test]
    fn test_blip_buffer_read_samples() {
        let mut buf = BlipBuffer::new();
        buf.set_sample_rate(44100, 1000).unwrap();
        buf.clock_rate(44100);
        buf.end_frame(100);
        let mut out = vec![0i16; 100];
        let n = buf.read_samples(&mut out, 100);
        assert_eq!(n, 100);
    }
}
