// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Effects_Buffer.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use crate::blip_buffer::{BlipBuffer, BlipTime};
use crate::multi_buffer::{Channel, MultiBuffer};

const ECHO_SIZE: usize = 4096;
const ECHO_MASK: usize = ECHO_SIZE - 1;
const REVERB_SIZE: usize = 8192 * 2;
const REVERB_MASK: usize = REVERB_SIZE - 1;
const MAX_BUF_COUNT: usize = 7;
const CHAN_TYPES_COUNT: usize = 3;

type FixedT = i32;

const fn to_fixed(f: f64) -> FixedT {
    (f * (1i64 << 15) as f64 + 0.5) as FixedT
}

const fn fmul(x: FixedT, y: FixedT) -> FixedT {
    ((x as i64 * y as i64) >> 15) as FixedT
}

/// Configuration for Effects_Buffer.
#[derive(Clone)]
pub struct EffectsConfig {
    pub pan_1: f64,
    pub pan_2: f64,
    pub echo_delay: f64,
    pub echo_level: f64,
    pub reverb_delay: f64,
    pub delay_variance: f64,
    pub reverb_level: f64,
    pub effects_enabled: bool,
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self {
            pan_1: -0.15,
            pan_2: 0.15,
            reverb_delay: 88.0,
            reverb_level: 0.12,
            echo_delay: 61.0,
            echo_level: 0.10,
            delay_variance: 18.0,
            effects_enabled: false,
        }
    }
}

#[derive(Default)]
struct ChanData {
    pan_1_levels: [FixedT; 2],
    pan_2_levels: [FixedT; 2],
    echo_delay_l: usize,
    echo_delay_r: usize,
    echo_level: FixedT,
    reverb_delay_l: usize,
    reverb_delay_r: usize,
    reverb_level: FixedT,
}

/// Multi-channel effects buffer with panning, echo, and reverb.
pub struct EffectsBuffer {
    max_voices: usize,
    bufs: Vec<BlipBuffer>,
    chan_types: Vec<Channel>,
    config: EffectsConfig,
    stereo_remain: i64,
    effect_remain: i64,
    buf_count: usize,
    effects_enabled: bool,
    reverb_buf: Vec<Vec<i16>>,
    echo_buf: Vec<Vec<i16>>,
    reverb_pos: Vec<usize>,
    echo_pos: Vec<usize>,
    chans: ChanData,
    sample_rate: i32,
    length: i32,
    channels_changed_count: u32,
}

impl EffectsBuffer {
    pub fn new(num_voices: usize, center_only: bool) -> Self {
        let buf_count = num_voices
            * if center_only {
                MAX_BUF_COUNT - 4
            } else {
                MAX_BUF_COUNT
            };
        let mut bufs = Vec::with_capacity(buf_count);
        for _ in 0..buf_count {
            bufs.push(BlipBuffer::new());
        }
        let mut reverb_buf = Vec::with_capacity(num_voices);
        let mut echo_buf = Vec::with_capacity(num_voices);
        for _ in 0..num_voices {
            reverb_buf.push(vec![0i16; REVERB_SIZE]);
            echo_buf.push(vec![0i16; ECHO_SIZE]);
        }

        let mut eb = Self {
            max_voices: num_voices,
            bufs,
            chan_types: vec![Channel::null(); num_voices * CHAN_TYPES_COUNT],
            config: EffectsConfig::default(),
            stereo_remain: 0,
            effect_remain: 0,
            buf_count,
            effects_enabled: false,
            reverb_buf,
            echo_buf,
            reverb_pos: vec![0; num_voices],
            echo_pos: vec![0; num_voices],
            chans: ChanData::default(),
            sample_rate: 0,
            length: 0,
            channels_changed_count: 1,
        };
        eb.set_depth(0.0);
        eb
    }

    pub fn set_depth(&mut self, d: f64) {
        let f = d as f32;
        let clamped_f = if f > 0.5 { 0.5 } else { f };
        let c = EffectsConfig {
            pan_1: -0.6 * d,
            pan_2: 0.6 * d,
            reverb_delay: 880.0 * 0.1,
            echo_delay: 610.0 * 0.1,
            reverb_level: 0.5 * clamped_f as f64,
            echo_level: 0.30 * clamped_f as f64,
            delay_variance: 180.0 * 0.1,
            effects_enabled: d > 0.0,
        };
        self.config_buffer(c);
    }

