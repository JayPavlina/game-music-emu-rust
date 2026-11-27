// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Dual_Resampler.cpp (2026-06-09 to 2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! Combination of FIR resampling and Blip_Buffer mixing, used by the VGM FM
//! path. 1:1 port of `Dual_Resampler.cpp` state and bookkeeping.
//!
//! C++ `Dual_Resampler` is an abstract base class whose `play_frame` callback
//! is implemented by `Vgm_Emu_Impl`. Here it is a plain data struct; the
//! frame loop (`dual_play` / `play_frame_` / `mix_samples`) lives in
//! `VgmEmu` (see `vgm_emu.rs`) so the callback can borrow the emulator's
//! other fields directly.

use gme_core::fir_resampler::FirResampler;

pub struct DualResampler {
    pub sample_buf: Vec<i16>,
    pub sample_buf_size: usize,
    pub oversamples_per_frame: i32,
    pub buf_pos: i32,
    pub resampler_size: usize,
    pub resampler: FirResampler<12>,
}

impl Default for DualResampler {
    fn default() -> Self {
        Self::new()
    }
}

impl DualResampler {
    pub fn new() -> Self {
        Self {
            sample_buf: Vec::new(),
            sample_buf_size: 0,
            oversamples_per_frame: -1,
            buf_pos: -1,
            resampler_size: 0,
            resampler: FirResampler::new(),
        }
    }

    /// C++ `Dual_Resampler::setup`
    pub fn setup(&mut self, oversample: f64, rolloff: f64, gain: f64) -> f64 {
        self.resampler.time_ratio(oversample, rolloff, gain * 0.5)
    }

    /// C++ `Dual_Resampler::reset`
    pub fn reset(&mut self, max_pairs: usize) {
        // expand allocations a bit
        let alloc = (max_pairs + (max_pairs >> 2)) * 2;
        self.sample_buf.resize(alloc, 0);
        self.resize(max_pairs);
        self.resampler_size =
            self.oversamples_per_frame as usize + (self.oversamples_per_frame as usize >> 2);
        self.resampler.buffer_size(self.resampler_size);
    }

    /// C++ `Dual_Resampler::resize` (called by `Gym_Emu::set_tempo_` as well)
    pub fn resize(&mut self, pairs: usize) {
        let new_sample_buf_size = pairs * 2;
        if self.sample_buf_size != new_sample_buf_size {
            if new_sample_buf_size > self.sample_buf.len() {
                return;
            }
            self.sample_buf_size = new_sample_buf_size;
            self.oversamples_per_frame = (pairs as f64 * self.resampler.ratio()) as i32 * 2 + 2;
            self.clear();
        }
    }

    /// C++ `Dual_Resampler::clear`
    pub fn clear(&mut self) {
        self.buf_pos = self.sample_buf_size as i32;
        self.resampler.clear();
    }
}
