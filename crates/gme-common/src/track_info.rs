/// Maximum field length in track info.
pub const GME_MAX_FIELD: usize = 255;

/// Track metadata. Replaces C++ `track_info_t`.
#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub track_count: i64,
    /// Length in milliseconds, -1 if unknown.
    pub length: i64,
    pub intro_length: i64,
    pub loop_length: i64,
    pub fade_length: i64,
    pub repeat_count: i64,
    /// Computed play length.
    pub play_length: i64,
    pub system: String,
    pub game: String,
    pub song: String,
    pub author: String,
    pub composer: String,
    pub engineer: String,
    pub sequencer: String,
    pub tagger: String,
    pub copyright: String,
    pub date: String,
    pub comment: String,
    pub dumper: String,
    pub disc: String,
    pub track: String,
    pub ost: String,
}

impl Default for TrackInfo {
    fn default() -> Self {
        Self {
            track_count: 0,
            length: -1,
            intro_length: -1,
            loop_length: -1,
            fade_length: -1,
            repeat_count: -1,
            play_length: -1,
            system: String::new(),
            game: String::new(),
            song: String::new(),
            author: String::new(),
            composer: String::new(),
            engineer: String::new(),
            sequencer: String::new(),
            tagger: String::new(),
            copyright: String::new(),
            date: String::new(),
            comment: String::new(),
            dumper: String::new(),
            disc: String::new(),
            track: String::new(),
            ost: String::new(),
        }
    }
}

impl TrackInfo {
    pub fn new(track_count: i64) -> Self {
        Self {
            track_count,
            ..Default::default()
        }
    }

    /// Compute play_length from length/intro/loop fields.
    pub fn compute_play_length(&mut self) {
        if self.length >= 0 {
            self.play_length = self.length;
        } else if self.intro_length >= 0 && self.loop_length >= 0 {
            self.play_length = self.intro_length + self.loop_length * 2;
        } else {
            self.play_length = 150_000; // default 2.5 minutes
        }
    }
}

/// Copy a field string, trimming whitespace and junk.
pub fn copy_field(out: &mut String, input: &str) {
    if input.is_empty() {
        return;
    }
    let trimmed = input.trim();
    let truncated = if trimmed.len() > GME_MAX_FIELD {
        &trimmed[..trimmed.floor_char_boundary(GME_MAX_FIELD)]
    } else {
        trimmed
    };
    // Strip out placeholder fields
    if truncated == "?" || truncated == "<?>" || truncated == "< ? >" {
        out.clear();
    } else {
        *out = truncated.to_string();
    }
}

/// Equalizer parameters.
#[derive(Clone, Debug, Copy)]
pub struct Equalizer {
    pub treble: f64,
    pub bass: f64,
}

impl Default for Equalizer {
    fn default() -> Self {
        Self {
            treble: -1.0,
            bass: 60.0,
        }
    }
}

impl Equalizer {
    pub fn new(treble: f64, bass: f64) -> Self {
        Self { treble, bass }
    }

    /// TV speaker preset.
    pub fn tv() -> Self {
        Self::new(-8.0, 180.0)
    }
}
