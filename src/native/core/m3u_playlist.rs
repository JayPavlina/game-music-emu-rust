use crate::native::{UInt, Int};

#[derive(Default)]
pub struct PlaylistInfo {
    title: Vec<String>,
    composer: Vec<String>,
    engineer: Vec<String>,
    ripping: Vec<String>,
    tagging: Vec<String>,
}

#[derive(Default)]
pub struct PlaylistEntry {
    /// filename without stupid ::TYPE suffix
    file: String,
    /// if filename has ::TYPE suffix, this will be "TYPE". "" if none.
    r#type: String,
    name: String,
    /// true if track was specified in hex
    decimal_track: bool,
    // integers are -1 if not present
    /// 1-based
    track: Option<UInt>,
    /// seconds
    length: Option<UInt>,
    intro: Option<UInt>,
    r#loop: Option<UInt>,
    fade: Option<UInt>,
    /// count
    repeat: Option<UInt>,
}

#[derive(Default)]
pub struct M3uPlaylist {
    entries: Vec<PlaylistEntry>,
    data: Vec<u8>,
    first_error: Int,
    info: PlaylistInfo,
}