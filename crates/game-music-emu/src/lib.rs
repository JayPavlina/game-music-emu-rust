//! # Game Music Emu (Pure Rust)
//!
//! A pure Rust implementation of [Game Music Emu](https://bitbucket.org/mpyne/game-music-emu/),
//! a library for playing music from retro video game consoles.
//!
//! The facade source uses MIT. Playback links LGPL-2.1-or-later components.
//! See `LICENSING.md` in this crate for notices and distribution guidance.
//!
//! ## Supported Formats
//!
//! | Format | System | Feature Flag |
//! |--------|--------|-------------|
//! | NSF, NSFE | Nintendo NES | `nsf` |
//! | SPC | Super Nintendo | `spc` |
//! | GBS | Game Boy | `gbs` |
//! | VGM | Sega SMS/Genesis | `vgm` |
//! | SAP | Atari XL | `sap` |
//! | KSS | MSX | `kss` |
//! | AY | ZX Spectrum | `ay` |
//! | GYM | Sega Genesis | `gym` |
//! | HES | PC Engine | `hes` |
//! | VGZ | gzipped VGM | `vgm` |
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use game_music_emu::GameMusicEmu;
//!
//! let mut emu = GameMusicEmu::from_file("assets/test.nsf", 44100).unwrap();
//! emu.start_track(0).unwrap();
//!
//! let mut samples = vec![0i16; 4410 * 2]; // 0.1 seconds, stereo
//! emu.play(&mut samples).unwrap();
//! ```
//!
//! ## Feature Flags
//!
//! Enable or disable format support to reduce compile time and binary size:
//!
//! ```toml
//! game-music-emu = { version = "0.5", default-features = false, features = ["nsf"] }
//! ```

pub use gme_common::format::Format;
pub use gme_common::music_emu::Ym2612Backend;
pub use gme_common::track_info::{Equalizer, TrackInfo};
pub use gme_core::error::GmeError;

/// Result type used throughout the library.
pub type Result<T> = std::result::Result<T, GmeError>;

use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator};
use std::path::Path;

/// Recognize a music file by its header, including formats disabled in this build.
pub fn identify_header(data: &[u8]) -> Option<Format> {
    Format::identify_header(data)
}

trait Decoder: GmeFile + MusicEmulator {
    fn file(&self) -> &GmeFileState;
    fn file_mut(&mut self) -> &mut GmeFileState;
    fn playback(&self) -> &MusicEmuState;
    fn playback_mut(&mut self) -> &mut MusicEmuState;
}