    pub fn config_buffer(&mut self, cfg: EffectsConfig) {
        self.channels_changed_count += 1;

        // Clear echo/reverb if enabling effects
        if !self.config.effects_enabled && cfg.effects_enabled {
            for i in 0..self.max_voices {
                self.echo_buf[i].fill(0);
                self.reverb_buf[i].fill(0);
            }
        }

        self.config = cfg;

        if self.config.effects_enabled {
            self.chans.pan_1_levels[0] = to_fixed(1.0) - to_fixed(self.config.pan_1);
            self.chans.pan_1_levels[1] = to_fixed(2.0) - self.chans.pan_1_levels[0];

            self.chans.pan_2_levels[0] = to_fixed(1.0) - to_fixed(self.config.pan_2);
            self.chans.pan_2_levels[1] = to_fixed(2.0) - self.chans.pan_2_levels[0];

            self.chans.reverb_level = to_fixed(self.config.reverb_level);
            self.chans.echo_level = to_fixed(self.config.echo_level);

            let delay_offset =
                (1.0 / 2000.0 * self.config.delay_variance * self.sample_rate as f64) as i32;

            let reverb_sample_delay =
                (1.0 / 1000.0 * self.config.reverb_delay * self.sample_rate as f64) as i32;
            self.chans.reverb_delay_l = pin_range(
                REVERB_SIZE as i32 - (reverb_sample_delay - delay_offset) * 2,
                REVERB_SIZE as i32 - 2,
                0,
            ) as usize;
            self.chans.reverb_delay_r = pin_range(
                REVERB_SIZE as i32 + 1 - (reverb_sample_delay + delay_offset) * 2,
                REVERB_SIZE as i32 - 1,
                1,
            ) as usize;

            let echo_sample_delay =
                (1.0 / 1000.0 * self.config.echo_delay * self.sample_rate as f64) as i32;
            self.chans.echo_delay_l = pin_range(
                ECHO_SIZE as i32 - 1 - (echo_sample_delay - delay_offset),
                ECHO_SIZE as i32 - 1,
                0,
            ) as usize;
            self.chans.echo_delay_r = pin_range(
                ECHO_SIZE as i32 - 1 - (echo_sample_delay + delay_offset),
                ECHO_SIZE as i32 - 1,
                0,
            ) as usize;

            for i in 0..self.max_voices {
                self.chan_types[i * CHAN_TYPES_COUNT] = Channel {
                    center: &mut self.bufs[i * MAX_BUF_COUNT] as *mut BlipBuffer,
                    left: &mut self.bufs[i * MAX_BUF_COUNT + 3] as *mut BlipBuffer,
                    right: &mut self.bufs[i * MAX_BUF_COUNT + 4] as *mut BlipBuffer,
                };
                self.chan_types[i * CHAN_TYPES_COUNT + 1] = Channel {
                    center: &mut self.bufs[i * MAX_BUF_COUNT + 1] as *mut BlipBuffer,
                    left: &mut self.bufs[i * MAX_BUF_COUNT + 3] as *mut BlipBuffer,
                    right: &mut self.bufs[i * MAX_BUF_COUNT + 4] as *mut BlipBuffer,
                };
                self.chan_types[i * CHAN_TYPES_COUNT + 2] = Channel {
                    center: &mut self.bufs[i * MAX_BUF_COUNT + 2] as *mut BlipBuffer,
                    left: &mut self.bufs[i * MAX_BUF_COUNT + 5] as *mut BlipBuffer,
                    right: &mut self.bufs[i * MAX_BUF_COUNT + 6] as *mut BlipBuffer,
                };
            }
        } else {
            for i in 0..self.max_voices {
                for j in 0..CHAN_TYPES_COUNT {
                    self.chan_types[i * CHAN_TYPES_COUNT + j] = Channel {
                        center: &mut self.bufs[i * MAX_BUF_COUNT] as *mut BlipBuffer,
                        left: &mut self.bufs[i * MAX_BUF_COUNT + 1] as *mut BlipBuffer,
                        right: &mut self.bufs[i * MAX_BUF_COUNT + 2] as *mut BlipBuffer,
                    };
                }
            }
        }

        if self.buf_count < MAX_BUF_COUNT {
            for i in 0..self.max_voices {
                for j in 0..CHAN_TYPES_COUNT {
                    let c = &mut self.chan_types[i * CHAN_TYPES_COUNT + j];
                    c.left = c.center;
                    c.right = c.center;
                }
            }
        }
    }

