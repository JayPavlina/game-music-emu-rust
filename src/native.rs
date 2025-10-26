use crate::emu_equalizer::EmuEqualizer;
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

pub(crate) fn seek(handle: &EmuHandle, msec: u32) -> GmeResult<()> {
    unsafe { process_result(gme_seek(handle.to_raw(), msec as i32)) }
}

pub(crate) fn set_fade(handle: &EmuHandle, start_msec: u32) {
    unsafe { gme_set_fade(handle.to_raw(), start_msec as i32) }
}

pub(crate) fn set_stereo_depth(handle: &EmuHandle, depth: f64) {
    unsafe { gme_set_stereo_depth(handle.to_raw(), depth) }
}

pub(crate) fn ignore_silence(handle: &EmuHandle, ignore: bool) {
    unsafe { gme_ignore_silence(handle.to_raw(), ignore as i32) }
}

pub(crate) fn set_tempo(handle: &EmuHandle, tempo: f64) {
    unsafe { gme_set_tempo(handle.to_raw(), tempo) }
}

pub(crate) fn mute_voice(handle: &EmuHandle, index: u32, mute: bool) {
    unsafe { gme_mute_voice(handle.to_raw(), index as i32, mute as i32) }
}

pub(crate) fn mute_voices(handle: &EmuHandle, mask: i32) {
    unsafe { gme_mute_voices(handle.to_raw(), mask) }
}

pub(crate) fn voice_count(handle: &EmuHandle) -> u32 {
    unsafe { gme_voice_count(handle.to_raw()) as u32 }
}