// Keep the set of movable, heap-allocated decoders closed. Their internal
// pointers refer to their own buffers and are initialized at the final address.
macro_rules! decoders {
    ($( $feature:literal => $ty:path, [$($format:ident),+]; )*) => {
        $(
            #[cfg(feature = $feature)]
            impl Decoder for $ty {
                fn file(&self) -> &GmeFileState { &self.state }
                fn file_mut(&mut self) -> &mut GmeFileState { &mut self.state }
                fn playback(&self) -> &MusicEmuState { &self.emu_state }
                fn playback_mut(&mut self) -> &mut MusicEmuState { &mut self.emu_state }
            }
        )*

        /// Formats enabled by the current Cargo features.
        pub fn type_list() -> &'static [Format] {
            &[$($(#[cfg(feature = $feature)] Format::$format,)+)*]
        }

        fn decoder(format: Format) -> Result<Box<dyn Decoder>> {
            match format {
                $(#[cfg(feature = $feature)]
                $(Format::$format)|+ => Ok(Box::new(<$ty>::new())),)*
                #[allow(unreachable_patterns)]
                _ => Err(GmeError::Internal("Format not supported (feature not enabled)")),
            }
        }
    }
}

decoders! {
    "ay" => gme_ay::ay_emu::AyEmu, [Ay];
    "gbs" => gme_gbs::gbs_emu::GbsEmu, [Gbs];
    "gym" => gme_gym::gym_emu::GymEmu, [Gym];
    "hes" => gme_hes::hes_emu::HesEmu, [Hes];
    "kss" => gme_kss::kss_emu::KssEmu, [Kss];
    "nsf" => gme_nsf::nsf_emu::NsfEmu, [Nsf, Nsfe];
    "sap" => gme_sap::sap_emu::SapEmu, [Sap];
    "spc" => gme_spc::spc_emu::SpcEmu, [Spc];
    "vgm" => gme_vgm::vgm_emu::VgmEmu, [Vgm, Vgz];
}

struct CollectionTrack {
    data: Vec<u8>,
    info: TrackInfo,
}

/// A pure Rust emulator producing interleaved PCM samples.
pub struct GameMusicEmu {
    format: Format,
    sample_rate: i32,
    inner: Box<dyn Decoder>,
    loaded: bool,
    current_track: Option<i32>,
    stereo_depth: f64,
    accuracy: bool,
    backend: Ym2612Backend,
    echo_disabled: bool,
    collection: Vec<CollectionTrack>,
}

// SAFETY: decoder() only constructs the owned decoders listed above. Moving
// this handle transfers exclusive ownership without moving the decoder itself.
unsafe impl Send for GameMusicEmu {}

impl GameMusicEmu {
    /// Create an unloaded emulator for a format enabled in this build.
    pub fn new(format: Format, sample_rate: u32) -> Result<Self> {
        if sample_rate == 0 || sample_rate > i32::MAX as u32 / 2 {
            return Err(GmeError::Internal("Invalid sample rate"));
        }
        Self::create(format, sample_rate)
    }

    /// Select a YM2612 synthesis core for GYM, VGM or VGZ before loading.
    pub fn new_with_ym2612(
        format: Format,
        sample_rate: u32,
        backend: Ym2612Backend,
    ) -> Result<Self> {
        let mut player = Self::new(format, sample_rate)?;
        player.inner.select_ym2612(backend)?;
        player.backend = backend;
        Ok(player)
    }

    /// Read track metadata and playlists without setting up audio playback.
    pub fn new_info_only(format: Format) -> Result<Self> {
        Self::create(format, 0)
    }

    /// Render eight interleaved stereo pairs for the classic chip decoders.
    pub fn new_multi_channel(format: Format, sample_rate: u32) -> Result<Self> {
        let mut emu = Self::new(format, sample_rate)?;
        emu.inner.enable_multi_channel()?;
        Ok(emu)
    }

    fn create(format: Format, sample_rate: u32) -> Result<Self> {
        let mut inner = decoder(format)?;
        inner.playback_mut().equalizer = match format {
            Format::Nsf | Format::Nsfe => Equalizer::new(-1.0, 80.0),
            Format::Gbs => Equalizer::new(-1.0, 120.0),
            Format::Vgm | Format::Vgz => Equalizer::new(-14.0, 80.0),
            _ => Equalizer::default(),
        };
        Ok(Self {
            format,
            sample_rate: sample_rate as i32,
            inner,
            loaded: false,
            current_track: None,
            stereo_depth: 0.0,
            accuracy: false,
            backend: Ym2612Backend::Nuked,
            echo_disabled: false,
            collection: Vec::new(),
        })
    }

    /// Load a file, using its extension for headerless formats such as GYM.
    pub fn from_file(path: impl AsRef<Path>, sample_rate: u32) -> Result<Self> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|_| GmeError::OpenFailed)?;
        let format = identify_header(&data)
            .or_else(|| Format::identify_extension(path.extension()?.to_str()?))
            .ok_or(GmeError::WrongFileType)?;
        let mut emu = Self::new(format, sample_rate)?;
        emu.load_data(&data)?;
        Ok(emu)
    }

    /// Auto-detect the format and load music from memory.
    pub fn from_data(data: &[u8], sample_rate: u32) -> Result<Self> {
        let format = identify_header(data).ok_or(GmeError::WrongFileType)?;
        let mut emu = Self::new(format, sample_rate)?;
        emu.load_data(data)?;
        Ok(emu)
    }

    /// Replace the music in this emulator while retaining playback preferences.
    pub fn load_data(&mut self, data: impl AsRef<[u8]>) -> Result<()> {
        self.inner = self.prepare_decoder(data.as_ref(), self.sample_rate)?;
        self.current_track = None;
        self.collection.clear();
        self.loaded = true;
        Ok(())
    }

    fn prepare_decoder(&self, data: &[u8], sample_rate: i32) -> Result<Box<dyn Decoder>> {
        use std::io::Read;
        let mut plain = Vec::new();
        let data = if matches!(self.format, Format::Vgm | Format::Vgz)
            && data.starts_with(&[0x1f, 0x8b])
        {
            flate2::read::GzDecoder::new(data)
                .read_to_end(&mut plain)
                .map_err(|_| GmeError::WrongFileType)?;
            &plain
        } else {
            data
        };

        let mut next = decoder(self.format)?;
        if self.multi_channel() {
            next.enable_multi_channel()?;
        }
        if self.backend != Ym2612Backend::Nuked {
            next.select_ym2612(self.backend)?;
        }
        next.load_mem_impl(data)?;
        if sample_rate == 0 {
            return Ok(next);
        }
        next.set_sample_rate_impl(sample_rate)?;
        let state = self.inner.playback();
        next.playback_mut().equalizer = state.equalizer;
        next.set_equalizer_impl(&state.equalizer);
        next.playback_mut().ignore_silence_flag = state.ignore_silence_flag;
        next.playback_mut().set_tempo_range(state.tempo);
        next.set_tempo_impl(state.tempo);
        next.set_stereo_depth_impl(self.stereo_depth);
        next.enable_accuracy_impl(self.accuracy);
        next.playback_mut().mute_mask = state.mute_mask;
        next.mute_voices_impl(state.mute_mask);
        next.playback_mut().autoload_playback_limit = state.autoload_playback_limit;
        next.disable_echo_impl(self.echo_disabled);
        Ok(next)
    }

    /// Load or replace music from disk using this emulator's format.
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.load_data(std::fs::read(path).map_err(|_| GmeError::OpenFailed)?)
    }

    /// Load a custom reader, including readers that return short reads.
    pub fn load_reader(&mut self, mut reader: impl std::io::Read) -> Result<()> {
        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .map_err(|_| GmeError::ReadError)?;
        self.load_data(data)
    }

    /// Combine files of the same single-track format into an indexed collection.
    /// Files are owned by the player. A failed replacement keeps the old music.
    pub fn load_tracks(&mut self, files: &[impl AsRef<[u8]>]) -> Result<()> {
        if self.format.info().track_count != 1 || files.is_empty() {
            return Err(GmeError::Internal(
                "Expected one or more single-track files",
            ));
        }
        let count = i32::try_from(files.len()).map_err(|_| GmeError::OutOfMemory)?;
        let mut tracks = Vec::with_capacity(files.len());
        for file in files {
            let data = file.as_ref();
            let entry = self.prepare_decoder(data, 0)?;
            let mut info = TrackInfo::default();
            entry
                .file()
                .track_info_with_m3u(&mut info, 0, self.format, |out, index| {
                    entry.track_info_impl(out, index)
                })?;
            tracks.push(CollectionTrack {
                data: data.to_vec(),
                info,
            });
        }
        self.load_data(&tracks[0].data)?;
        self.inner.file_mut().set_track_count(count);
        self.collection = tracks;
        Ok(())
    }

    /// Retrieve and clear the latest nonfatal decoder or playlist warning.
    pub fn warning(&mut self) -> Option<String> {
        let file = self.inner.file_mut();
        let warning = file.warning()?;
        Some(if warning == "Problem in m3u playlist" {
            std::mem::take(&mut file.playlist_warning)
        } else {
            warning.to_owned()
        })
    }

    pub fn multi_channel(&self) -> bool {
        self.inner.playback().multi_channel
    }

    /// Interleaved samples per output frame (2 or 16).
    pub fn out_channels(&self) -> usize {
        self.inner.playback().out_channels() as usize
    }

    /// Start a zero-based track in the current playlist.
    pub fn start_track(&mut self, track: i32) -> Result<()> {
        let mut mapped = track;
        self.inner
            .file()
            .remap_track(&mut mapped, self.format.info().flags)?;
        if self.sample_rate == 0 {
            return Err(GmeError::Internal("Metadata-only player"));
        }
        if let Some(entry) = self.collection.get(mapped as usize) {
            let mut next = self.prepare_decoder(&entry.data, self.sample_rate)?;
            next.start_track_impl(0)?;
            next.file_mut()
                .set_track_count(self.collection.len() as i32);
            next.file_mut().track_count = self.track_count();
            std::mem::swap(
                &mut self.inner.file_mut().playlist,
                &mut next.file_mut().playlist,
            );
            self.inner = next;
        } else {
            self.inner.file_mut().warning();
            self.inner.start_track_impl(mapped)?;
        }
        self.current_track = Some(track);
        Ok(())
    }

    /// Generate interleaved i16 samples. The buffer must contain whole output frames.
    pub fn play(&mut self, out: &mut [i16]) -> Result<()> {
        if self.current_track.is_none() {
            return Err(GmeError::Internal("No track started"));
        }
        if !out.len().is_multiple_of(self.out_channels()) || out.len() > i32::MAX as usize {
            return Err(GmeError::Internal("Invalid output buffer length"));
        }
        self.inner.play_impl(out.len() as i32, out)
    }

    pub fn track_count(&self) -> i32 {
        self.inner.file().track_count
    }

    pub fn track_info(&self, track: i32) -> Result<TrackInfo> {
        let mut info = TrackInfo::default();
        self.inner
            .file()
            .track_info_with_m3u(&mut info, track, self.format, |out, mapped| {
                if let Some(entry) = self.collection.get(mapped as usize) {
                    out.clone_from(&entry.info);
                    Ok(())
                } else {
                    self.inner.track_info_impl(out, mapped)
                }
            })?;
        info.track_count = i64::from(self.track_count());
        Ok(info)
    }

    pub fn load_m3u(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.load_m3u_data(std::fs::read(path).map_err(|_| GmeError::OpenFailed)?)
    }
    pub fn load_m3u_data(&mut self, data: impl AsRef<[u8]>) -> Result<()> {
        self.inner.file_mut().load_m3u_data(data.as_ref())
    }
    /// Remove the external playlist and any embedded NSFE playlist.
    pub fn clear_playlist(&mut self) {
        self.inner.clear_playlist_impl();
        self.inner.file_mut().clear_playlist_state();
    }
    pub fn voice_count(&self) -> i32 {
        if self.loaded && self.sample_rate != 0 {
            self.inner.voice_count()
        } else {
            0
        }
    }
    pub fn voice_name(&self, index: u32) -> Option<String> {
        if index >= self.voice_count() as u32 {
            return None;
        }
        self.inner
            .voice_names()
            .get(index as usize)
            .map(|s| (*s).to_owned())
    }
    pub fn voice_names(&self) -> Vec<String> {
        if self.voice_count() == 0 {
            return Vec::new();
        }
        self.inner
            .voice_names()
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    }
    /// Mute (or restore) one voice without changing the other mask bits.
    pub fn mute_voice(&mut self, voice: u32, mute: bool) {
        if let Some(bit) = 1i32.checked_shl(voice) {
            let mask = self.inner.playback().mute_mask;
            self.mute_voices(if mute { mask | bit } else { mask & !bit });
        }
    }
    /// Set all mute bits at once. Zero restores all voices.
    pub fn mute_voices(&mut self, mask: i32) {
        self.inner.playback_mut().mute_mask = mask;
        if self.loaded && self.sample_rate != 0 {
            self.inner.mute_voices_impl(mask);
        }
    }
    pub fn equalizer(&self) -> Equalizer {
        self.inner.playback().equalizer
    }
    pub fn set_equalizer(&mut self, eq: Equalizer) -> Result<()> {
        if !eq.bass.is_finite() || !eq.treble.is_finite() {
            return Err(GmeError::Internal("Invalid equalizer"));
        }
        self.inner.playback_mut().equalizer = eq;
        if self.loaded && self.sample_rate != 0 {
            self.inner.set_equalizer_impl(&eq);
        }
        Ok(())
    }
    /// Adjust stereo enhancement from 0 (off) to 1 (full depth).
    pub fn set_stereo_depth(&mut self, depth: f64) {
        if depth.is_finite() {
            self.stereo_depth = depth.clamp(0.0, 1.0);
            if self.loaded && self.sample_rate != 0 {
                self.inner.set_stereo_depth_impl(self.stereo_depth);
            }
        }
    }
    /// Toggle hardware accuracy options supported by the format (SPC filtering).
    pub fn enable_accuracy(&mut self, enable: bool) {
        self.accuracy = enable;
        if self.loaded && self.sample_rate != 0 {
            self.inner.enable_accuracy_impl(enable);
        }
    }
    pub fn set_tempo(&mut self, tempo: f64) {
        if tempo.is_finite() {
            self.inner.playback_mut().set_tempo_range(tempo);
            let tempo = self.inner.playback().tempo;
            if self.loaded && self.sample_rate != 0 {
                self.inner.set_tempo_impl(tempo);
            }
        }
    }
    /// Schedule a fade in milliseconds. The legacy default length is 8000 ms.
    pub fn set_fade(&mut self, start_ms: i32, length_ms: i32) {
        self.inner.playback_mut().set_fade(start_ms, length_ms);
    }
    /// Disable the SPC echo unit. The setting survives restart and reload.
    pub fn disable_echo(&mut self, disable: bool) {
        self.echo_disabled = disable;
        if self.loaded && self.sample_rate != 0 {
            self.inner.disable_echo_impl(disable);
        }
    }

    /// Apply embedded track duration when starting a track (currently SPC).
    pub fn set_autoload_playback_limit(&mut self, enabled: bool) {
        self.inner.playback_mut().autoload_playback_limit = enabled;
    }
    pub fn autoload_playback_limit(&self) -> bool {
        self.inner.playback().autoload_playback_limit
    }

    pub fn seek(&mut self, msec: i32) -> Result<()> {
        self.seek_samples(self.inner.playback().msec_to_samples(msec))
    }

    /// Absolute position in interleaved samples. The position must be a frame boundary.
    pub fn seek_samples(&mut self, time: i32) -> Result<()> {
        if time < 0 || time % self.inner.playback().out_channels() != 0 {
            return Err(GmeError::Internal("Seek position must be a complete frame"));
        }
        let track = self
            .current_track
            .ok_or(GmeError::Internal("No track started"))?;
        if time < self.inner.playback().out_time {
            self.start_track(track)?;
        }
        let count = time - self.inner.playback().out_time;
        self.inner.skip_impl(count)
    }
    pub fn tell_samples(&self) -> i32 {
        self.inner.playback().tell_samples()
    }

    /// Elapsed musical time, accounting for tempo changes since track start.
    pub fn tell_scaled(&self) -> i64 {
        self.inner.playback().tell_scaled()
    }

    pub fn seek_scaled(&mut self, msec: i32) -> Result<()> {
        let track = self
            .current_track
            .ok_or(GmeError::Internal("No track started"))?;
        let target = i64::from(msec.max(0)) * i64::from(self.sample_rate) / 1000;
        if target < i64::from(self.inner.playback().out_time_scaled) {
            self.start_track(track)?;
        }
        let state = self.inner.playback();
        let channels = state.out_channels();
        let frames = target - i64::from(state.out_time_scaled);
        let samples = (frames as f64 * f64::from(channels) / state.tempo) as i64;
        let aligned = samples.saturating_add(i64::from(channels) - 1) / i64::from(channels)
            * i64::from(channels);
        let position = i64::from(state.out_time).saturating_add(aligned);
        self.seek_samples(
            i32::try_from(position).map_err(|_| GmeError::Internal("Seek out of range"))?,
        )
    }
    pub fn tell(&self) -> i32 {
        self.inner.playback().tell()
    }
    pub fn track_ended(&self) -> bool {
        self.inner.playback().track_ended
    }
    pub fn set_ignore_silence(&mut self, ignore: bool) {
        self.inner.playback_mut().ignore_silence_flag = ignore;
    }
    pub fn ignore_silence(&mut self, ignore: bool) {
        self.set_ignore_silence(ignore);
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render ~2s through the public API and return the RMS. Exercises the
    /// exact construction sequence users hit (auto-detect, box, load order).
    #[cfg(feature = "nsf")]
    fn public_api_rms(path: &str, secs: f64) -> f64 {
        let data = std::fs::read(format!("../../{}", path))
            .or_else(|_| std::fs::read(path))
            .unwrap_or_else(|_| panic!("{} not found", path));
        let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        emu.start_track(0).unwrap();
        let mut samples = vec![0i16; (44100.0 * secs) as usize * 2];
        emu.play(&mut samples).unwrap();
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    /// Regression test: the public API used to initialize audio before
    /// loading, so header-dependent setup (expansion chips) was skipped —
    /// the FDS wave channel was silent ("missing instrument"). A healthy
    /// render of this file has RMS well above 1000; without the FDS chip it
    /// drops by more than half.
    #[cfg(feature = "nsf")]
    #[test]
    fn test_fds_expansion_audible_via_public_api() {
        let rms = public_api_rms("assets/expansion/fds.nsf", 2.0);
        assert!(
            rms > 1000.0,
            "FDS render RMS {:.1} too low — expansion chip missing?",
            rms
        );
    }

    /// Regression test: NSF skip_impl used to pump clocks into the blip
    /// buffer without reading samples out, so seeking far ahead overflowed
    /// the buffer and panicked on a slice index. It also never advanced
    /// out_time, so tell() stayed at 0 after a seek.
    #[cfg(feature = "nsf")]
    #[test]
    fn test_nsf_seek_then_play_via_public_api() {
        let data = std::fs::read("../../assets/test.nsf")
            .or_else(|_| std::fs::read("assets/test.nsf"))
            .expect("test.nsf not found");
        let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        emu.start_track(0).unwrap();
        emu.seek(35_000).unwrap();
        assert_eq!(emu.tell(), 35_000, "tell() should reflect the seek");
        let mut samples = vec![0i16; 44100 * 2];
        emu.play(&mut samples).unwrap();
        assert!(
            samples.iter().any(|&s| s != 0),
            "no audio after seeking to 35s"
        );
    }

    /// Regression test: DPCM sample fetches used to go through pointers
    /// captured before load / before the emulator was boxed, silencing or
    /// corrupting the DMC channel through the public API.
    #[cfg(feature = "nsf")]
    #[test]
    fn test_nsf_voices_match_header_via_public_api() {
        let data = std::fs::read("../../assets/expansion/fds.nsf")
            .or_else(|_| std::fs::read("assets/expansion/fds.nsf"))
            .expect("fds.nsf not found");
        let emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        assert_eq!(
            emu.voice_count(),
            6,
            "FDS NSF should expose 5 APU voices + Wave"
        );
        assert_eq!(emu.voice_names().last().map(String::as_str), Some("Wave"));
    }

    /// VGZ (gzipped VGM) must decompress transparently and render exactly
    /// like the same bytes pre-decompressed.
    #[cfg(feature = "vgm")]
    #[test]
    fn test_vgz_decompresses_to_vgm() {
        use std::io::Read;
        let gz = std::fs::read("../../assets/test.vgz")
            .or_else(|_| std::fs::read("assets/test.vgz"))
            .expect("test.vgz not found");
        let mut vgm = Vec::new();
        flate2::read::GzDecoder::new(&gz[..])
            .read_to_end(&mut vgm)
            .unwrap();
        assert_eq!(&vgm[..4], b"Vgm ");

        let render = |data: &[u8]| -> Vec<i16> {
            let mut emu = GameMusicEmu::from_data(data, 44100).unwrap();
            emu.start_track(0).unwrap();
            let mut samples = vec![0i16; 44100 * 2];
            emu.play(&mut samples).unwrap();
            samples
        };
        assert_eq!(render(&gz), render(&vgm));
    }

    /// Generic load-and-play smoke check through the public API.
    #[allow(dead_code)]
    fn smoke_load_and_play(asset: &str) {
        let data = std::fs::read(format!("../../{}", asset))
            .or_else(|_| std::fs::read(asset))
            .unwrap_or_else(|_| panic!("{} not found", asset));
        let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        assert!(emu.track_count() > 0, "{}: no tracks", asset);
        emu.start_track(0).unwrap();
        let mut samples = vec![0i16; 44100 * 2]; // 1 second stereo
        emu.play(&mut samples).unwrap();
        assert!(
            samples.iter().any(|&s| s != 0),
            "{}: produced only silence",
            asset
        );
    }

    #[cfg(feature = "nsf")]
    #[test]
    fn test_nsfe_load_and_play() {
        smoke_load_and_play("assets/test.nsfe");
    }

    #[cfg(feature = "spc")]
    #[test]
    fn test_spc_load_and_play() {
        smoke_load_and_play("assets/test.spc");
    }

    #[cfg(feature = "gbs")]
    #[test]
    fn test_gbs_load_and_play() {
        smoke_load_and_play("assets/test.gbs");
    }

    #[cfg(feature = "vgm")]
    #[test]
    fn test_vgm_load_and_play() {
        smoke_load_and_play("assets/test.vgm");
    }

    #[cfg(feature = "kss")]
    #[test]
    fn test_kss_load_and_play() {
        smoke_load_and_play("assets/test.kss");
    }

    #[cfg(feature = "ay")]
    #[test]
    fn test_ay_load_and_play() {
        smoke_load_and_play("assets/test.ay");
    }

    #[cfg(feature = "gym")]
    #[test]
    fn test_gym_load_and_play() {
        smoke_load_and_play("assets/test.gym");
    }

    /// test.hes track 0 is genuinely silent (matches C++), so smoke track 1.
    #[cfg(feature = "hes")]
    #[test]
    fn test_hes_load_and_play() {
        let data = std::fs::read("../../assets/test.hes")
            .or_else(|_| std::fs::read("assets/test.hes"))
            .expect("test.hes not found");
        let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        assert!(emu.track_count() > 1, "HES should expose 256 tracks");
        emu.start_track(1).unwrap();
        let mut samples = vec![0i16; 44100 * 2];
        emu.play(&mut samples).unwrap();
        assert!(
            samples.iter().any(|&s| s != 0),
            "test.hes track 1 should produce audio"
        );
    }

    #[cfg(feature = "sap")]
    #[test]
    fn test_sap_load_and_play() {
        let data = std::fs::read("../../assets/test.sap")
            .or_else(|_| std::fs::read("assets/test.sap"))
            .expect("test.sap not found");
        let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        assert!(emu.track_count() > 0, "Should have tracks");
        emu.start_track(0).unwrap();
        let mut samples = vec![0i16; 4410 * 2]; // 0.1 seconds stereo
        emu.play(&mut samples).unwrap();
        assert_eq!(samples.len(), 4410 * 2);
    }

    #[cfg(feature = "nsf")]
    #[test]
    fn test_nsf_default_audio_quality() {
        let data = std::fs::read("../../assets/test.nsf")
            .or_else(|_| std::fs::read("assets/test.nsf"))
            .expect("test.nsf not found");
        let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        emu.start_track(0).unwrap();

        let mut all_samples = Vec::new();
        for _ in 0..10 {
            let mut buf = vec![0i16; 4410 * 2];
            emu.play(&mut buf).unwrap();
            all_samples.extend_from_slice(&buf);
        }

        let mono: Vec<i16> = all_samples.chunks(2).map(|c| c[0]).collect();
        let nonzero = mono.iter().filter(|&&s| s != 0).count();
        let max_abs = mono.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        let rms: f64 = {
            let sum: f64 = mono.iter().map(|&s| (s as f64).powi(2)).sum();
            (sum / mono.len() as f64).sqrt()
        };

        eprintln!(
            "Default NSF - Samples: {}, Non-zero: {}, Max: {}, RMS: {:.1}",
            mono.len(),
            nonzero,
            max_abs,
            rms
        );
    }

    /// Replicates the record_range flow: start_track, then play in a loop
    /// checking track_ended/tell, verifying non-zero output is produced.
    /// Same test but with the default nsf feature
    #[cfg(feature = "nsf")]
    #[test]
    fn test_nsf_recording_flow() {
        let data = std::fs::read("../../assets/test.nsf")
            .or_else(|_| std::fs::read("assets/test.nsf"))
            .expect("test.nsf not found");
        let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
        emu.start_track(0).unwrap();

        assert!(
            !emu.track_ended(),
            "track_ended should be false right after start_track"
        );

        let mut emu_buffer = [0i16; 4096];
        let mut total_nonzero = 0usize;
        let mut iterations = 0usize;

        while !emu.track_ended() && emu.tell() < 1000 {
            emu.play(&mut emu_buffer).unwrap();
            total_nonzero += emu_buffer.iter().filter(|&&s| s != 0).count();
            iterations += 1;
            if iterations >= 200 {
                break;
            }
        }

        assert!(iterations > 0, "Should have played at least one buffer");
        assert!(
            total_nonzero > 0,
            "Recording flow should produce non-zero samples, got {} iterations with {} non-zero samples",
            iterations,
            total_nonzero
        );
    }

    /// The tone driver uses the incoming track index even above the declared count.
    #[cfg(feature = "nsf")]
    #[test]
    fn test_nsf_high_tracks() {
        let data = include_bytes!("../../../assets/test.nsf");
        for track in [4, 10, 16, 22, 25, 28, 31, 82] {
            let mut emu = GameMusicEmu::from_data(data.as_slice(), 44100).unwrap();
            assert!(track < emu.track_count());
            emu.start_track(track).unwrap();
            assert!(
                !emu.track_ended(),
                "track {} ended immediately after start",
                track
            );

            let mut total_nonzero = 0usize;
            let mut buf = [0i16; 4096];
            for _ in 0..50 {
                if emu.track_ended() {
                    break;
                }
                emu.play(&mut buf).unwrap();
                total_nonzero += buf.iter().filter(|&&s| s != 0).count();
            }
            eprintln!("  track {}: {} non-zero samples", track, total_nonzero);
            assert!(total_nonzero > 0, "track {} produced no audio", track);
        }
    }
}
