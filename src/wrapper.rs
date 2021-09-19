use crate::emu_type::{EmuType};
use crate::{bridge, GmeOrIoError, GmeResult};
use std::path::Path;
use crate::bridge::EmuHandle;

// pub trait GameMusicEmuBase {
//     fn new(emu_type: EmuType, sample_rate: u32) -> Self;
//     // fn from_file(path: impl AsRef<Path>, sample_rate: u32) -> Result<GameMusicEmu, GmeOrIoError>;
//     fn load_data(&mut self, data: impl AsRef<[u8]>);
// }

/// Provides a wrapper around native functions that take an `EmuHandle`
#[derive(Clone)]
pub struct GameMusicEmu {
    handle: EmuHandle,
}

impl GameMusicEmu {
    /// Create an instance for the specified [crate::EmuType]
    pub fn new(emu_type: EmuType, sample_rate: u32) -> Self {
        Self { handle: bridge::new_emu(emu_type, sample_rate) }
    }

    /// Creates a new instance by loading a file at the specified path
    pub fn from_file(path: impl AsRef<Path>, sample_rate: u32) -> Result<GameMusicEmu, GmeOrIoError> {
        Ok(Self { handle: bridge::open_file(path, sample_rate)? })
    }

    /// Creates a new instance by loading data at the specified path
    pub fn from_data(data: impl AsRef<[u8]>, sample_rate: u32) -> GmeResult<GameMusicEmu> {
        Ok(Self { handle: bridge::open_data(data.as_ref(), sample_rate)? })
    }

    /// Load music file from memory into emulator. Makes a copy of data passed.
    pub fn load_data(&self, data: impl AsRef<[u8]>) -> GmeResult<()> { bridge::load_data(&self.handle, data.as_ref()) }

    /// Load music file into emulator
    pub fn load_file(&self, path: impl AsRef<Path>) -> Result<(), GmeOrIoError> {
        bridge::load_file(&self.handle, path)
    }

    /// Generate `count` 16-bit signed samples into `buffer`. Output is in stereo.
    pub fn play(&self, count: usize, buffer: &mut [i16]) -> GmeResult<()> {
        bridge::play(&self.handle, count, buffer)
    }

    /// Start a track, where 0 is the first track
    pub fn start_track(&self, index: usize) -> GmeResult<()> {
        bridge::start_track(&self.handle, index as _)
    }

    /// Number of milliseconds played since beginning of track
    pub fn tell(&self) -> u32 { bridge::tell(&self.handle) }

    /// Number of tracks available
    pub fn track_count(&self) -> usize { bridge::track_count(&self.handle) }

    /// True if track ended
    pub fn track_ended(&self) -> bool { bridge::track_ended(&self.handle) }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::sync::Arc;
//     use crate::test_utils::*;
//
//     #[test]
//     fn test_new_emu() {
//         let emu = GameMusicEmu::new(EmuType::Nsf, 44100);
//         assert!(!emu.handle.to_raw().is_null());
//     }
//
//     #[test]
//     fn test_from_file() {
//         let emu = GameMusicEmu::from_file(TEST_NSF_PATH, 44100).unwrap();
//         assert!(!emu.track_count() > 0);
//     }
//
//     #[test]
//     fn test_from_data() {
//         let data = get_test_nsf_data();
//         let emulator = GameMusicEmu::from_data(&data, 44100).unwrap();
//         assert!(!emulator.track_count() > 0);
//     }
//
//     #[test]
//     fn test_load_data() {
//         let buffer = get_test_nsf_data();
//         let emulator = GameMusicEmu::new(EmuType::Nsf, 44100);
//         let result = emulator.load_data(&vec![1 as u8, 2 as u8, 3 as u8]);
//         assert_eq!(result.err().unwrap().message(), "Wrong file type for this emulator");
//         assert_eq!(emulator.track_count(), 0);
//         emulator.load_data(&buffer).unwrap();
//         assert_eq!(emulator.track_count(), 1);
//     }
//
//     #[test]
//     fn test_arc() {
//         let gme = GameMusicEmu::new(EmuType::Nsf, 44100);
//         let handle = gme.handle;
//         assert_eq!(Arc::strong_count(&handle.emu), 1);
//         let handle = handle.clone();
//         assert_eq!(Arc::strong_count(&handle.emu), 2);
//     }
// }