pub(crate) fn voice_name(handle: &EmuHandle, index: u32) -> Option<String> {
    unsafe {
        let ptr = gme_voice_name(handle.to_raw(), index as i32);
        if ptr.is_null() {
            None
        } else {
            Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }
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

pub(crate) fn load_m3u(handle: &EmuHandle, path: impl AsRef<Path>) -> GmeResult<()> {
    let cstring = CString::new(
        path.as_ref()
            .to_str()
            .ok_or_else(|| GmeError::new("Invalid path".into()))?,
    )
    .map_err(|e| GmeError::new(format!("Failed to convert path to CString: {}", e)))?;
    unsafe { process_result(gme_load_m3u(handle.to_raw(), cstring.as_ptr())) }
}

pub(crate) fn load_m3u_data(handle: &EmuHandle, data: &[u8]) -> GmeResult<()> {
    unsafe {
        process_result(gme_load_m3u_data(
            handle.to_raw(),
            data.as_ptr(),
            data.len(),
        ))
    }
}

pub(crate) fn clear_playlist(handle: &EmuHandle) {
    unsafe { gme_clear_playlist(handle.to_raw()) }
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

pub(crate) fn equalizer(handle: &EmuHandle) -> EmuEqualizer {
    let mut gme_eq = gme_equalizer_t {
        treble: 0.0,
        bass: 0.0,
        d2: 0.0,
        d3: 0.0,
        d4: 0.0,
        d5: 0.0,
        d6: 0.0,
        d7: 0.0,
        d8: 0.0,
        d9: 0.0,
    };
    unsafe {
        gme_equalizer(handle.to_raw(), &mut gme_eq);
    }
    EmuEqualizer::from(gme_eq)
}

pub(crate) fn set_equalizer(handle: &EmuHandle, eq: EmuEqualizer) -> GmeResult<()> {
    let gme_eq = gme_equalizer_t {
        treble: eq.treble,
        bass: eq.bass,
        d2: 0.0,
        d3: 0.0,
        d4: 0.0,
        d5: 0.0,
        d6: 0.0,
        d7: 0.0,
        d8: 0.0,
        d9: 0.0,
    };
    unsafe {
        gme_set_equalizer(handle.to_raw(), &gme_eq);
    }
    Ok(())
}

pub(crate) fn enable_accuracy(handle: &EmuHandle, enable: bool) {
    unsafe { gme_enable_accuracy(handle.to_raw(), enable as i32) }
}

impl From<gme_equalizer_t> for EmuEqualizer {
    fn from(gme_eq: gme_equalizer_t) -> Self {
        Self {
            treble: gme_eq.treble,
            bass: gme_eq.bass,
        }
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

#[repr(C)]
pub(crate) struct gme_equalizer_t_struct {
    pub treble: f64,
    pub bass: f64,
    /* reserved */
    pub d2: f64,
    pub d3: f64,
    pub d4: f64,
    pub d5: f64,
    pub d6: f64,
    pub d7: f64,
    pub d8: f64,
    pub d9: f64,
}

#[allow(non_camel_case_types)]
type gme_equalizer_t = gme_equalizer_t_struct;

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

    /// Seek to specified position in milliseconds
    fn gme_seek(emu: *const MusicEmu, msec: i32) -> *const c_char;

    /// Set fade parameters
    fn gme_set_fade(emu: *const MusicEmu, start_msec: i32);

    /// Adjust stereo echo depth, where 0.0 = off and 1.0 = maximum. Has no effect for
    /// GYM, SPC, and Sega Genesis VGM music
    fn gme_set_stereo_depth(emu: *const MusicEmu, depth: f64);

    /// Disable automatic end-of-track detection and skipping of silence at beginning
    /// if ignore is true
    fn gme_ignore_silence(emu: *const MusicEmu, ignore: i32);

    /// Set tempo multiplier (1.0 = normal speed)
    fn gme_set_tempo(emu: *const MusicEmu, tempo: f64);

    /// Mute or unmute a specific voice
    fn gme_mute_voice(emu: *const MusicEmu, index: i32, mute: i32);

    /// Set muting state of all voices at once using a bit mask, where -1 mutes all
    /// voices, 0 unmutes them all, 0x01 mutes just the first voice, etc.
    fn gme_mute_voices(emu: *const MusicEmu, mask: i32);

    /// Get number of voices available in current track
    fn gme_voice_count(emu: *const MusicEmu) -> i32;

    /// Get name of specified voice
    fn gme_voice_name(emu: *const MusicEmu, index: i32) -> *const c_char;

    /// Load M3U playlist file
    fn gme_load_m3u(emu: *const MusicEmu, path: *const c_char) -> *const c_char;

    /// Load M3U playlist data from memory
    fn gme_load_m3u_data(emu: *const MusicEmu, data: *const u8, size: usize) -> *const c_char;

    /// Clear loaded playlist
    fn gme_clear_playlist(emu: *const MusicEmu);

    /// Get track info for specified track
    fn gme_track_info(emu: *const MusicEmu, out: *mut gme_info_t, track: i32) -> *const c_char;

    /// Free track info structure
    fn gme_free_info(info: gme_info_t);

    /// Get current equalizer settings
    fn gme_equalizer(emu: *const MusicEmu, out: *mut gme_equalizer_t);

    /// Set equalizer settings
    fn gme_set_equalizer(emu: *const MusicEmu, eq: *const gme_equalizer_t);

    /// Enable or disable high-accuracy emulation mode
    fn gme_enable_accuracy(emu: *const MusicEmu, enable: i32);
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
        assert_eq!(voice_count(&handle), 5);
        assert_eq!(voice_name(&handle, 0).as_deref(), Some("Square 1"));
        start_track(&handle, 0).unwrap();
    }

    #[test]
    fn test_open_file() {
        let handle = open_file(TEST_NSF_PATH, 44100).unwrap();
        assert_eq!(track_count(&handle), 1);
        assert_eq!(voice_count(&handle), 5);
        assert_eq!(voice_name(&handle, 0).as_deref(), Some("Square 1"));
        start_track(&handle, 0).unwrap();
    }

    #[test]
    fn test_seek_and_tell() {
        let handle = open_data(&get_test_nsf_data(), 44100).unwrap();
        start_track(&handle, 0).unwrap();
        seek(&handle, 10000).unwrap();
        assert!(tell(&handle) >= 10000);
    }

    #[test]
    fn test_open_m3u_data() {
        let handle = open_file(TEST_NSF_PATH, 44100).unwrap();
        assert_eq!(track_count(&handle), 1);
        load_m3u_data(&handle, &get_test_m3u_data()).unwrap();
        clear_playlist(&handle);
    }

    #[test]
    fn test_open_m3u_file() {
        let handle = open_file(TEST_NSF_PATH, 44100).unwrap();
        assert_eq!(track_count(&handle), 1);
        load_m3u(&handle, TEST_M3U_PATH).unwrap();
        clear_playlist(&handle);
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

    #[test]
    fn test_equalizer() {
        let handle = open_file(TEST_NSF_PATH, 44100).unwrap();
        let eq1 = equalizer(&handle);
        assert_eq!(eq1.bass, 80.0);
        let mut eq2 = eq1.clone();
        eq2.bass = 30.0;
        set_equalizer(&handle, eq2).unwrap();
        let eq3 = equalizer(&handle);
        assert_eq!(eq3.bass, 30.0);
    }
}
