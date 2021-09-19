use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use crate::emu_type::{EmuType};
use std::path::Path;
use crate::error::{GmeOrIoError, GmeError, GmeResult};
use std::sync::Arc;
use std::mem::{transmute, transmute_copy};

/// Holds a pointer to a `MusicEmu` instance in the C++ code. It automatically frees the instance
/// when dropped.
#[derive(Clone)]
pub(crate) struct EmuHandle {
    pub(crate) emu: Arc<BridgeMusicEmu>
}

impl EmuHandle {
    pub(crate) fn new(emu: *const BridgeMusicEmu) -> Self {
        unsafe { Self { emu: Arc::new(transmute(emu)) } }
    }

    pub(crate) fn to_raw(&self) -> *const BridgeMusicEmu { unsafe { transmute_copy(&*self.emu) } }
}

impl Drop for EmuHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.emu) == 1 {
            delete(self);
        }
    }
}

pub(crate) fn delete(handle: &EmuHandle) {
    unimplemented!()
    // unsafe { gme_delete(handle.to_raw()); }
}

/// Determine likely `EmuType` based on first four bytes of file.
pub fn identify_header(buffer: &[u8]) -> EmuType {
    unimplemented!()
    // unsafe {
    //     EmuType::from_extension(&CStr::from_ptr(gme_identify_header(buffer.as_ptr())).to_str()
    //         .unwrap().to_string())
    // }
}

/// Load music file from memory into emulator. Makes a copy of data passed.
pub(crate) fn load_data(handle: &EmuHandle, data: &[u8]) -> GmeResult<()> {
    unimplemented!()
    // unsafe {
    //     process_result(gme_load_data(handle.to_raw(), data.as_ptr(), data.len()))
    // }
}

/// Load music file into emulator
pub(crate) fn load_file(handle: &EmuHandle, path: impl AsRef<Path>) -> Result<(), GmeOrIoError> {
    let buffer = get_file_data(path)?;
    Ok(load_data(handle, &buffer)?)
}

/// Creates an `EmuHandle` with the specified `EmuType`
pub(crate) fn new_emu(emu_type: EmuType, sample_rate: u32) -> EmuHandle {
    unimplemented!()
    // unsafe {
    //     let cstring = CString::new(emu_type.to_extension()).unwrap();
    //     let gme_type = gme_identify_extension(cstring.as_ptr());
    //     let music_emu = gme_new_emu(gme_type, sample_rate as i32);
    //     EmuHandle::new(music_emu)
    // }
}


pub(crate) fn open_data(data: &[u8], sample_rate: u32) -> GmeResult<EmuHandle> {
    let emu_type = identify_header(data);
    let handle = new_emu(emu_type, sample_rate);
    load_data(&handle, data)?;
    Ok(handle)
}

pub(crate) fn open_file(path: impl AsRef<Path>, sample_rate: u32) -> Result<EmuHandle, GmeOrIoError> {
    let buffer = get_file_data(path)?;
    Ok(open_data(&buffer, sample_rate)?)
}

pub(crate) fn play(handle: &EmuHandle, count: usize, buffer: &mut [i16]) -> Result<(), GmeError> {
    unimplemented!()
    // unsafe { process_result(gme_play(handle.to_raw(), count as i32, buffer.as_mut_ptr())) }
}

pub(crate) fn start_track(handle: &EmuHandle, index: u32) -> GmeResult<()> {
    unimplemented!()
    // unsafe { process_result(gme_start_track(handle.to_raw(), index as i32)) }
}

pub(crate) fn tell(handle: &EmuHandle) -> u32 {
    unimplemented!()
    // unsafe { gme_tell(handle.to_raw()) as u32 }
}

pub(crate) fn track_count(handle: &EmuHandle) -> usize {
    unimplemented!()
    // unsafe { gme_track_count(handle.to_raw()) as usize }
}

pub(crate) fn track_ended(handle: &EmuHandle) -> bool {
    unimplemented!()
    // unsafe { gme_track_ended(handle.to_raw()) }
}

/// Returns all of the supported `EmuTypes`. This is based on the features the crate is compiled
/// with.
pub fn type_list() -> Vec<EmuType> {
    unimplemented!()
    // let mut types = Vec::new();
    // unsafe {
    //     let mut p = gme_type_list();
    //     while *p != std::ptr::null() {
    //         let gme_type = p.clone().read();
    //         let extension = CStr::from_ptr((*gme_type).extension).to_str().unwrap();
    //         types.push(EmuType::from_extension(extension));
    //         p = p.offset(1);
    //     }
    // }
    // types
}

fn process_result(result: *const c_char) -> GmeResult<()> {
    if result.is_null() {
        Ok(())
    } else {
        unsafe { Err(GmeError::new(CStr::from_ptr(result).to_str().unwrap().to_string())) }
    }
}

pub(crate) fn get_file_data(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

#[repr(C)]
#[derive(Clone)]
pub(crate) struct BridgeMusicEmu { _private: isize }

// gme_type_t_ is struct
// gme_type_t holds pointer to other
#[repr(C)]
pub(crate) struct gme_type_t_struct {
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
#[allow(non_camel_case_types)]
type gme_type_t = *const gme_type_t_struct;

// extern {
//     /// Finish using emulator and free memory
//     fn gme_delete(emu: *const BridgeMusicEmu);
//
//     /// Determine likely game music type based on first four bytes of file. Returns string
//     /// containing proper file suffix (i.e. "NSF", "SPC", etc.) or "" if file header is not
//     /// recognized.
//     fn gme_identify_header(header: *const u8) -> *const c_char;
//
//     /// Get corresponding music type for file path or extension passed in.
//     fn gme_identify_extension(extension: *const c_char) -> *const gme_type_t;
//
//     /// Load music file from memory into emulator. Makes a copy of data passed.
//     fn gme_load_data(emu: *const BridgeMusicEmu, data: *const u8, size: usize) -> *const c_char;
//
//     /// Generate `count` 16-bit signed samples into `buffer`. Output is in stereo.
//     fn gme_play(emu: *const BridgeMusicEmu, count: i32, out: *mut i16) -> *const c_char;
//
//     /// Create new emulator and set sample rate.
//     fn gme_new_emu(gme_type: *const gme_type_t, sample_rate: i32) -> *const BridgeMusicEmu;
//
//     /// Start a track, where 0 is the first track
//     fn gme_start_track(emu: *const BridgeMusicEmu, index: i32) -> *const c_char;
//
//     /// Number of milliseconds played since beginning of track
//     fn gme_tell(emu: *const BridgeMusicEmu) -> i32;
//
//     /// Number of tracks available
//     fn gme_track_count(emu: *const BridgeMusicEmu) -> i32;
//
//     /// True if a track has reached its end
//     fn gme_track_ended(emu: *const BridgeMusicEmu) -> bool;
//
//     /// Pointer to array of all music types, with NULL entry at end.
//     fn gme_type_list() -> *const gme_type_t;
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::test_utils::*;
//
//     #[test]
//     fn test_get_types() {
//         let types = type_list();
//         assert!(types.len() > 1);
//     }
//
//     #[test]
//     fn test_open_data() {
//         let handle = open_data(&get_test_nsf_data(), 44100).unwrap();
//         assert_eq!(track_count(&handle), 1);
//         start_track(&handle, 0).unwrap();
//     }
//
//     #[test]
//     fn test_open_file() {
//         let handle = open_file(TEST_NSF_PATH, 44100).unwrap();
//         assert_eq!(track_count(&handle), 1);
//         start_track(&handle, 0).unwrap();
//     }
// }