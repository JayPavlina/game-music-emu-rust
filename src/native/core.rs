use crate::native::{DynamicResult, Sample, Int, Double, Long, UInt};

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
pub use gme_file::*;
pub use blip_buffer::*;
use crate::native::core::gme_file::GmeFile;
use crate::native::core::multi_buffer::MultiBuffer;

pub struct MusicEmu {
    // general
    pub equalizer: Equalizer,
    pub file: GmeFile,
    pub max_initial_silence: Int,
    pub voice_names: Vec<String>,
    pub voice_count: Int,
    pub mute_mask: Int,
    pub tempo: Double,
    pub gain: Double,
    pub multi_channel: bool,
    // Set in constructor. Should never change.
    sample_rate: Long,

    // track-specific
    pub current_track: Option<UInt>,
    pub out_time: Long,
    // number of samples played since start of track
    pub emu_time: Long,
    // number of samples emulator has generated since start of track
    pub emu_track_ended: bool,
    // emulator has reached end of track
    pub track_ended: bool,

    // fading
    pub fade_start: Long,
    pub fade_step: Int,

    // silence detection
    pub silence_lookahead: Int,
    // speed to run emulator when looking ahead for silence
    pub ignore_silence: bool,
    pub silence_time: Long,
    // number of samples where most recent silence began
    pub silence_count: Long,
    // number of samples of silence to play before using buf
    pub buf_remain: Long,
    // number of samples left in silence buffer
    // enum { buf_size = 2048 };
    pub buf: Vec<Sample>,
    pub effects_buffer: MultiBuffer,
}

impl MusicEmu {
    pub fn new(sample_rate: Long) -> Self {
        Self {
            file: Default::default(),
            equalizer: Default::default(),
            max_initial_silence: 2,
            voice_names: vec!["Voice 1", "Voice 2", "Voice 3", "Voice 4",
                              "Voice 5", "Voice 6", "Voice 7", "Voice 8"].into_iter().map(|x| x.to_string()).collect(),
            voice_count: 0,
            mute_mask: 0,
            tempo: 1.0,
            gain: 1.0,
            multi_channel: false,
            sample_rate,
            current_track: None,
            out_time: 0,
            emu_time: 0,
            emu_track_ended: false,
            track_ended: false,
            fade_start: 0,
            fade_step: 0,
            silence_lookahead: 3,
            ignore_silence: false,
            silence_time: 0,
            silence_count: 0,
            buf_remain: 0,
            buf: vec![],
            effects_buffer: Default::default(),
        }
    }

    pub fn clear_track_vars(&mut self) {
        self.current_track = None;
        self.out_time = 0;
        self.emu_time = 0;
        self.emu_track_ended = true;
        self.track_ended = true;
        self.fade_start = Int::MAX / 2 + 1;
        self.fade_step = 1;
        self.silence_time = 0;
        self.silence_count = 0;
        self.buf_remain = 0;
        self.file.warning = None;
    }

    fn start_track(&mut self, track: Int) {
        self.clear_track_vars();

        let mut remapped = track;
        unimplemented!();
        // RETURN_ERR(remap_track_(&remapped));
        // current_track_ = track;
        // RETURN_ERR(start_track_(remapped));
        //
        // emu_track_ended_ = false;
        // track_ended_ = false;
        //
        // if (!ignore_silence_)
        // {
        //     // play until non-silence or end of track
        //     for (long end = max_initial_silence * out_channels() * sample_rate(); emu_time < end;)
        //     {
        //         fill_buf();
        //         if (buf_remain | (int)
        //         emu_track_ended_ )
        //         break;
        //     }
        //
        //     emu_time = buf_remain;
        //     out_time = 0;
        //     silence_time = 0;
        //     silence_count = 0;
        // }
        // return track_ended()?;
        // warning(): 0;
    }
}


#[derive(smart_default::SmartDefault)]
pub struct Equalizer {
    /// -50.0 = muffled, 0 = flat, +5.0 = extra-crisp
    #[default(_code = "-1.0")]
    treble: Double,
    /// 1 = full bass, 90 = average, 16000 = almost no bass
    #[default = 60.0]
    bass: Double,
    // reserved
    d2: Double,
    d3: Double,
    d4: Double,
    d5: Double,
    d6: Double,
    d7: Double,
    d8: Double,
    d9: Double,
}

impl Equalizer {
    pub const fn new(treble: Double, bass: Double) -> Self {
        Self {
            treble,
            bass,
            d2: 0.0,
            d3: 0.0,
            d4: 0.0,
            d5: 0.0,
            d6: 0.0,
            d7: 0.0,
            d8: 0.0,
            d9: 0.0,
        }
    }
}