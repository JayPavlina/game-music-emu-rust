use crate::structures::{EmuType, EmuHandle, GmeError, GmeVoidResult};
use crate::native;
use std::path::Path;

/// Provides a wrapper around native functions that take an `EmuHandle`
#[derive(Clone)]
pub struct GameMusicEmu {
    handle: EmuHandle,
    emu_type: EmuType,
}

impl GameMusicEmu {
    pub fn new(emu_type: EmuType, sample_rate: u32) -> Self {
        Self { handle: native::new_emu(emu_type, sample_rate), emu_type, }
    }

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
    pub fn start_track(&self, index: u32) -> GmeVoidResult { native::start_track(&self.handle, index) }

    #[inline]
    pub fn track_count(&self) -> usize { native::track_count(&self.handle) }

    #[inline]
    pub fn handle(&self) -> &EmuHandle { &self.handle}
}

#[cfg(test)]
mod tests {
    use crate::wrapper::GameMusicEmu;
    use crate::structures::EmuType;
    use std::io::Read;
    use std::sync::Arc;

    fn emu_with_test_file_loaded() -> GameMusicEmu {
        let mut file = std::fs::File::open("test.nsf").unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let emulator = GameMusicEmu::new(EmuType::Nsf, 44100);
        emulator.load_data(&buffer);
        emulator
    }

    #[test]
    fn test_new_emu() {
        let emulator = GameMusicEmu::new(EmuType::Nsf, 44100);
        assert!(!emulator.handle.to_raw().is_null());
    }

    #[test]
    fn test_load_file() {
        let mut file = std::fs::File::open("test.nsf").unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();

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
        let gme = emu_with_test_file_loaded();
        let handle = gme.handle;
        assert_eq!(Arc::strong_count(&handle.emu), 1);
        let handle = handle.clone();
        assert_eq!(Arc::strong_count(&handle.emu), 2);
    }
}