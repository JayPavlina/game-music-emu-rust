use std::f64::consts::PI;

const STEREO: usize = 2;
const MAX_RES: usize = 32;

/// FIR resampler core (width-agnostic).
pub struct FirResamplerCore {
    buf: Vec<i16>,
    write_pos: usize,
    res: usize,
    imp_phase: usize,
    width: usize,
    write_offset: usize,
    skip_bits: u32,
    step: usize,
    input_per_cycle: usize,
    ratio: f64,
    impulses: Vec<i16>,
}

impl FirResamplerCore {
    pub fn new(width: usize) -> Self {
        let write_offset = width * STEREO - STEREO;
        Self {
            buf: Vec::new(),
            write_pos: 0,
            res: 1,
            imp_phase: 0,
            width,
            write_offset,
            skip_bits: 0,
            step: STEREO,
            input_per_cycle: 0,
            ratio: 1.0,
            impulses: vec![0; MAX_RES * width],
        }
    }

    pub fn time_ratio(&mut self, new_factor: f64, rolloff: f64, gain: f64) -> f64 {
        self.ratio = new_factor;

        let mut fstep = 0.0f64;
        let mut least_error = 2.0f64;
        let mut pos = 0.0f64;
        self.res = 0;
        for r in 1..=MAX_RES {
            pos += self.ratio;
            let nearest = (pos + 0.5).floor();
            let error = (pos - nearest).abs();
            if error < least_error {
                self.res = r;
                fstep = nearest / r as f64;
                least_error = error;
            }
        }

        self.skip_bits = 0;
        self.step = STEREO * fstep.floor() as usize;
        self.ratio = fstep;
        fstep %= 1.0;

        let filter = if self.ratio < 1.0 {
            1.0
        } else {
            1.0 / self.ratio
        };
        let mut pos = 0.0f64;
        self.input_per_cycle = 0;
        for i in 0..self.res {
            gen_sinc_impl(
                rolloff,
                ((self.width as f64 * filter + 1.0) as usize) & !1,
                pos,
                filter,
                0x7FFF as f64 * gain * filter,
                self.width,
                &mut self.impulses[i * self.width..(i + 1) * self.width],
            );

            pos += fstep;
            self.input_per_cycle += self.step;
            if pos >= 0.9999999 {
                pos -= 1.0;
                self.skip_bits |= 1 << i;
                self.input_per_cycle += 1;
            }
        }

        self.clear();
        self.ratio
    }

    pub fn clear(&mut self) {
        self.imp_phase = 0;
        if !self.buf.is_empty() {
            self.write_pos = self.write_offset;
            for i in 0..self.write_offset {
                self.buf[i] = 0;
            }
        }
    }

    pub fn buffer_size(&mut self, new_size: usize) {
        self.buf.resize(new_size + self.write_offset, 0);
        self.clear();
    }

    pub fn max_write(&self) -> usize {
        if self.buf.is_empty() {
            0
        } else {
            self.buf.len() - self.write_pos
        }
    }

    pub fn buffer_ptr(&mut self) -> &mut [i16] {
        &mut self.buf[self.write_pos..]
    }

    pub fn write(&mut self, count: usize) {
        self.write_pos += count;
        debug_assert!(self.write_pos <= self.buf.len());
    }

    pub fn written(&self) -> usize {
        self.write_pos - self.write_offset
    }

    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    pub fn avail(&self, input_count: i32) -> i32 {
        let input_count = input_count as usize;
        let write_offset = self.width * STEREO;
        if input_count < write_offset {
            return 0;
        }
        let effective = input_count - write_offset;
        self.avail_internal(effective as i32)
    }

    fn avail_internal(&self, input_count: i32) -> i32 {
        if self.input_per_cycle == 0 {
            return 0;
        }
        let cycle_count = input_count as usize / self.input_per_cycle;
        let mut output_count = cycle_count * self.res * STEREO;
        let mut input_remaining = input_count as usize - cycle_count * self.input_per_cycle;

        let mut skip = self.skip_bits >> self.imp_phase;
        let mut remain = self.res - self.imp_phase;
        while input_remaining > 0 {
            input_remaining =
                input_remaining.saturating_sub(self.step + (skip & 1) as usize * STEREO);
            skip >>= 1;
            remain -= 1;
            if remain == 0 {
                skip = self.skip_bits;
                remain = self.res;
            }
            output_count += STEREO;
        }
        output_count as i32
    }

    pub fn input_needed(&self, output_count: i32) -> i32 {
        let mut input_count = 0usize;
        let mut skip = self.skip_bits >> self.imp_phase;
        let mut remain = self.res - self.imp_phase;
        let mut out_remaining = output_count as usize;

        while out_remaining > STEREO {
            input_count += self.step + (skip & 1) as usize * STEREO;
            skip >>= 1;
            remain -= 1;
            if remain == 0 {
                skip = self.skip_bits;
                remain = self.res;
            }
            out_remaining -= STEREO;
        }

        let input_extra =
            input_count as i64 - (self.write_pos as i64 - ((self.width - 1) * STEREO) as i64);
        input_extra.max(0) as i32
    }

    pub fn skip_input(&mut self, count: i64) -> usize {
        let remain = self.write_pos;
        let max_count = remain.saturating_sub(self.width * STEREO);
        let count = (count as usize).min(max_count);
        let new_remain = remain - count;
        self.buf.copy_within(count..count + new_remain, 0);
        self.write_pos = new_remain;
        count
    }
}