    fn mix_mono(&mut self, out: &mut [i16], count: i32, voice: usize) {
        let bass = self.bufs[voice * MAX_BUF_COUNT].bass_shift;
        let buf = &self.bufs[voice * MAX_BUF_COUNT].buffer;
        let mut accum = self.bufs[voice * MAX_BUF_COUNT].reader_accum;
        let stride = self.max_voices * 2;
        let base = voice * 2;

        for i in 0..count as usize {
            let s = accum >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            // SAFETY: count <= frame count, buf has sufficient size.
            unsafe {
                accum += *buf.get_unchecked(i) - (accum >> bass);
            }
            let s = if s as i16 as i32 != s {
                0x7FFF - (s >> 24)
            } else {
                s
            };
            let idx = base + i * stride;
            out[idx] = s as i16;
            out[idx + 1] = s as i16;
        }

        self.bufs[voice * MAX_BUF_COUNT].reader_accum = accum;
    }

    fn mix_stereo(&mut self, out: &mut [i16], count: i32, voice: usize) {
        let bass = self.bufs[voice * MAX_BUF_COUNT].bass_shift;
        let buf_c = &self.bufs[voice * MAX_BUF_COUNT].buffer;
        let buf_l = &self.bufs[voice * MAX_BUF_COUNT + 1].buffer;
        let buf_r = &self.bufs[voice * MAX_BUF_COUNT + 2].buffer;
        let mut accum_c = self.bufs[voice * MAX_BUF_COUNT].reader_accum;
        let mut accum_l = self.bufs[voice * MAX_BUF_COUNT + 1].reader_accum;
        let mut accum_r = self.bufs[voice * MAX_BUF_COUNT + 2].reader_accum;
        let stride = self.max_voices * 2;
        let base = voice * 2;

        for i in 0..count as usize {
            let cs = accum_c >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let left = cs + (accum_l >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16));
            let right = cs + (accum_r >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16));

            // SAFETY: count <= frame count, buffers have sufficient size.
            unsafe {
                accum_c += *buf_c.get_unchecked(i) - (accum_c >> bass);
                accum_l += *buf_l.get_unchecked(i) - (accum_l >> bass);
                accum_r += *buf_r.get_unchecked(i) - (accum_r >> bass);
            }

            let left = if left as i16 as i32 != left {
                0x7FFF - (left >> 24)
            } else {
                left
            };
            let right = if right as i16 as i32 != right {
                0x7FFF - (right >> 24)
            } else {
                right
            };

