// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Multi_Buffer.cpp (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use crate::blip_buffer::{BlipBuffer, BlipTime};

/// Channel mapping for stereo output.
#[derive(Clone, Copy)]
pub struct Channel {
    pub center: *mut BlipBuffer,
    pub left: *mut BlipBuffer,
    pub right: *mut BlipBuffer,
}

impl Channel {
    pub fn null() -> Self {
        Self {
            center: std::ptr::null_mut(),
            left: std::ptr::null_mut(),
            right: std::ptr::null_mut(),
        }
    }
}

// Channel type flags
pub const CHANNEL_TYPE_INDEX_MASK: i32 = 0xFF;
pub const CHANNEL_WAVE_TYPE: i32 = 0x100;
pub const CHANNEL_NOISE_TYPE: i32 = 0x200;
pub const CHANNEL_MIXED_TYPE: i32 = CHANNEL_WAVE_TYPE | CHANNEL_NOISE_TYPE;

/// Trait for multi-channel audio buffers.
/// Replaces C++ Multi_Buffer abstract class.
pub trait MultiBuffer {
    /// Set number of channels.
    fn set_channel_count(&mut self, _count: i32) -> Result<(), &'static str> {
        Ok(())
    }

    /// Get channel mapping for given index and type.
    fn channel(&mut self, index: i32, channel_type: i32) -> Channel;

    fn set_sample_rate(&mut self, rate: i32, msec: i32) -> Result<(), &'static str>;
    fn clock_rate(&mut self, rate: u32);
    fn bass_freq(&mut self, freq: i32);
    fn clear(&mut self);
    fn sample_rate(&self) -> i32;
    fn length(&self) -> i32;

    fn end_frame(&mut self, clocks: BlipTime);

    /// Number of samples per output frame (1 = mono, 2 = stereo).
    fn samples_per_frame(&self) -> i32;

    fn channels_changed_count(&self) -> u32;
    fn channels_changed(&mut self);

    fn read_samples(&mut self, out: &mut [i16], count: i64) -> i64;
    fn samples_avail(&self) -> i64;
}

/// Silent buffer - generates no samples.
pub struct SilentBuffer {
    chan: Channel,
    sample_rate: i32,
    length: i32,
    channels_changed_count: u32,
}

impl Default for SilentBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl SilentBuffer {
    pub fn new() -> Self {
        Self {
            chan: Channel::null(),
            sample_rate: 0,
            length: 0,
            channels_changed_count: 1,
        }
    }
}

impl MultiBuffer for SilentBuffer {
    fn channel(&mut self, _index: i32, _channel_type: i32) -> Channel {
        self.chan
    }

    fn set_sample_rate(&mut self, rate: i32, msec: i32) -> Result<(), &'static str> {
        self.sample_rate = rate;
        self.length = msec;
        Ok(())
    }

    fn clock_rate(&mut self, _rate: u32) {}
    fn bass_freq(&mut self, _freq: i32) {}
    fn clear(&mut self) {}

    fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    fn length(&self) -> i32 {
        self.length
    }

    fn end_frame(&mut self, _clocks: BlipTime) {}

    fn samples_per_frame(&self) -> i32 {
        1
    }

    fn channels_changed_count(&self) -> u32 {
        self.channels_changed_count
    }

    fn channels_changed(&mut self) {
        self.channels_changed_count += 1;
    }

    fn read_samples(&mut self, _out: &mut [i16], _count: i64) -> i64 {
        0
    }

    fn samples_avail(&self) -> i64 {
        0
    }
}

/// Mono buffer - single Blip_Buffer for all channels.
pub struct MonoBuffer {
    buf: BlipBuffer,
    chan: Channel,
    sample_rate: i32,
    length: i32,
    channels_changed_count: u32,
}

impl Default for MonoBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl MonoBuffer {
    pub fn new() -> Self {
        let buf = BlipBuffer::new();
        let buf_ptr = &buf as *const BlipBuffer as *mut BlipBuffer;
        Self {
            buf,
            chan: Channel {
                center: buf_ptr,
                left: buf_ptr,
                right: buf_ptr,
            },
            sample_rate: 0,
            length: 0,
            channels_changed_count: 1,
        }
    }

    pub fn center(&mut self) -> &mut BlipBuffer {
        &mut self.buf
    }
}

impl MultiBuffer for MonoBuffer {
    fn channel(&mut self, _index: i32, _channel_type: i32) -> Channel {
        // Update pointers in case buf was moved
        let ptr = &mut self.buf as *mut BlipBuffer;
        self.chan.center = ptr;
        self.chan.left = ptr;
        self.chan.right = ptr;
        self.chan
    }

