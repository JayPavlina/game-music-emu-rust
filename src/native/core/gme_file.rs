use crate::native::{Int, Long, UInt};
use crate::native::core::m3u_playlist::M3uPlaylist;

#[derive(Default)]
pub struct EmuTypeInfo {
    /// name of system this music file type is generally for
    pub system_name: &'static str,
    /// non-zero for formats with a fixed number of tracks
    pub fixed_track_count: Option<UInt>,
    // Music_Emu* (*new_emu)();    /* Create new emulator for this type (useful in C++ only) */
    // Music_Emu* (*new_info)();   /* Create new info reader for this type */

    /* internal */
    pub extension: &'static str,
    pub flags: Int,
}

pub struct TrackInfo {
    track_count: Long,

    /// times in milliseconds; -1 if unknown
    length: Long,
    intro_length: Long,
    loop_length: Long,

    /// empty string if not available
    system: String,
    game: String,
    song: String,
    author: String,
    copyright: String,
    comment: String,
    dumper: String,
}

#[derive(Default)]
pub struct GmeFile {
    pub type_info: EmuTypeInfo,
    pub track_count: Int,
    pub raw_track_count: Int,
    pub warning: Option<String>,
    pub user_data: Vec<u8>,
    // gme_user_cleanup_t user_cleanup_;
    playlist: M3uPlaylist,
    // char playlist_warning [64];
    pub file_data: Vec<u8>, // only if loaded into memory using default load
}

impl GmeFile {
    pub fn clear_playlist(&mut self) {
        unimplemented!()
        // TODO
        // self.playlist.clear();
        // clear_playlist_();
        // track_count_ = raw_track_count_;
    }

    pub fn unload(&mut self) {
        // clear_playlist(); // *before* clearing track count
        self.track_count = 0;
        self.raw_track_count = 0;
        self.file_data.clear();
    }


}