            let idx = base + i * stride;
            out[idx] = left as i16;
            out[idx + 1] = right as i16;
        }

        self.bufs[voice * MAX_BUF_COUNT].reader_accum = accum_c;
        self.bufs[voice * MAX_BUF_COUNT + 1].reader_accum = accum_l;
        self.bufs[voice * MAX_BUF_COUNT + 2].reader_accum = accum_r;
    }

    fn mix_mono_enhanced(&mut self, out: &mut [i16], frames: i32, voice: usize) {
        let base = voice * MAX_BUF_COUNT;
        let bass = self.bufs[base + 2].bass_shift;
        let buf_sq1 = &self.bufs[base].buffer;
        let buf_sq2 = &self.bufs[base + 1].buffer;
        let buf_center = &self.bufs[base + 2].buffer;
        let mut accum_sq1 = self.bufs[base].reader_accum;
        let mut accum_sq2 = self.bufs[base + 1].reader_accum;
        let mut accum_center = self.bufs[base + 2].reader_accum;

        let reverb_buf = &mut self.reverb_buf[voice];
        let echo_buf = &mut self.echo_buf[voice];
        let mut echo_pos = self.echo_pos[voice];
        let mut reverb_pos = self.reverb_pos[voice];

        for i in 0..frames as usize {
            let sum1_s = accum_sq1 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let sum2_s = accum_sq2 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);

            // SAFETY: frames <= buffer size.
            unsafe {
                accum_sq1 += *buf_sq1.get_unchecked(i) - (accum_sq1 >> bass);
                accum_sq2 += *buf_sq2.get_unchecked(i) - (accum_sq2 >> bass);
            }

            let new_reverb_l = fmul(sum1_s, self.chans.pan_1_levels[0])
                + fmul(sum2_s, self.chans.pan_2_levels[0])
                + reverb_buf[(reverb_pos + self.chans.reverb_delay_l) & REVERB_MASK] as i32;

            let new_reverb_r = fmul(sum1_s, self.chans.pan_1_levels[1])
                + fmul(sum2_s, self.chans.pan_2_levels[1])
                + reverb_buf[(reverb_pos + self.chans.reverb_delay_r) & REVERB_MASK] as i32;

            reverb_buf[reverb_pos] = fmul(new_reverb_l, self.chans.reverb_level) as i16;
            reverb_buf[reverb_pos + 1] = fmul(new_reverb_r, self.chans.reverb_level) as i16;
            reverb_pos = (reverb_pos + 2) & REVERB_MASK;

            let sum3_s = accum_center >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            unsafe {
                accum_center += *buf_center.get_unchecked(i) - (accum_center >> bass);
            }

            let left = new_reverb_l
                + sum3_s
                + fmul(
                    self.chans.echo_level,
                    echo_buf[(echo_pos + self.chans.echo_delay_l) & ECHO_MASK] as i32,
                );
            let right = new_reverb_r
                + sum3_s
                + fmul(
                    self.chans.echo_level,
                    echo_buf[(echo_pos + self.chans.echo_delay_r) & ECHO_MASK] as i32,
                );

            echo_buf[echo_pos] = sum3_s as i16;
            echo_pos = (echo_pos + 1) & ECHO_MASK;

            let left = if left as i16 as i32 != left {
                0x7FFF - (left >> 24)
            } else {
                left
            };
            let right = if right as i16 as i32 != right {
                0x7FFF - (right >> 24)
            } else {
                right
            };

            out[voice * 2 + i * self.max_voices * 2] = left as i16;
            out[voice * 2 + i * self.max_voices * 2 + 1] = right as i16;
        }

        self.reverb_pos[voice] = reverb_pos;
        self.echo_pos[voice] = echo_pos;
        self.bufs[base].reader_accum = accum_sq1;
        self.bufs[base + 1].reader_accum = accum_sq2;
        self.bufs[base + 2].reader_accum = accum_center;
    }

    fn mix_enhanced(&mut self, out: &mut [i16], frames: i32, voice: usize) {
        let base = voice * MAX_BUF_COUNT;
        let bass = self.bufs[base + 2].bass_shift;
        let buf_sq1 = &self.bufs[base].buffer;
        let buf_sq2 = &self.bufs[base + 1].buffer;
        let buf_center = &self.bufs[base + 2].buffer;
        let buf_l1 = &self.bufs[base + 3].buffer;
        let buf_r1 = &self.bufs[base + 4].buffer;
        let buf_l2 = &self.bufs[base + 5].buffer;
        let buf_r2 = &self.bufs[base + 6].buffer;
        let mut accum_sq1 = self.bufs[base].reader_accum;
        let mut accum_sq2 = self.bufs[base + 1].reader_accum;
        let mut accum_center = self.bufs[base + 2].reader_accum;
        let mut accum_l1 = self.bufs[base + 3].reader_accum;
        let mut accum_r1 = self.bufs[base + 4].reader_accum;
        let mut accum_l2 = self.bufs[base + 5].reader_accum;
        let mut accum_r2 = self.bufs[base + 6].reader_accum;

        let reverb_buf = &mut self.reverb_buf[voice];
        let echo_buf = &mut self.echo_buf[voice];
        let mut echo_pos = self.echo_pos[voice];
        let mut reverb_pos = self.reverb_pos[voice];

        for i in 0..frames as usize {
            let sum1_s = accum_sq1 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let sum2_s = accum_sq2 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);

            // SAFETY: frames <= buffer size.
            unsafe {
                accum_sq1 += *buf_sq1.get_unchecked(i) - (accum_sq1 >> bass);
                accum_sq2 += *buf_sq2.get_unchecked(i) - (accum_sq2 >> bass);
            }

            let l1_s = accum_l1 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let r1_s = accum_r1 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);

            let new_reverb_l = fmul(sum1_s, self.chans.pan_1_levels[0])
                + fmul(sum2_s, self.chans.pan_2_levels[0])
                + l1_s
                + reverb_buf[(reverb_pos + self.chans.reverb_delay_l) & REVERB_MASK] as i32;

            let new_reverb_r = fmul(sum1_s, self.chans.pan_1_levels[1])
                + fmul(sum2_s, self.chans.pan_2_levels[1])
                + r1_s
                + reverb_buf[(reverb_pos + self.chans.reverb_delay_r) & REVERB_MASK] as i32;

            unsafe {
                accum_l1 += *buf_l1.get_unchecked(i) - (accum_l1 >> bass);
                accum_r1 += *buf_r1.get_unchecked(i) - (accum_r1 >> bass);
            }

            reverb_buf[reverb_pos] = fmul(new_reverb_l, self.chans.reverb_level) as i16;
            reverb_buf[reverb_pos + 1] = fmul(new_reverb_r, self.chans.reverb_level) as i16;
            reverb_pos = (reverb_pos + 2) & REVERB_MASK;

            let sum3_s = accum_center >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            unsafe {
                accum_center += *buf_center.get_unchecked(i) - (accum_center >> bass);
            }

            let l2_s = accum_l2 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let r2_s = accum_r2 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);

            let left = new_reverb_l
                + sum3_s
                + l2_s
                + fmul(
                    self.chans.echo_level,
                    echo_buf[(echo_pos + self.chans.echo_delay_l) & ECHO_MASK] as i32,
                );
            let right = new_reverb_r
                + sum3_s
                + r2_s
                + fmul(
                    self.chans.echo_level,
                    echo_buf[(echo_pos + self.chans.echo_delay_r) & ECHO_MASK] as i32,
                );

            unsafe {
                accum_l2 += *buf_l2.get_unchecked(i) - (accum_l2 >> bass);
                accum_r2 += *buf_r2.get_unchecked(i) - (accum_r2 >> bass);
            }

            echo_buf[echo_pos] = sum3_s as i16;
            echo_pos = (echo_pos + 1) & ECHO_MASK;

            let left = if left as i16 as i32 != left {
                0x7FFF - (left >> 24)
            } else {
                left
            };
            let right = if right as i16 as i32 != right {
                0x7FFF - (right >> 24)
            } else {
                right
            };

            out[voice * 2 + i * self.max_voices * 2] = left as i16;
            out[voice * 2 + i * self.max_voices * 2 + 1] = right as i16;
        }

        self.reverb_pos[voice] = reverb_pos;
        self.echo_pos[voice] = echo_pos;
        self.bufs[base].reader_accum = accum_sq1;
        self.bufs[base + 1].reader_accum = accum_sq2;
        self.bufs[base + 2].reader_accum = accum_center;
        self.bufs[base + 3].reader_accum = accum_l1;
        self.bufs[base + 4].reader_accum = accum_r1;
        self.bufs[base + 5].reader_accum = accum_l2;
        self.bufs[base + 6].reader_accum = accum_r2;
    }
}