    fn set_sample_rate(&mut self, rate: i32, msec: i32) -> Result<(), &'static str> {
        self.buf.set_sample_rate(rate, msec)?;
        self.sample_rate = self.buf.sample_rate();
        self.length = self.buf.length_ms();
        Ok(())
    }

    fn clock_rate(&mut self, rate: u32) {
        self.buf.clock_rate(rate);
    }

    fn bass_freq(&mut self, freq: i32) {
        self.buf.bass_freq(freq);
    }

    fn clear(&mut self) {
        self.buf.clear(true);
    }

    fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    fn length(&self) -> i32 {
        self.length
    }

    fn end_frame(&mut self, clocks: BlipTime) {
        self.buf.end_frame(clocks);
    }

    fn samples_per_frame(&self) -> i32 {
        1
    }

    fn channels_changed_count(&self) -> u32 {
        self.channels_changed_count
    }

    fn channels_changed(&mut self) {
        self.channels_changed_count += 1;
    }

    fn read_samples(&mut self, out: &mut [i16], _count: i64) -> i64 {
        let avail = self.buf.samples_avail() as usize;
        let count = if (_count as usize) < avail {
            _count as usize
        } else {
            avail
        };
        let read = self.buf.read_samples(out, count);
        if read > 0 {
            let bass = self.buf.bass_shift;
            let mut accum = 0i32;
            for o in out.iter_mut().take(read) {
                let s_raw = accum >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
                let s = if s_raw as i16 as i32 != s_raw {
                    0x7FFF - (s_raw >> 24)
                } else {
                    s_raw
                };
                accum = accum.wrapping_add(*o as i32).wrapping_sub(accum >> bass);
                *o = s as i16;
            }
        }
        read as i64
    }

    fn samples_avail(&self) -> i64 {
        self.buf.samples_avail()
    }
}

/// Stereo buffer - three Blip_Buffers for center, left, right.
pub struct StereoBuffer {
    bufs: [BlipBuffer; 3],
    chan: Channel,
    stereo_added: i32,
    was_stereo: bool,
    sample_rate: i32,
    length: i32,
    channels_changed_count: u32,
}

impl Default for StereoBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl StereoBuffer {
    pub fn new() -> Self {
        let bufs = [BlipBuffer::new(), BlipBuffer::new(), BlipBuffer::new()];
        Self {
            bufs,
            chan: Channel::null(),
            stereo_added: 0,
            was_stereo: false,
            sample_rate: 0,
            length: 0,
            channels_changed_count: 1,
        }
    }

    pub fn center(&mut self) -> &mut BlipBuffer {
        &mut self.bufs[0]
    }

    pub fn left(&mut self) -> &mut BlipBuffer {
        &mut self.bufs[1]
    }

    pub fn right(&mut self) -> &mut BlipBuffer {
        &mut self.bufs[2]
    }

    fn update_chan_ptrs(&mut self) {
        self.chan.center = &mut self.bufs[0] as *mut BlipBuffer;
        self.chan.left = &mut self.bufs[1] as *mut BlipBuffer;
        self.chan.right = &mut self.bufs[2] as *mut BlipBuffer;
    }

    fn mix_stereo(&mut self, out: &mut [i16], count: i32) {
        let bass = self.bufs[1].bass_shift;
        let buf0 = &self.bufs[0].buffer;
        let buf1 = &self.bufs[1].buffer;
        let buf2 = &self.bufs[2].buffer;
        let mut accum0 = self.bufs[0].reader_accum;
        let mut accum1 = self.bufs[1].reader_accum;
        let mut accum2 = self.bufs[2].reader_accum;

        for i in 0..count as usize {
            let c = accum0 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let l_raw = c.wrapping_add(accum1 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16));
            let r_raw = c.wrapping_add(accum2 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16));

            let l = if l_raw as i16 as i32 != l_raw {
                0x7FFF - (l_raw >> 24)
            } else {
                l_raw
            };
            let r = if r_raw as i16 as i32 != r_raw {
                0x7FFF - (r_raw >> 24)
            } else {
                r_raw
            };

