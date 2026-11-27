mod core;
use self::core::*;
use std::cell::UnsafeCell;

/// Owns a chip at a stable address for its internal operator and table pointers.
#[derive(Default)]
pub struct Mame {
    state: Option<Box<UnsafeCell<YM2612>>>,
}

impl Mame {
    pub fn set_rate(&mut self, sample_rate: f64, clock_rate: f64) -> bool {
        if !(1000.0..=1_000_000.0).contains(&sample_rate)
            || !(1.0..=1_073_741_823.0).contains(&clock_rate)
        {
            return false;
        }
        // All fields are integers, floats, nullable pointers or nullable callbacks.
        // Initialize internal pointers only after allocating the chip at its final address.
        let state = unsafe { Box::<UnsafeCell<YM2612>>::new_zeroed().assume_init() };
        static TABLES: std::sync::Once = std::sync::Once::new();
        TABLES.call_once(|| unsafe { initialize_tables() });
        unsafe {
            let chip = (state.get()).cast();
            ym2612_init(
                chip,
                std::ptr::null_mut(),
                clock_rate as i32,
                sample_rate as i32,
                None,
                None,
            );
            ym2612_reset_chip(chip);
        }
        self.state = Some(state);
        true
    }
    pub fn reset(&mut self) {
        if let Some(state) = self.state.as_deref() {
            unsafe { ym2612_reset_chip(state.get().cast()) };
        }
    }
    pub fn mute_voices(&mut self, mask: i32) {
        if let Some(state) = self.state.as_deref() {
            unsafe { ym2612_set_mutemask(state.get().cast(), mask as u32) };
        }
    }
    pub fn write(&mut self, bank: bool, addr: u8, data: u8) {
        if let Some(state) = self.state.as_deref() {
            unsafe {
                let chip = state.get().cast();
                let port = if bank { 2 } else { 0 };
                ym2612_write(chip, port, addr);
                ym2612_write(chip, port + 1, data);
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
            unsafe { ym2612_generate(state.get().cast(), samples.as_mut_ptr(), frames, 1) };
        }
    }
}
