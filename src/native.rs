use crate::emu_track_info::EmuTrackInfo;
use crate::emu_type::EmuType;
use crate::error::{GmeError, GmeOrIoError, GmeResult};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Arc;

/// Holds a pointer to a `MusicEmu` instance in the C++ code. It automatically frees the instance
/// when dropped.
#[derive(Clone)]
pub(crate) struct EmuHandle {
    pub(crate) emu: Arc<MusicEmu>,
}

impl EmuHandle {
    pub(crate) fn new(emu: *const MusicEmu) -> Self {
        #[allow(clippy::crosspointer_transmute)]
        unsafe {
            Self {
                emu: Arc::new(std::mem::transmute::<*const MusicEmu, MusicEmu>(emu)),
            }
        }
    }

    pub(crate) fn to_raw(&self) -> *const MusicEmu {
        unsafe { std::mem::transmute_copy(&*self.emu) }
    }
}

impl Drop for EmuHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.emu) == 1 {
            delete(self);
        }
    }
}

pub(crate) fn delete(handle: &EmuHandle) {
    unsafe {
        gme_delete(handle.to_raw());
    }
}

/// Determine likely `EmuType` based on first four bytes of file.
pub fn identify_header(buffer: &[u8]) -> EmuType {
    unsafe {
        EmuType::from_extension(
            CStr::from_ptr(gme_identify_header(buffer.as_ptr()))
                .to_str()
                .unwrap(),
        )
    }
}

/// Load music file from memory into emulator. Makes a copy of data passed.
pub(crate) fn load_data(handle: &EmuHandle, data: &[u8]) -> GmeResult<()> {
    unsafe {
        // let mut emu_ptr: *const MusicEmu = std::ptr::null_mut();
        process_result(gme_load_data(handle.to_raw(), data.as_ptr(), data.len()))
    }
}

/// Load music file into emulator
pub(crate) fn load_file(handle: &EmuHandle, path: impl AsRef<Path>) -> Result<(), GmeOrIoError> {
    let buffer = get_file_data(path)?;
    Ok(load_data(handle, &buffer)?)
}

/// Creates an `EmuHandle` with the specified `EmuType`
pub(crate) fn new_emu(emu_type: EmuType, sample_rate: u32) -> EmuHandle {
    unsafe {
        let cstring = CString::new(emu_type.to_extension()).unwrap();
        let gme_type = gme_identify_extension(cstring.as_ptr());
        let music_emu = gme_new_emu(gme_type, sample_rate as i32);
        EmuHandle::new(music_emu)
    }
}

pub(crate) fn open_data(data: &[u8], sample_rate: u32) -> GmeResult<EmuHandle> {
    let emu_type = identify_header(data);
    let handle = new_emu(emu_type, sample_rate);
    load_data(&handle, data)?;
    Ok(handle)
}

pub(crate) fn open_file(
    path: impl AsRef<Path>,
    sample_rate: u32,
) -> Result<EmuHandle, GmeOrIoError> {
    let buffer = get_file_data(path)?;
    Ok(open_data(&buffer, sample_rate)?)
}

pub(crate) fn play(handle: &EmuHandle, count: usize, buffer: &mut [i16]) -> Result<(), GmeError> {
    unsafe { process_result(gme_play(handle.to_raw(), count as i32, buffer.as_mut_ptr())) }
}

pub(crate) fn start_track(handle: &EmuHandle, index: u32) -> GmeResult<()> {
    unsafe { process_result(gme_start_track(handle.to_raw(), index as i32)) }
}

pub(crate) fn tell(handle: &EmuHandle) -> u32 {
    unsafe { gme_tell(handle.to_raw()) as u32 }
}

pub(crate) fn track_count(handle: &EmuHandle) -> usize {
    unsafe { gme_track_count(handle.to_raw()) as usize }
}

pub(crate) fn track_ended(handle: &EmuHandle) -> bool {
    unsafe { gme_track_ended(handle.to_raw()) }
}

/// Returns all of the supported `EmuTypes`. This is based on the features the crate is compiled
/// with.
pub fn type_list() -> Vec<EmuType> {
    let mut types = Vec::new();
    unsafe {
        let mut p = gme_type_list();
        while !(*p).is_null() {
            let gme_type = p.read();
            let extension = CStr::from_ptr((*gme_type).extension).to_str().unwrap();
            types.push(EmuType::from_extension(extension));
            p = p.offset(1);
        }
    }
    types
}

fn process_result(result: *const c_char) -> GmeResult<()> {
    if result.is_null() {
        Ok(())
    } else {
        unsafe {
            Err(GmeError::new(
                CStr::from_ptr(result).to_str().unwrap().to_string(),
            ))
        }
    }
}

