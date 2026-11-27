use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipTime};
use gme_core::error::GmeResult;
use gme_core::multi_buffer::{MultiBuffer, StereoBuffer};

use crate::music_emu::MusicEmulator;
use crate::track_info::Equalizer;

/// Trait for emulators that use Blip_Buffer for sound synthesis.
/// Replaces C++ `Classic_Emu` class.
pub trait ClassicEmulator: MusicEmulator {
    /// Wire a voice to buffer outputs. Pass null pointers to mute.
    fn set_voice(
        &mut self,
        index: i32,
        center: *mut BlipBuffer,
        left: *mut BlipBuffer,
        right: *mut BlipBuffer,
    );

    /// Update equalization on sound chips.
    fn update_eq(&mut self, eq: BlipEq);

    /// Run emulation for given clock cycles. Updates time_io.
    fn run_clocks(&mut self, time_io: &mut BlipTime, msec: i32) -> GmeResult;

    /// Get voice types for channel mapping.
    fn voice_types(&self) -> &[i32] {
        &[]
    }
}

/// Shared state for classic emulators.
pub struct ClassicEmuState {
    pub buf: Option<*mut dyn MultiBuffer>,
    pub stereo_buffer: Option<Box<StereoBuffer>>,
    pub clock_rate: u32,
    pub buf_changed_count: u32,
}

impl Default for ClassicEmuState {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassicEmuState {
    pub fn new() -> Self {
        Self {
            buf: None,
            stereo_buffer: None,
            clock_rate: 0,
            buf_changed_count: 0,
        }
    }

    /// Initialize the buffer. Creates a default Stereo_Buffer if none set.
    pub fn init_buffer(&mut self, sample_rate: i32) -> GmeResult {
        if self.buf.is_none() {
            if self.stereo_buffer.is_none() {
                self.stereo_buffer = Some(Box::new(StereoBuffer::new()));
            }
            self.buf = Some(self.stereo_buffer.as_mut().unwrap().as_mut() as *mut dyn MultiBuffer);
        }
        if let Some(buf) = self.buf {
            unsafe {
                (*buf).set_sample_rate(sample_rate, 1000 / 20)?;
            }
        }
        Ok(())
    }

    /// Set up the buffer with a clock rate.
    pub fn setup_buffer(&mut self, rate: u32, voice_count: i32, eq: &Equalizer) -> GmeResult {
        self.clock_rate = rate;
        if let Some(buf) = self.buf {
            unsafe {
                (*buf).set_channel_count(voice_count)?;
                (*buf).clock_rate(rate);
                (*buf).bass_freq(eq.bass as i32);
                self.buf_changed_count = (*buf).channels_changed_count();
            }
        }
        Ok(())
    }

    /// Start track - clears buffer.
    pub fn start_track(&mut self) {
        if let Some(buf) = self.buf {
            unsafe {
                (*buf).clear();
            }
        }
    }

    /// Play samples through the buffer.
    pub fn play_classic<E: ClassicEmulator + ?Sized>(
        &mut self,
        emu: &mut E,
        count: i32,
        out: &mut [i16],
    ) -> GmeResult {
        let buf = self
            .buf
            .ok_or(gme_core::error::GmeError::Internal("No buffer"))?;
        let mut remain = count as i64;
        let mut pos = 0;

        while remain > 0 {
            unsafe {
                let read = (*buf).read_samples(&mut out[pos..], remain);
                remain -= read;
                pos += read as usize;

                if remain > 0 {
                    // Check for channel changes
                    let new_count = (*buf).channels_changed_count();
                    if self.buf_changed_count != new_count {
                        self.buf_changed_count = new_count;
                        // Would need to remute voices here
                    }
                    let msec = (*buf).length();
                    let mut clocks_emulated =
                        (msec as i64 * self.clock_rate as i64 / 1000) as BlipTime;
                    emu.run_clocks(&mut clocks_emulated, msec)?;
                    (*buf).end_frame(clocks_emulated);
                }
            }
        }
        Ok(())
    }

