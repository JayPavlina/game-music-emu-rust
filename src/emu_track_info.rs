#[derive(Clone, Default)]
pub struct EmuTrackInfo {
    pub length: Option<u32>,
    pub intro_length: Option<u32>,
    pub loop_length: Option<u32>,
    pub play_length: u32,
    pub system: Option<String>,
    pub game: Option<String>,
    pub song: Option<String>,
    pub author: Option<String>,
    pub copyright: Option<String>,
    pub comment: Option<String>,
    pub dumper: Option<String>,
}

impl EmuTrackInfo {
    pub fn new() -> Self {
        Self::default()
    }
}