            // SAFETY: count <= samples_avail() <= buffer_size, and buffers are
            // allocated with 2*buffer_size + EXTRA elements.
            unsafe {
                accum0 = accum0
                    .wrapping_add(*buf0.get_unchecked(i))
                    .wrapping_sub(accum0 >> bass);
                accum1 = accum1
                    .wrapping_add(*buf1.get_unchecked(i))
                    .wrapping_sub(accum1 >> bass);
                accum2 = accum2
                    .wrapping_add(*buf2.get_unchecked(i))
                    .wrapping_sub(accum2 >> bass);

                *out.get_unchecked_mut(i * 2) = l as i16;
                *out.get_unchecked_mut(i * 2 + 1) = r as i16;
            }
        }

        self.bufs[0].reader_accum = accum0;
        self.bufs[1].reader_accum = accum1;
        self.bufs[2].reader_accum = accum2;
    }

    fn mix_stereo_no_center(&mut self, out: &mut [i16], count: i32) {
        let bass = self.bufs[1].bass_shift;
        let buf1 = &self.bufs[1].buffer;
        let buf2 = &self.bufs[2].buffer;
        let mut accum1 = self.bufs[1].reader_accum;
        let mut accum2 = self.bufs[2].reader_accum;

        for i in 0..count as usize {
            let l_raw = accum1 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let r_raw = accum2 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);

            let l = if l_raw as i16 as i32 != l_raw {
                0x7FFF - (l_raw >> 24)
            } else {
                l_raw
            };
            let r = if r_raw as i16 as i32 != r_raw {
                0x7FFF - (r_raw >> 24)
            } else {
                r_raw
            };

            unsafe {
                accum1 = accum1
                    .wrapping_add(*buf1.get_unchecked(i))
                    .wrapping_sub(accum1 >> bass);
                accum2 = accum2
                    .wrapping_add(*buf2.get_unchecked(i))
                    .wrapping_sub(accum2 >> bass);

                *out.get_unchecked_mut(i * 2) = l as i16;
                *out.get_unchecked_mut(i * 2 + 1) = r as i16;
            }
        }

        self.bufs[1].reader_accum = accum1;
        self.bufs[2].reader_accum = accum2;
    }

    fn mix_mono(&mut self, out: &mut [i16], count: i32) {
        let bass = self.bufs[0].bass_shift;
        let buf0 = &self.bufs[0].buffer;
        let mut accum0 = self.bufs[0].reader_accum;

        for i in 0..count as usize {
            let s_raw = accum0 >> (crate::blip_buffer::BLIP_SAMPLE_BITS - 16);
            let s = if s_raw as i16 as i32 != s_raw {
                0x7FFF - (s_raw >> 24)
            } else {
                s_raw
            };

            unsafe {
                accum0 = accum0
                    .wrapping_add(*buf0.get_unchecked(i))
                    .wrapping_sub(accum0 >> bass);

                *out.get_unchecked_mut(i * 2) = s as i16;
                *out.get_unchecked_mut(i * 2 + 1) = s as i16;
            }
        }

        self.bufs[0].reader_accum = accum0;
    }
}

impl MultiBuffer for StereoBuffer {
    fn channel(&mut self, _index: i32, _channel_type: i32) -> Channel {
        self.update_chan_ptrs();
        self.chan
    }

    fn set_sample_rate(&mut self, rate: i32, msec: i32) -> Result<(), &'static str> {
        for buf in &mut self.bufs {
            buf.set_sample_rate(rate, msec)?;
        }
        self.sample_rate = self.bufs[0].sample_rate();
        self.length = self.bufs[0].length_ms();
        Ok(())
    }

    fn clock_rate(&mut self, rate: u32) {
        for buf in &mut self.bufs {
            buf.clock_rate(rate);
        }
    }

    fn bass_freq(&mut self, freq: i32) {
        for buf in &mut self.bufs {
            buf.bass_freq(freq);
        }
    }

    fn clear(&mut self) {
        self.stereo_added = 0;
        self.was_stereo = false;
        for buf in &mut self.bufs {
            buf.clear(true);
        }
    }

    fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    fn length(&self) -> i32 {
        self.length
    }

    fn end_frame(&mut self, clock_count: BlipTime) {
        self.stereo_added = 0;
        for i in 0..3 {
            self.stereo_added |= self.bufs[i].clear_modified() << i;
            self.bufs[i].end_frame(clock_count);
        }
    }

    fn samples_per_frame(&self) -> i32 {
        2
    }

    fn channels_changed_count(&self) -> u32 {
        self.channels_changed_count
    }

    fn channels_changed(&mut self) {
        self.channels_changed_count += 1;
    }

    fn read_samples(&mut self, out: &mut [i16], count: i64) -> i64 {
        debug_assert!(count & 1 == 0); // count must be even
        let mut count = (count as u64 / 2) as i64;

        let avail = self.bufs[0].samples_avail();
        if count > avail {
            count = avail;
        }
        if count > 0 {
            let bufs_used = self.stereo_added | if self.was_stereo { 1 } else { 0 };
            if bufs_used <= 1 {
                self.mix_mono(out, count as i32);
                self.bufs[0].remove_samples(count);
                self.bufs[1].remove_silence(count);
                self.bufs[2].remove_silence(count);
            } else if bufs_used & 1 != 0 {
                self.mix_stereo(out, count as i32);
                self.bufs[0].remove_samples(count);
                self.bufs[1].remove_samples(count);
                self.bufs[2].remove_samples(count);
            } else {
                self.mix_stereo_no_center(out, count as i32);
                self.bufs[0].remove_silence(count);
                self.bufs[1].remove_samples(count);
                self.bufs[2].remove_samples(count);
            }

            if self.bufs[0].samples_avail() == 0 {
                self.was_stereo = self.stereo_added != 0;
                self.stereo_added = 0;
            }
        }

        count * 2
    }

    fn samples_avail(&self) -> i64 {
        self.bufs[0].samples_avail() * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stereo_buffer_basic() {
        let mut buf = StereoBuffer::new();
        buf.set_sample_rate(44100, 1000).unwrap();
        buf.clock_rate(44100);
        buf.clear();
        assert_eq!(buf.sample_rate(), 44100);
        assert_eq!(buf.samples_per_frame(), 2);
    }

    #[test]
    fn test_mono_buffer_basic() {
        let mut buf = MonoBuffer::new();
        buf.set_sample_rate(44100, 1000).unwrap();
        buf.clock_rate(44100);
        buf.clear();
        assert_eq!(buf.samples_per_frame(), 1);
    }
}
