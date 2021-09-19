use crate::native::core::{MusicEmu, ClassicEmu, EmuTypeInfo, Equalizer};
use crate::native::{DynamicResult, NativeMusicEmuBase, Int};
use std::io::Read;
use serde::{Deserialize, Serialize};
use crate::native::nes::nes_apu::NesApu;
use once_cell::unsync::Lazy;

/// Header for an NSF file
#[derive(Default, Debug, Serialize, Deserialize)]
struct Header {
    tag: [char; 5],
    vers: u8,
    track_count: u8,
    first_track: u8,
    load_addr: [u8; 2],
    init_addr: [u8; 2],
    play_addr: [u8; 2],
    game: [char; 32],
    author: [char; 32],
    copyright: [char; 32],
    ntsc_speed: [u8; 2],
    banks: [u8; 8],
    pal_speed: [u8; 2],
    speed_flags: u8,
    chip_flags: u8,
    unused: [u8; 4],
}

impl Header {
    const HEADER_SIZE: usize = 128;
    const TAG: [u8; 5] = [78, 69, 83, 77, 26];

    /// Returns true if it's an NSF header
    pub fn is_nsf(bytes: &[u8]) -> bool {
        bytes.len() >= Self::TAG.len() && bytes[..Self::TAG.len()] == Self::TAG
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> bincode::Result<Self> {
        bincode::deserialize(&bytes)
    }
}

/// Nsf Music Emulator
pub struct NsfEmu {
    music_emu: MusicEmu,
    classic_emu: ClassicEmu,
    apu: NesApu,
    header: Header,
}

impl NsfEmu {
    const BANK_COUNT: usize = 8;
    const EMU_TYPE_INFO: EmuTypeInfo = EmuTypeInfo {
        system_name: "Nintendo NES",
        fixed_track_count: None,
        extension: "NSF",
        flags: 1
    };
    const NES_EQ: Equalizer = Equalizer::new(-1.0, 80.0);
    const FAMICOM_EQ: Equalizer = Equalizer::new(-15.0, 80.0);


    pub fn new(sample_rate: Int) -> Self {
        let mut instance = Self {
            music_emu: MusicEmu::new(sample_rate),
            classic_emu: Default::default(),
            apu: Default::default(),
            header: Default::default()
        };
        instance.music_emu.file.type_info = NsfEmu::EMU_TYPE_INFO;
        instance.music_emu.silence_lookahead = 6;
        // apu.dmc_reader( pcm_read, this );
        instance.music_emu.equalizer = NsfEmu::NES_EQ;
        instance.music_emu.gain = 1.4;
        unimplemented!();
        instance
    }

    pub fn unload(&mut self) {
        self.music_emu.voice_count = 0;
        self.music_emu.clear_track_vars();
        self.music_emu.file.unload();
    }
}

impl NativeMusicEmuBase for NsfEmu {
    fn load_data(&mut self, data: impl AsRef<[u8]>) -> DynamicResult<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_header() {
        let bytes = std::fs::read("assets/test.nsf").unwrap();
        assert!(Header::is_nsf(&bytes));
        let header = Header::from_bytes(&bytes).unwrap();
        println!("header: {:?}", header);
    }
}