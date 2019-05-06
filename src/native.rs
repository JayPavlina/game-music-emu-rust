use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::slice;
use crate::structures::{EmuType, EmuHandle, GmeError, GmeVoidResult, GmeHandleResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub(crate) fn delete(handle: &EmuHandle) {
    unsafe { gme_delete(handle.to_raw()); }
}

/// Determine likely `EmuType` based on first four bytes of file.
pub fn identify_header(buffer: &[u8]) -> EmuType {
    unsafe {
        EmuType::from_extension(&CStr::from_ptr(gme_identify_header(buffer.as_ptr())).to_str()
            .unwrap().to_string())
    }
}

/// Load music file from memory into emulator. Makes a copy of data passed.
pub fn load_data(handle: &EmuHandle, data: &[u8]) -> GmeVoidResult {
    unsafe {
        let mut emu_ptr: *const MusicEmu = std::ptr::null_mut();
        process_result(gme_load_data(handle.to_raw(), data.as_ptr(), data.len()))
    }
}

/// Load music file into emulator
pub fn load_file(handle: &EmuHandle, path: impl AsRef<Path>) -> GmeVoidResult {
    let buffer = get_file_data(path);
    load_data(handle, &buffer)
}

/// Creates an `EmuHandle` with the specified `EmuType`
pub fn new_emu(emu_type: EmuType, sample_rate: u32) -> EmuHandle {
    unsafe {
        let cstring = CString::new(emu_type.to_extension()).unwrap();
        let gme_type = gme_identify_extension(cstring.as_ptr());
        let music_emu = gme_new_emu(gme_type, sample_rate as i32);
        EmuHandle::new(music_emu)
    }
}


/// Creates a new `EmuHandle` and loads it with `data`. Makes a copy of the data.
pub fn open_data(data: &[u8], sample_rate: u32) -> GmeHandleResult {
    let emu_type = identify_header(data);
    let handle = new_emu(emu_type, sample_rate);
    let error = load_data(&handle, data);
    if error.is_ok() { Ok(handle) } else { Err(error.err().unwrap()) }
}

/// Creates a new `EmuHandle` and loads it with the file at the specified path
pub fn open_file(path: impl AsRef<Path>, sample_rate: u32) -> GmeHandleResult {
    let buffer = get_file_data(path);
    open_data(&buffer, sample_rate)
}

/// Generate `count` 16-bit signed samples into `buffer`. Output is in stereo.
pub fn play(handle: &EmuHandle, count: usize, buffer: &mut [i16]) -> GmeVoidResult {
    unsafe { process_result(gme_play(handle.to_raw(), count as i32, buffer.as_mut_ptr())) }
}

/// Start a track, where 0 is the first track
pub fn start_track(handle: &EmuHandle, index: u32) -> GmeVoidResult {
    unsafe { process_result(gme_start_track(handle.to_raw(), index as i32)) }
}

/// Number of tracks available
pub fn track_count(handle: &EmuHandle) -> usize {
    unsafe { gme_track_count(handle.to_raw()) as usize }
}

/// Returns all of the supported `EmuTypes`. This is based on the features the crate is compiled
/// with.
pub fn type_list() -> Vec<EmuType> {
    let mut types = Vec::new();
    unsafe {
        let mut p = gme_type_list();
        while *p != std::ptr::null() {
            let gme_type = p.clone().read();
            let extension = CStr::from_ptr((*gme_type).extension).to_str().unwrap();
            println!("extension: {}", extension);
            types.push(EmuType::from_extension(extension));
            p = p.offset(1);
        }
    }
    types
}

fn process_result(result: *const c_char) -> GmeVoidResult {
    if result.is_null() {
        Ok(())
    } else {
        unsafe { Err(GmeError::new(CStr::from_ptr(result).to_str().unwrap().to_string())) }
    }
}

fn get_file_data(path: impl AsRef<Path>) -> Vec<u8> {
    let mut file = File::open(path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    buffer
}

#[repr(C)]
#[derive(Clone)]
pub struct MusicEmu { _private: isize }

// gme_type_t_ is struct
// gme_type_t holds pointer to other
#[repr(C)]
pub struct gme_type_t_struct {
    /// name of system this music file type is generally for
    pub system: *const c_char,
    /// non-zero for formats with a fixed number of tracks
    track_count: i32,
    /// Create new emulator for this type (useful in C++ only)
    new_emu: *const isize,
    /// Create new info reader for this type
    new_info: *const isize,

    pub extension: *const c_char,
    /// internal
    flags: i32,
}

//#[derive(Debug, Copy, Clone)]
type gme_type_t = *const gme_type_t_struct;

extern {
    /// Finish using emulator and free memory
    fn gme_delete(emu: *const MusicEmu);

    /// Determine likely game music type based on first four bytes of file. Returns string
    /// containing proper file suffix (i.e. "NSF", "SPC", etc.) or "" if file header is not
    /// recognized.
    fn gme_identify_header(header: *const u8) -> *const c_char;

    /// Get corresponding music type for file path or extension passed in.
    fn gme_identify_extension(extension: *const c_char) -> *const gme_type_t;

    /// Load music file from memory into emulator. Makes a copy of data passed.
    fn gme_load_data(emu: *const MusicEmu, data: *const u8, size: usize) -> *const c_char;

    /// Generate `count` 16-bit signed samples into `buffer`. Output is in stereo.
    fn gme_play(emu: *const MusicEmu, count: i32, out: *mut i16) -> *const c_char;

    /// Create new emulator and set sample rate.
    fn gme_new_emu(gme_type: *const gme_type_t, sample_rate: i32) -> *const MusicEmu;

    /// Start a track, where 0 is the first track
    fn gme_start_track(emu: *const MusicEmu, index: i32) -> *const c_char;

    /// Number of tracks available
    fn gme_track_count(emu: *const MusicEmu) -> i32;

    /// Pointer to array of all music types, with NULL entry at end.
    fn gme_type_list() -> *const gme_type_t;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_types() {
        let types = type_list();
        assert!(types.len() > 1);
    }

    #[test]
    fn test_open_data() {
        let mut file = File::open("test.nsf").unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();

        let handle = open_data(&buffer, 44100).ok().unwrap();
        assert_eq!(track_count(&handle), 1);
        start_track(&handle, 0);
    }

    #[test]
    fn test_open_file() {
        let handle = open_file("test.nsf", 44100).ok().unwrap();
        assert_eq!(track_count(&handle), 1);
        start_track(&handle, 0);
    }
}