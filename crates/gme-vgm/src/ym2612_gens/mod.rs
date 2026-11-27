mod core;
use self::core::*;
use std::cell::UnsafeCell;

/// Owns a chip at a stable address for its internal operator and table pointers.
#[derive(Default)]
pub struct Gens {
    state: Option<Box<UnsafeCell<Chip>>>,
}

impl Gens {
    pub fn set_rate(&mut self, sample_rate: f64, clock_rate: f64) -> bool {
        if !(1000.0..=1_000_000.0).contains(&sample_rate)
            || !(1.0..=1_073_741_823.0).contains(&clock_rate)
        {
            return false;
        }
        // All fields are integers, floats, nullable pointers or nullable callbacks.
        // Initialize internal pointers only after allocating the chip at its final address.
        let state = unsafe { Box::<UnsafeCell<Chip>>::new_zeroed().assume_init() };
        unsafe {
            gens_set_rate(state.get(), sample_rate, clock_rate);
        }
        self.state = Some(state);
        true
    }
    pub fn reset(&mut self) {
        if let Some(state) = self.state.as_deref() {
            unsafe { gens_reset(state.get()) };
        }
    }
    pub fn mute_voices(&mut self, mask: i32) {
        if let Some(state) = self.state.as_deref() {
            unsafe { (*state.get()).mute_mask = mask };
        }
    }
    pub fn write(&mut self, bank: bool, addr: u8, data: u8) {
        if let Some(state) = self.state.as_deref() {
            unsafe {
                if bank {
                    gens_write1(state.get(), addr.into(), data.into());
                } else {
                    gens_write0(state.get(), addr.into(), data.into());
                }
            }
        }
    }
    pub fn render(&mut self, samples: &mut [i16]) {
        let frames = i32::try_from(samples.len() / 2).expect("frame count exceeds i32");
        if frames == 0 {
            return;
        }
        if let Some(state) = self.state.as_deref() {
            // The core writes two samples per frame and retains no output pointer.
            unsafe { gens_run(state.get(), frames, samples.as_mut_ptr()) };
        }
    }
}