    /// Mute voices by mask.
    pub fn mute_voices<E: ClassicEmulator + ?Sized>(&mut self, emu: &mut E, mask: i32) {
        let voice_count = emu.voice_count();
        let voice_types: Vec<i32> = emu.voice_types().to_vec();
        for i in (0..voice_count).rev() {
            if mask & (1 << i) != 0 {
                emu.set_voice(
                    i,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
            } else if let Some(buf) = self.buf {
                let vt = if (i as usize) < voice_types.len() {
                    voice_types[i as usize]
                } else {
                    0
                };
                unsafe {
                    let ch = (*buf).channel(i, vt);
                    emu.set_voice(i, ch.center, ch.left, ch.right);
                }
            }
        }
    }

    /// Set equalizer on the buffer.
    pub fn set_equalizer<E: ClassicEmulator + ?Sized>(&mut self, emu: &mut E, eq: &Equalizer) {
        let blip_eq = BlipEq::new(eq.treble);
        emu.update_eq(blip_eq);
        if let Some(buf) = self.buf {
            unsafe {
                (*buf).bass_freq(eq.bass as i32);
            }
        }
    }
}

/// ROM data handler with padding for bank mapping.
pub struct RomData {
    pub rom: Vec<u8>,
    pub file_size: i64,
    pub rom_addr: i32,
    pub mask: i32,
    pub size: i32,
}

impl Default for RomData {
    fn default() -> Self {
        Self::new()
    }
}

impl RomData {
    const PAD_EXTRA: usize = 8;

    pub fn new() -> Self {
        Self {
            rom: Vec::new(),
            file_size: 0,
            rom_addr: 0,
            mask: 0,
            size: 0,
        }
    }

    /// Load ROM data with header extraction.
    pub fn load(
        &mut self,
        data: &[u8],
        header_size: usize,
        header_out: &mut [u8],
        fill: u8,
        pad_size: usize,
    ) -> GmeResult {
        self.rom_addr = 0;
        self.mask = 0;
        self.size = 0;
        self.rom.clear();

        self.file_size = data.len() as i64;
        if self.file_size <= header_size as i64 {
            return Err(gme_core::error::GmeError::WrongFileType);
        }

        let file_offset = pad_size - header_size;
        let total_size = file_offset + data.len() + pad_size;
        self.rom.resize(total_size, fill);

        // Copy file data
        self.rom[file_offset..file_offset + data.len()].copy_from_slice(data);

        // Copy header
        header_out[..header_size]
            .copy_from_slice(&self.rom[file_offset..file_offset + header_size]);

        self.file_size -= header_size as i64;

        // Fill padding
        self.rom[..pad_size].fill(fill);
        self.rom[total_size - pad_size..total_size].fill(fill);

        Ok(())
    }

    /// Set address that file data should start at.
    pub fn set_addr(&mut self, addr: i64, unit: i32) {
        self.rom_addr = (addr as i32) - unit - Self::PAD_EXTRA as i32;

        let rounded = (addr + self.file_size + unit as i64 - 1) / unit as i64 * unit as i64;
        if rounded <= 0 {
            self.mask = 0;
            self.size = 0;
        } else {
            let max_addr = (rounded - 1) as u32;
            let shift = 32 - max_addr.leading_zeros();
            self.mask = (1i32 << shift) - 1;
            self.size = rounded as i32;
        }

        let new_size = (self.size as i64 - self.rom_addr as i64 + Self::PAD_EXTRA as i64) as usize;
        if new_size > self.rom.len() {
            self.rom.resize(new_size, 0);
        }
    }

    /// Pointer to beginning of file data.
    pub fn begin(&self) -> &[u8] {
        let pad = Self::PAD_EXTRA + Self::PAD_EXTRA; // unit-dependent, simplified
        if pad < self.rom.len() {
            &self.rom[pad..]
        } else {
            &[]
        }
    }

    /// Mask an address to nearest power of two.
    pub fn mask_addr(&self, addr: i32) -> i32 {
        addr & self.mask
    }

    /// Get data at address.
    pub fn at_addr(&self, addr: i32) -> &[u8] {
        let offset = (self.mask_addr(addr) - self.rom_addr) as usize;
        if offset < self.rom.len() {
            &self.rom[offset..]
        } else {
            &self.rom[..1] // unmapped page
        }
    }
}
