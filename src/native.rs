#![cfg(feature = "native")]

mod core;
mod nes;

use crate::{EmuType, GmeResult};
use crate::native::nes::NsfEmu;

pub type DynamicResult<T> = Result<T, Box<dyn std::error::Error>>;
pub type Sample = i32;
pub type UInt = u32;
pub type Int = i32;
pub type Long = i32;
pub type ULong = u32;
pub type Short = i16;
pub type Byte = u8;
pub type Double = f64;
pub type Single = f32;

pub struct NativeGameMusicEmu<T: NativeMusicEmuBase>(Box<T>);

impl<T: NativeMusicEmuBase> NativeGameMusicEmu<T> {
    pub fn new(emu_type: EmuType, sample_rate: u32) -> Self {
        match emu_type {
            EmuType::Nsf => Self(Box::new(unimplemented!())),
            _ => unimplemented!()
        }
    }

    pub fn load_data(&mut self, data: impl AsRef<[u8]>) -> DynamicResult<()> {
        self.0.load_data(data)
    }
}

/// The one that is implemented for individual emulators
pub trait NativeMusicEmuBase {
    fn load_data(&mut self, data: impl AsRef<[u8]>) -> DynamicResult<()>;
}