pub(crate) fn get_file_data(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

impl From<gme_info_t> for EmuTrackInfo {
    fn from(info: gme_info_t) -> Self {
        unsafe {
            let info_ref = &*info;
            EmuTrackInfo {
                length: if info_ref.length >= 0 {
                    Some(info_ref.length as u32)
                } else {
                    None
                },
                intro_length: if info_ref.intro_length >= 0 {
                    Some(info_ref.intro_length as u32)
                } else {
                    None
                },
                loop_length: if info_ref.loop_length >= 0 {
                    Some(info_ref.loop_length as u32)
                } else {
                    None
                },
                play_length: info_ref.play_length as u32,
                system: if !info_ref.system.is_null() {
                    Some(
                        CStr::from_ptr(info_ref.system)
                            .to_string_lossy()
                            .into_owned(),
                    )
                } else {
                    None
                },
                game: if !info_ref.game.is_null() {
                    Some(CStr::from_ptr(info_ref.game).to_string_lossy().into_owned())
                } else {
                    None
                },
                song: if !info_ref.song.is_null() {
                    Some(CStr::from_ptr(info_ref.song).to_string_lossy().into_owned())
                } else {
                    None
                },
                author: if !info_ref.author.is_null() {
                    Some(
                        CStr::from_ptr(info_ref.author)
                            .to_string_lossy()
                            .into_owned(),
                    )
                } else {
                    None
                },
                copyright: if !info_ref.copyright.is_null() {
                    Some(
                        CStr::from_ptr(info_ref.copyright)
                            .to_string_lossy()
                            .into_owned(),
                    )
                } else {
                    None
                },
                comment: if !info_ref.comment.is_null() {
                    Some(
                        CStr::from_ptr(info_ref.comment)
                            .to_string_lossy()
                            .into_owned(),
                    )
                } else {
                    None
                },
                dumper: if !info_ref.dumper.is_null() {
                    Some(
                        CStr::from_ptr(info_ref.dumper)
                            .to_string_lossy()
                            .into_owned(),
                    )
                } else {
                    None
                },
            }
        }
    }
}

pub(crate) fn track_info(handle: &EmuHandle, track: u32) -> GmeResult<EmuTrackInfo> {
    unsafe {
        let mut info_ptr: gme_info_t = std::ptr::null_mut();
        let err = gme_track_info(handle.to_raw(), &mut info_ptr, track as i32);
        process_result(err)?;
        let info = EmuTrackInfo::from(info_ptr);
        free_info(info_ptr);
        Ok(info)
    }
}

pub(crate) fn free_info(info: gme_info_t) {
    unsafe {
        gme_free_info(info);
    }
}

#[repr(C)]
#[derive(Clone)]
pub(crate) struct MusicEmu {
    _private: isize,
}

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

#[repr(C)]
pub(crate) struct gme_info_t_struct {
    pub length: i32,
    pub intro_length: i32,
    pub loop_length: i32,
    pub play_length: i32,
    /* reserved */
    pub i4: i32,
    pub i5: i32,
    pub i6: i32,
    pub i7: i32,
    pub i8: i32,
    pub i9: i32,
    pub i10: i32,
    pub i11: i32,
    pub i12: i32,
    pub i13: i32,
    pub i14: i32,
    pub i15: i32,

    pub system: *const c_char,
    pub game: *const c_char,
    pub song: *const c_char,
    pub author: *const c_char,
    pub copyright: *const c_char,
    pub comment: *const c_char,
    pub dumper: *const c_char,
    /* reserved */
    pub s7: *const c_char,
    pub s8: *const c_char,
    pub s9: *const c_char,
    pub s10: *const c_char,
    pub s11: *const c_char,
    pub s12: *const c_char,
    pub s13: *const c_char,
    pub s14: *const c_char,
    pub s15: *const c_char,
}

#[allow(non_camel_case_types)]
type gme_info_t = *mut gme_info_t_struct;

unsafe extern "C" {
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

    /// Number of milliseconds played since beginning of track
    fn gme_tell(emu: *const MusicEmu) -> i32;

    /// Number of tracks available
    fn gme_track_count(emu: *const MusicEmu) -> i32;

    /// True if a track has reached its end
    fn gme_track_ended(emu: *const MusicEmu) -> bool;

    /// Pointer to array of all music types, with NULL entry at end.
    fn gme_type_list() -> *const gme_type_t;

    /// Get track info for specified track
    fn gme_track_info(emu: *const MusicEmu, out: *mut gme_info_t, track: i32) -> *const c_char;

    /// Free track info structure
    fn gme_free_info(info: gme_info_t);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[test]
    fn test_get_types() {
        let types = type_list();
        assert!(types.len() > 1);
    }

    #[test]
    fn test_open_data() {
        let handle = open_data(&get_test_nsf_data(), 44100).unwrap();
        assert_eq!(track_count(&handle), 1);
        start_track(&handle, 0).unwrap();
    }

    #[test]
    fn test_open_file() {
        let handle = open_file(TEST_NSF_PATH, 44100).unwrap();
        assert_eq!(track_count(&handle), 1);
        start_track(&handle, 0).unwrap();
    }

    #[test]
    fn test_track_info() {
        let handle = open_data(&get_test_nsf_data(), 44100).unwrap();
        unsafe {
            let mut info_ptr: gme_info_t = std::ptr::null_mut();
            let err = gme_track_info(handle.to_raw(), &mut info_ptr, 0);
            assert!(err.is_null());
            let info = &*info_ptr;
            let length = info.length;
            let intro_length = info.intro_length;
            let loop_length = info.loop_length;
            let play_length = info.play_length;
            let system = CStr::from_ptr(info.system).to_str().unwrap();
            let game = CStr::from_ptr(info.game).to_str().unwrap();
            let song = CStr::from_ptr(info.song).to_str().unwrap();
            let author = CStr::from_ptr(info.author).to_str().unwrap();
            let copyright = CStr::from_ptr(info.copyright).to_str().unwrap();
            let comment = CStr::from_ptr(info.comment).to_str().unwrap();
            let dumper = CStr::from_ptr(info.dumper).to_str().unwrap();
            assert_eq!(length, -1);
            assert_eq!(intro_length, -1);
            assert_eq!(loop_length, -1);
            assert_eq!(play_length, 150000);
            assert_eq!(system, "Nintendo NES");
            assert_eq!(game, "Tetris (GB)");
            assert_eq!(song, "");
            assert_eq!(author, "");
            assert_eq!(copyright, "Nintendo");
            assert_eq!(comment, "");
            assert_eq!(dumper, "");
            gme_free_info(info_ptr);
        }
    }
}
