use crate::native::{DynamicResult, Sample};

mod blip_buffer;
mod classic_emu;
mod data_reader;
mod dual_resampler;
mod effects_buffer;
mod fir_resampler;
mod gme_file;
mod m3u_playlist;
mod multi_buffer;
mod music_emu;

pub use classic_emu::*;

pub struct MusicEmu {
    // general
    // equalizer_t equalizer_;
    max_initial_silence: i32,
    voice_names: String,
    voice_count: i32,
    mute_mask: i32,
    tempo: f64,
    gain: f64,
    multi_channel: bool,
    sample_rate: u64,

    // track-specific
    current_track: u32,
    out_time: i32,  // number of samples played since start of track
    emu_time: i32,  // number of samples emulator has generated since start of track
    emu_track_ended: bool, // emulator has reached end of track
    track_ended: bool,

    // fading
    fade_start: i32,
    fade_step: i32,

    // silence detection
    silence_lookahead: i32, // speed to run emulator when looking ahead for silence
    ignore_silence: bool,
    silence_time: i32,     // number of samples where most recent silence began
    silence_count: i32,    // number of samples of silence to play before using buf
    buf_remain: i32,       // number of samples left in silence buffer
    // enum { buf_size = 2048 };
    buf: Vec<Sample>
}

impl MusicEmu {
    pub fn set_sample_rate(&mut self) {

    }
}