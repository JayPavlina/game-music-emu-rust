use crate::native::{Int, Long};

pub struct FileTypeInfo {
    /// name of system this music file type is generally for
    system: String,
    /// non-zero for formats with a fixed number of tracks
    track_count: Int,
    // Music_Emu* (*new_emu)();    /* Create new emulator for this type (useful in C++ only) */
    // Music_Emu* (*new_info)();   /* Create new info reader for this type */

    /* internal */
    extension: String,
    flags: Int,
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

pub struct GmeFile {
    type_info: FileTypeInfo,
    track_count: Int,
    raw_track_count: Int,
    warning: String,
    user_data: Vec<u8>,
    // gme_user_cleanup_t user_cleanup_;
    // M3u_Playlist playlist;
    // char playlist_warning [64];
    file_data: Vec<u8>, // only if loaded into memory using default load
}

impl GmeFile {

}