fn gen_sinc_impl(
    rolloff: f64,
    width: usize,
    offset: f64,
    spacing: f64,
    scale: f64,
    count: usize,
    out: &mut [i16],
) {
    let maxh = 256.0f64;
    let step = PI / maxh * spacing;
    let to_w = maxh * 2.0 / width as f64;
    let pow_a_n = rolloff.powf(maxh);
    let scale = scale / (maxh * 2.0);

    let mut angle = ((count as f64 / 2.0 - 1.0) + offset) * -step;
    for o in out.iter_mut().take(count) {
        *o = 0;
        let w = angle * to_w;
        if w.abs() < PI {
            let rolloff_cos_a = rolloff * angle.cos();
            let num = 1.0 - rolloff_cos_a - pow_a_n * (maxh * angle).cos()
                + pow_a_n * rolloff * ((maxh - 1.0) * angle).cos();
            let den = 1.0 - rolloff_cos_a - rolloff_cos_a + rolloff * rolloff;
            let sinc = scale * num / den - scale;
            *o = (w.cos() * sinc + sinc) as i16;
        }
        angle += step;
    }
}

/// FIR resampler with fixed width.
pub struct FirResampler<const WIDTH: usize> {
    core: FirResamplerCore,
}

impl<const WIDTH: usize> Default for FirResampler<WIDTH> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const WIDTH: usize> FirResampler<WIDTH> {
    pub fn new() -> Self {
        Self {
            core: FirResamplerCore::new(WIDTH),
        }
    }

    pub fn time_ratio(&mut self, factor: f64, rolloff: f64, gain: f64) -> f64 {
        self.core.time_ratio(factor, rolloff, gain)
    }

    pub fn clear(&mut self) {
        self.core.clear();
    }

    pub fn buffer_size(&mut self, size: usize) {
        self.core.buffer_size(size);
    }

    pub fn max_write(&self) -> usize {
        self.core.max_write()
    }

    pub fn buffer_ptr(&mut self) -> &mut [i16] {
        self.core.buffer_ptr()
    }

    pub fn write(&mut self, count: usize) {
        self.core.write(count);
    }

    pub fn written(&self) -> usize {
        self.core.written()
    }

    pub fn ratio(&self) -> f64 {
        self.core.ratio()
    }

    pub fn avail(&self, input_count: i32) -> i32 {
        self.core.avail(input_count)
    }

    pub fn input_needed(&self, output_count: i32) -> i32 {
        self.core.input_needed(output_count)
    }

    pub fn skip_input(&mut self, count: i64) -> usize {
        self.core.skip_input(count)
    }

    /// Read resampled output samples. Returns number of samples read (stereo pairs * 2).
    pub fn read(&mut self, out: &mut [i16]) -> i32 {
        let width = WIDTH;
        let step = self.core.step;
        let skip_bits = self.core.skip_bits;
        let ratio1 = self.core.ratio - 1.0;
        let should_resample = ratio1.abs() >= 0.00001;

        let mut out_pos = 0;
        let mut in_pos = 0;
        let mut imp_phase = self.core.imp_phase;
        let write_pos = self.core.write_pos;

        if write_pos >= width * STEREO {
            let end_limit = write_pos - width * STEREO;
            let mut count = out.len() / 2;

            while in_pos <= end_limit && count > 0 {
                count -= 1;

                if !should_resample {
                    out[out_pos] = self.core.buf[in_pos];
                    out[out_pos + 1] = self.core.buf[in_pos + 1];
                } else {
                    let mut l = 0i32;
                    let mut r = 0i32;
                    let imp_start = imp_phase * width;
                    let mut imp_idx = imp_start;
                    let mut i_idx = in_pos;

                    // SAFETY: imp_idx < res * width, i_idx + 3 < write_pos,
                    // both buffers are allocated with sufficient size.
                    unsafe {
                        for _ in 0..width / 2 {
                            let pt0 = *self.core.impulses.get_unchecked(imp_idx) as i32;
                            l += pt0 * *self.core.buf.get_unchecked(i_idx) as i32;
                            r += pt0 * *self.core.buf.get_unchecked(i_idx + 1) as i32;
                            let pt1 = *self.core.impulses.get_unchecked(imp_idx + 1) as i32;
                            imp_idx += 2;
                            l += pt1 * *self.core.buf.get_unchecked(i_idx + 2) as i32;
                            r += pt1 * *self.core.buf.get_unchecked(i_idx + 3) as i32;
                            i_idx += 4;
                        }
                    }

                    l >>= 15;
                    r >>= 15;

                    let skip = skip_bits >> imp_phase;
                    in_pos += ((skip & 1) as usize * STEREO) & STEREO;
                    imp_phase += 1;
                    if imp_phase == self.core.res {
                        imp_phase = 0;
                    }

                    out[out_pos] = l as i16;
                    out[out_pos + 1] = r as i16;
                }

                in_pos += step;
                out_pos += 2;
            }
        }

        // Compact remaining input
        let left = write_pos - in_pos;
        self.core.buf.copy_within(in_pos..write_pos, 0);
        self.core.write_pos = left;
        self.core.imp_phase = imp_phase;

        out_pos as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fir_resampler_basic() {
        let mut resampler = FirResampler::<12>::new();
        resampler.buffer_size(1024);
        let ratio = resampler.time_ratio(1.0, 0.999, 1.0);
        assert!((ratio - 1.0).abs() < 0.01);
    }
}