fn pin_range(n: i32, max: i32, min: i32) -> i32 {
    n.clamp(min, max)
}

impl MultiBuffer for EffectsBuffer {
    fn channel(&mut self, i: i32, channel_type: i32) -> Channel {
        let mut out = CHAN_TYPES_COUNT - 1;
        if channel_type == 0 {
            out = (i as usize % 5).min(CHAN_TYPES_COUNT - 1);
        } else if channel_type & crate::multi_buffer::CHANNEL_NOISE_TYPE == 0
            && (channel_type & crate::multi_buffer::CHANNEL_TYPE_INDEX_MASK) % 3 != 0
        {
            out = (channel_type & 1) as usize;
        }
        self.chan_types[(i as usize % self.max_voices) * CHAN_TYPES_COUNT + out]
    }

    fn set_sample_rate(&mut self, rate: i32, msec: i32) -> Result<(), &'static str> {
        for i in 0..self.buf_count {
            self.bufs[i].set_sample_rate(rate, msec)?;
        }
        self.sample_rate = self.bufs[0].sample_rate();
        self.length = self.bufs[0].length_ms();
        self.config_buffer(self.config.clone());
        self.clear();
        Ok(())
    }

    fn clock_rate(&mut self, rate: u32) {
        for i in 0..self.buf_count {
            self.bufs[i].clock_rate(rate);
        }
    }

    fn bass_freq(&mut self, freq: i32) {
        for i in 0..self.buf_count {
            self.bufs[i].bass_freq(freq);
        }
    }

    fn clear(&mut self) {
        self.stereo_remain = 0;
        self.effect_remain = 0;
        for i in 0..self.max_voices {
            self.echo_buf[i].fill(0);
            self.reverb_buf[i].fill(0);
        }
        for i in 0..self.buf_count {
            self.bufs[i].clear(true);
        }
    }

    fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    fn length(&self) -> i32 {
        self.length
    }

    fn end_frame(&mut self, clock_count: BlipTime) {
        let stereo_mask = if self.config.effects_enabled {
            0x78
        } else {
            0x06
        };
        let buf_count_per_voice = self.buf_count / self.max_voices;

        for v in 0..self.max_voices {
            let mut bufs_used = 0i32;
            for i in 0..buf_count_per_voice {
                let idx = v * buf_count_per_voice + i;
                bufs_used |= self.bufs[idx].clear_modified() << i;
                self.bufs[idx].end_frame(clock_count);

                if (bufs_used & stereo_mask) != 0
                    && self.buf_count == self.max_voices * MAX_BUF_COUNT
                {
                    self.stereo_remain = self.stereo_remain.max(
                        self.bufs[idx].samples_avail() + self.bufs[idx].output_latency() as i64,
                    );
                }
                if self.effects_enabled || self.config.effects_enabled {
                    self.effect_remain = self.effect_remain.max(
                        self.bufs[idx].samples_avail() + self.bufs[idx].output_latency() as i64,
                    );
                }
            }
        }

        self.effects_enabled = self.config.effects_enabled;
    }

    fn samples_per_frame(&self) -> i32 {
        (self.max_voices * 2) as i32
    }

    fn channels_changed_count(&self) -> u32 {
        self.channels_changed_count
    }

    fn channels_changed(&mut self) {
        self.channels_changed_count += 1;
    }

    fn samples_avail(&self) -> i64 {
        self.bufs[0].samples_avail() * i64::from(self.samples_per_frame())
    }

    fn read_samples(&mut self, out: &mut [i16], total_samples: i64) -> i64 {
        let n_channels = self.max_voices * 2;
        let buf_count_per_voice = self.buf_count / self.max_voices;

        let mut remain = self.bufs[0].samples_avail();
        let total = remain.min(total_samples / n_channels as i64);
        remain = total;

        let mut out_offset = 0;
        while remain > 0 {
            let mut active_bufs = buf_count_per_voice;
            let mut count = remain;

            if self.effect_remain > 0 {
                if count > self.effect_remain {
                    count = self.effect_remain;
                }
                if self.stereo_remain > 0 {
                    for v in 0..self.max_voices {
                        self.mix_enhanced(&mut out[out_offset..], count as i32, v);
                    }
                } else {
                    for v in 0..self.max_voices {
                        self.mix_mono_enhanced(&mut out[out_offset..], count as i32, v);
                    }
                    active_bufs = 3;
                }
            } else if self.stereo_remain > 0 {
                for v in 0..self.max_voices {
                    self.mix_stereo(&mut out[out_offset..], count as i32, v);
                }
                active_bufs = 3;
            } else {
                for v in 0..self.max_voices {
                    self.mix_mono(&mut out[out_offset..], count as i32, v);
                }
                active_bufs = 1;
            }

            out_offset += count as usize * n_channels;
            remain -= count;

            self.stereo_remain -= count;
            if self.stereo_remain < 0 {
                self.stereo_remain = 0;
            }
            self.effect_remain -= count;
            if self.effect_remain < 0 {
                self.effect_remain = 0;
            }

            for v in 0..self.max_voices {
                for i in 0..buf_count_per_voice {
                    let idx = v * buf_count_per_voice + i;
                    if i < active_bufs {
                        self.bufs[idx].remove_samples(count);
                    } else {
                        self.bufs[idx].remove_silence(count);
                    }
                }
            }
        }

        total * n_channels as i64
    }
}
