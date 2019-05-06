extern crate libc;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::slice;
use crate::structures::{GmeType, EmuHandle, GmeError};
use crate::experimental::GmeResult;

pub fn identify_header(buffer: Vec<u8>) -> String {
    unsafe { CStr::from_ptr(gme_identify_header(buffer.as_ptr())).to_str().unwrap().to_string() }
}

pub fn get_track_count(handle: &EmuHandle) -> usize {
    unsafe { gme_track_count(handle.to_raw()) as usize }
}

pub fn delete(handle: &EmuHandle) {
    unsafe { gme_delete(handle.to_raw()); }
}

/// Same as gme_open_file(), but uses file data already in memory. Makes copy of data.
pub fn open_data(data: &[u8], sample_rate: u32) -> Result<EmuHandle, GmeError> {
    unsafe {
        let mut emu_ptr = std::mem::MaybeUninit::uninit().as_mut_ptr();
        let error = gme_open_data(data.as_ptr(), data.len(), emu_ptr, sample_rate as i32);
        if error.is_null() {
            Ok(EmuHandle::new(*emu_ptr))
        } else {
            Err(GmeError::new(CStr::from_ptr(error).to_str().unwrap().to_string()))
        }
    }
}

/// Load music file from memory into emulator. Makes a copy of data passed.
pub fn load_data(handle: &EmuHandle, data: &[u8]) -> GmeResult {
    unsafe {
        let mut emu_ptr: *const MusicEmu = std::ptr::null_mut();
        process_result(gme_load_data(handle.to_raw(), data.as_ptr(), data.len()))
    }
}

/// Generate 'count' 16-bit signed samples info 'out'. Output is in stereo.
pub fn play(handle: &EmuHandle, count: usize, buffer: &mut [i16]) -> GmeResult {
    unsafe {
        process_result(gme_play(handle.to_raw(), count as i32, buffer.as_mut_ptr()))
    }
}

pub fn start_track(handle: &EmuHandle, index: u32) -> GmeResult {
    unsafe { process_result(gme_start_track(handle.to_raw(), index as i32)) }
}

fn process_result(result: *const c_char) -> GmeResult {
    if result.is_null() {
        Ok(())
    } else {
        unsafe { Err(GmeError::new(CStr::from_ptr(result).to_str().unwrap().to_string())) }
    }
}

pub fn get_types() -> Vec<GmeType> {
    let mut types = Vec::new();
    unsafe {
        let mut p = gme_type_list();
        while *p != std::ptr::null() {
            let gme_type = p.clone().read();
            let extension = CStr::from_ptr((*gme_type).extension).to_str().unwrap();
            println!("extension: {}", extension);
            types.push(GmeType::from_extension(extension));
            p = p.offset(1);
        }
    }
    types
}

pub fn new_emu(gme_type: GmeType, sample_rate: i32) -> EmuHandle {
    unsafe {
        let cstring = CString::new(gme_type.to_extension()).unwrap();
        let gme_type = gme_identify_extension(cstring.as_ptr());
        let music_emu = gme_new_emu(gme_type, sample_rate);
        EmuHandle::new(music_emu)
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct MusicEmu { _private: isize }

// gme_type_t_ is struct
// gme_type_t holds pointer to other
//
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

    /// array of extensions
    pub extension: *const c_char,
    /// internal
    flags: i32,
}

//#[derive(Debug, Copy, Clone)]
type gme_type_t = *const gme_type_t_struct;

extern {
    /// Same as gme_open_file(), but uses file data already in memory. Makes copy of data.
    pub fn gme_open_data(data: *const u8, size: usize, out: *mut *mut MusicEmu, sample_rate: i32) -> *const c_char;

    /// Load music file from memory into emulator. Makes a copy of data passed.
    fn gme_load_data(emu: *const MusicEmu, data: *const u8, size: usize) -> *const c_char;

    /// Generate 'count' 16-bit signed samples info 'out'. Output is in stereo.
    fn gme_play(emu: *const MusicEmu, count: i32, out: *mut i16) -> *const c_char;

    /// Number of tracks available
    fn gme_track_count(emu: *const MusicEmu) -> i32;

    /// Finish using emulator and free memory
    fn gme_delete(emu: *const MusicEmu);

    /// Start a track, where 0 is the first track
    fn gme_start_track(emu: *const MusicEmu, index: i32) -> *const c_char;

    /// Create new emulator and set sample rate.
    fn gme_new_emu(gme_type: *const gme_type_t, sample_rate: i32) -> *const MusicEmu;


    /// Pointer to array of all music types, with NULL entry at end.
    /// Set in blarg_config
    fn gme_type_list() -> *const gme_type_t;

    fn gme_identify_extension(extension: *const c_char) -> *const gme_type_t;
    fn gme_identify_header(header: *const u8) -> *const c_char; // this is working
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::fs::File;

    #[test]
    fn test_get_types() {
        let types = get_types();
        assert!(types.len() > 1);
    }

    #[test]
    fn test_open_data() {
        let mut file = File::open("test.nsf").unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();

        let handle = open_data(&buffer, 44100).ok().unwrap();
        assert_eq!(get_track_count(&handle), 1);
    }
}