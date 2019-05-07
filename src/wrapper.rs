use crate::structures::{EmuType, EmuHandle, GmeError, GmeVoidResult};
use crate::native;
use std::path::Path;

/// Provides a wrapper around native functions that take an `EmuHandle`
#[derive(Clone)]
pub struct GameMusicEmu {
    handle: EmuHandle
}

impl GameMusicEmu {
    // region static methods
    pub fn new(emu_type: EmuType, sample_rate: u32) -> Self {
        Self { handle: native::new_emu(emu_type, sample_rate) }
    }

    pub fn from_file(path: impl AsRef<Path>, sample_rate: u32) -> Result<GameMusicEmu, GmeError> {
        Ok(Self{handle: native::open_file(path, sample_rate)?})
    }

    pub fn from_data(data: &[u8], sample_rate: u32) -> Result<GameMusicEmu, GmeError> {
        Ok(Self{handle: native::open_data(data, sample_rate)?})
    }
    // endregion

    // region instance methods
    #[inline]
    pub fn load_data(&self, data: &[u8]) -> GmeVoidResult { native::load_data(&self.handle, data) }

    #[inline]
    pub fn load_file(&self, path: impl AsRef<Path>) -> GmeVoidResult {
        native::load_file(&self.handle, path)
    }

    #[inline]
    pub fn play(&self, count: usize, buffer: &mut [i16]) -> GmeVoidResult {
        native::play(&self.handle, count, buffer)
    }

    #[inline]
    pub fn start_track(&self, index: u32) -> GmeVoidResult {
        native::start_track(&self.handle, index)
    }

    #[inline]
    pub fn track_count(&self) -> usize { native::track_count(&self.handle) }
    // endregion
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::Arc;

    #[test]
    fn test_new_emu() {
        let emu = GameMusicEmu::new(EmuType::Nsf, 44100);
        assert!(!emu.handle.to_raw().is_null());
    }

    #[test]
    fn test_from_file() {
        let emu = GameMusicEmu::from_file("test.nsf", 44100).ok().unwrap();
        assert!(!emu.track_count() > 0);
    }

    #[test]
    fn test_from_data() {
        let data = native::get_file_data("test.nsf");
        let emulator = GameMusicEmu::from_data(&data, 44100).ok().unwrap();
        assert!(!emulator.track_count() > 0);
    }

    #[test]
    fn test_load_data() {
        let buffer = native::get_file_data("test.nsf");
        let emulator = GameMusicEmu::new(EmuType::Nsf, 44100);
        let result = emulator.load_data(&vec![1 as u8, 2 as u8, 3 as u8]);
        assert_eq!("Wrong file type for this emulator", result.err().unwrap().message());
        assert_eq!(emulator.track_count(), 0);
        let result = emulator.load_data(&buffer);
        assert!(result.is_ok());
        assert_eq!(emulator.track_count(), 1);
    }

    #[test]
    fn test_arc() {
        let gme = GameMusicEmu::new(EmuType::Nsf, 44100);
        let handle = gme.handle;
        assert_eq!(Arc::strong_count(&handle.emu), 1);
        let handle = handle.clone();
        assert_eq!(Arc::strong_count(&handle.emu), 2);
    }
}