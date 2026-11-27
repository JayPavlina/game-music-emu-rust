/// GBS format emulator.
/// Matches C++ Gbs_Emu.h/Gbs_Emu.cpp/gb_cpu_io.h.
use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator, count_silence};
use gme_common::track_info::Equalizer;
use gme_common::track_info::TrackInfo;
use gme_core::blip_buffer::{BlipEq, BlipTime};
use gme_core::effects_buffer::EffectsBuffer;
use gme_core::error::{GmeError, GmeResult};
use gme_core::multi_buffer::{CHANNEL_MIXED_TYPE, CHANNEL_WAVE_TYPE, MultiBuffer};

use crate::gb_apu::{GbApu, REGISTER_COUNT, SOUND_DATA, START_ADDR};
use crate::gb_cpu::{GbAddr, GbCpu};
use crate::header::{GBS_HEADER_SIZE, GbsHeader};
use crate::rom::RomData;

const BANK_SIZE: usize = 0x4000;
const RAM_ADDR: u16 = 0xA000;
const HI_PAGE: u16 = 0xFF00 - RAM_ADDR;
const JOYPAD_ADDR: u16 = 0xFF00;
/// Game Boy master clock rate.
const CLOCK_RATE: i32 = 4194304;
/// C++ Music_Emu buf_size (internal silence-detection buffer).
const SILENCE_BUF_SIZE: i64 = 2048;
/// C++ Music_Emu silence_max (seconds).
const SILENCE_MAX_SECS: i64 = 6;
/// Voice types from the C++ Gbs_Emu constructor (set_voice_types).
const VOICE_TYPES: [i32; 4] = [
    CHANNEL_WAVE_TYPE | 1,
    CHANNEL_WAVE_TYPE | 2,
    CHANNEL_WAVE_TYPE,  // | 0
    CHANNEL_MIXED_TYPE, // | 0
];

/// GBS emulator.
pub struct GbsEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    pub cpu: GbCpu,
    pub apu: GbApu,
    pub rom: RomData,
    pub header: GbsHeader,
    /// RAM: 0x4000 (bank-switched area) + 0x2000 (high page) + padding.
    pub ram: [u8; 0x4000 + 0x2000 + GbCpu::CPU_PADDING],
    /// Output buffer. The C++ reference path (gme_new_emu with the GBS
    /// type's stereo flag set) routes audio through Effects_Buffer, NOT
    /// Stereo_Buffer, so the port must too.
    pub stereo_buf: EffectsBuffer,
    pub buf_initialized: bool,
    /// Current CPU time within the frame.
    cpu_time: BlipTime,
    /// Period between play routine calls (in CPU clocks).
    play_period: BlipTime,
    /// Time of next play routine call.
    next_play: BlipTime,
}

// SAFETY: GbsEmu's raw pointers (via code_map, synth pointers) all point to data
// owned by the struct itself. No shared references or thread-unsafe interior mutability.
unsafe impl Send for GbsEmu {}

impl Default for GbsEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl GbsEmu {
    pub fn new() -> Self {
        let mut emu = Self {
            state: GmeFileState::new(),
            emu_state: MusicEmuState::new(),
            cpu: GbCpu::new(),
            apu: GbApu::new(),
            rom: RomData::new(),
            header: GbsHeader::default(),
            ram: [0; 0x4000 + 0x2000 + GbCpu::CPU_PADDING],
            stereo_buf: EffectsBuffer::new(1, false),
            buf_initialized: false,
            cpu_time: 0,
            play_period: 0,
            next_play: 0,
        };
        // C++ Gbs_Emu constructor settings
        emu.emu_state.silence_lookahead = 6;
        emu.emu_state.max_initial_silence = 21;
        emu.emu_state.gain = 1.2;
        // APU oscillator regs pointers are stale after the struct is placed
        // at its final address; reinitialize them now.
        emu.apu.reinit_osc_pointers();
        emu
    }

    pub fn play_period_value(&self) -> BlipTime {
        self.play_period
    }

    pub fn next_play_value(&self) -> BlipTime {
        self.next_play
    }

    /// Initialize audio buffer and wire oscillators.
    fn init_audio(&mut self, sample_rate: i32) -> GmeResult {
        self.stereo_buf.set_sample_rate(sample_rate, 1000 / 20)?;
        self.stereo_buf.clock_rate(4194304); // Game Boy clock rate

        // Wire oscillators to their channels (C++ Classic_Emu::mute_voices_
        // via remute_voices, with the stored mute mask)
        self.wire_voices(self.emu_state.mute_mask);

        // Apply volume matching C++ Gbs_Emu constructor: gain = 1.2
        self.apu.volume(1.2);

        // Apply default equalizer: treble = -1.0, bass = 120
        self.set_equalizer_impl(&Equalizer::new(-1.0, 120.0));

        self.buf_initialized = true;
        Ok(())
    }

    /// Set the ROM bank for $4000-$7FFF.
    fn set_bank(&mut self, n: i32) {
        let mut n = n & 0x1F;
        if n == 0 {
            n = 1;
        }
        let addr = n * BANK_SIZE as i32;
        if addr > self.rom.size() {
            return;
        }
        let masked = self.rom.mask_addr(addr);
        let ptr = self.rom.at_addr(masked);
        unsafe { self.cpu.map_code(BANK_SIZE as u16, BANK_SIZE as u16, ptr) };
    }

    /// Update the play timer based on timer_mode/timer_modulo.
    fn update_timer(&mut self) {
        if self.header.timer_mode & 0x04 != 0 {
            let rates: [i32; 4] = [10, 4, 6, 8];
            let shift = rates[(self.ram[HI_PAGE as usize + 7] & 3) as usize]
                - (self.header.timer_mode as i32 >> 7);
            self.play_period = (256i32 - self.ram[HI_PAGE as usize + 6] as i32) << shift;
        } else {
            self.play_period = 70224; // 59.73 Hz
        }
        if self.emu_state.tempo != 1.0 {
            self.play_period = (self.play_period as f64 / self.emu_state.tempo) as i32;
        }
    }

    /// Wire oscillator outputs per the mute mask.
    /// Transliteration of C++ Classic_Emu::mute_voices_ + Gbs_Emu::set_voice.
    fn wire_voices(&mut self, mask: i32) {
        for i in (0..4usize).rev() {
            if mask & (1 << i) != 0 {
                self.apu.osc_output(i, None, None, None);
            } else {
                let ch = self.stereo_buf.channel(i as i32, VOICE_TYPES[i]);
                self.apu
                    .osc_output(i, Some(ch.center), Some(ch.left), Some(ch.right));
            }
        }
    }

    /// Current emulated time (C++ Gbs_Emu::clock()).
    fn clock(&self) -> BlipTime {
        self.cpu_time - self.cpu.remain()
    }

    /// Write a byte to the CPU address space.
    /// Matches C++ Gbs_Emu::cpu_write (gb_cpu_io.h) exactly, including the
    /// I/O-range rewrites and bank switching.
    fn cpu_write(&mut self, addr: u32, data: u8) {
        let offset = addr.wrapping_sub(RAM_ADDR as u32);
        if offset <= 0xFFFF - RAM_ADDR as u32 {
            self.ram[offset as usize] = data;
            // I/O range ($E000-$FF7F)
            if (addr ^ 0xE000) < 0x1F80 {
                if addr.wrapping_sub(START_ADDR as u32) < REGISTER_COUNT as u32 {
                    let time = self.clock();
                    self.apu.write_register(time, addr as u16, data);
                } else if (addr ^ 0xFF06) < 2 {
                    self.update_timer();
                } else if addr == JOYPAD_ADDR as u32 {
                    self.ram[offset as usize] = 0; // keep joypad return value 0
                } else {
                    self.ram[offset as usize] = 0xFF;
                }
            }
        } else if (addr ^ 0x2000) < 0x2000 {
            self.set_bank(data as i32);
        }
    }

    /// Push return address onto stack and set PC (JSR).
    /// Matches C++ Gbs_Emu::cpu_jsr: sets pc, then pushes idle_addr (0xF00D)
    /// via cpu_write so the routine's RET lands at 0xF00D, which contains the
    /// 0xFF "illegal" opcode the CPU recognizes as a stop signal
    /// (pc becomes idle_addr + 1 = 0xF00E after the fetch).
    fn cpu_jsr(&mut self, addr: GbAddr) {
        self.cpu.r.pc = addr as u32;
        self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
        self.cpu_write(self.cpu.r.sp as u32, (GbCpu::IDLE_ADDR >> 8) as u8);
        self.cpu.r.sp = self.cpu.r.sp.wrapping_sub(1);
        self.cpu_write(self.cpu.r.sp as u32, (GbCpu::IDLE_ADDR & 0xFF) as u8);
    }
}

impl GmeFile for GbsEmu {
    fn format(&self) -> Format {
        Format::Gbs
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.set_track_count(n);
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        let header_bytes = self
            .rom
            .load(data, GBS_HEADER_SIZE)
            .map_err(|_| GmeError::WrongFileType)?;

        // Parse header
        let header = GbsHeader::parse(&header_bytes).ok_or(GmeError::WrongFileType)?;
        self.header = header;

        // Set track count from header
        self.state.set_track_count(self.header.track_count as i32);

        // Validate
        if &data[..3] != b"GBS" {
            return Err(GmeError::WrongFileType);
        }

        if self.header.vers != 1 {
            // Warning: unknown version
        }

        if self.header.timer_mode & 0x78 != 0 {
            // Warning: invalid timer mode
        }

        let load_addr = self.header.load_addr;
        if (self.header.load_addr >> 8) > 0x7F
            || (self.header.init_addr >> 8) > 0x7F
            || (self.header.play_addr >> 8) > 0x7F
            || load_addr < 0x400
        {
            // Warning: invalid addresses
        }

        self.emu_state.voice_count = 4; // Square 1, Square 2, Wave, Noise

        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, _track: i32) -> GmeResult {
        info.system = "Game Boy".to_string();
        info.game = self.header.game_name_str();
        info.author = self.header.author_str();
        info.copyright = self.header.copyright_str();
        info.track_count = self.header.track_count as i64;
        Ok(())
    }
}

impl MusicEmulator for GbsEmu {
    fn format(&self) -> Format {
        Format::Gbs
    }

    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        self.emu_state.sample_rate = sample_rate;
        self.init_audio(sample_rate)
    }

    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        // C++ Music_Emu::start_track → clear_track_vars()
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;

        // C++ Classic_Emu::start_track_ → buf->clear()
        self.stereo_buf.clear();

        // Reinitialize oscillator register/synth pointers; the GbsEmu struct
        // may have been moved since the APU was constructed, leaving the raw
        // pointers inside the oscillators pointing at the old location. Must
        // happen before any APU work below.
        self.apu.reinit_osc_pointers();

        // C++ Gbs_Emu::start_track_
        // memset(ram, 0, 0x4000); memset(ram + 0x4000, 0xFF, 0x1F00);
        // memset(ram + 0x5F00, 0, sizeof ram - 0x5F00);
        self.ram.fill(0);
        for i in 0..0x1F00 {
            self.ram[0x4000 + i] = 0xFF;
        }
        self.ram[HI_PAGE as usize] = 0; // joypad reads back as 0

        // Reset APU and write default sound registers
        self.apu.reset();
        for (i, &data) in SOUND_DATA.iter().enumerate() {
            self.apu.write_register(0, i as u16 + START_ADDR, data);
        }

        // Set up ROM
        let load_addr = self.header.load_addr;
        self.rom.set_addr(load_addr as i32, BANK_SIZE as i32);
        self.cpu.rst_base = load_addr;

        // Reset CPU with unmapped page
        unsafe { self.cpu.reset(self.rom.unmapped()) };

        // Map RAM and ROM
        unsafe {
            self.cpu.map_code(RAM_ADDR, 0x6000, self.ram.as_ptr());
            self.cpu.map_code(0, BANK_SIZE as u16, self.rom.at_addr(0));
        }
        // C++: set_bank( rom.size() > bank_size )
        self.set_bank((self.rom.size() > BANK_SIZE as i32) as i32);

        // Set timer registers
        self.ram[HI_PAGE as usize + 6] = self.header.timer_modulo;
        self.ram[HI_PAGE as usize + 7] = self.header.timer_mode;
        self.update_timer();
        self.next_play = self.play_period;

        // Set up CPU for init call
        self.cpu.r.a = track as u8;
        self.cpu.r.pc = GbCpu::IDLE_ADDR as u32;
        self.cpu.r.sp = self.header.stack_ptr;
        self.cpu_time = 0;
        self.cpu_jsr(self.header.init_addr);

        // Re-wire oscillator outputs (applies stored mute mask, matching the
        // persistent voice wiring in C++)
        self.mute_voices_impl(self.emu_state.mute_mask);

        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;

        // C++ Music_Emu::start_track: play until non-silence or end of track
        if !self.emu_state.ignore_silence_flag {
            let end =
                self.emu_state.max_initial_silence as i64 * 2 * self.emu_state.sample_rate as i64;
            while (self.emu_state.emu_time as i64) < end {
                self.fill_buf()?;
                if self.emu_state.buf_remain > 0 || self.emu_state.emu_track_ended {
                    break;
                }
            }

            // C++: emu_time = buf_remain — initial silence is discarded
            self.emu_state.emu_time = self.emu_state.buf_remain as i32;
            self.emu_state.out_time = 0;
            self.emu_state.out_time_scaled = 0;
            self.emu_state.silence_time = 0;
            self.emu_state.silence_count = 0;
        }

        Ok(())
    }

    /// Transliteration of C++ Music_Emu::play.
    fn play_impl(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let out_count = count as usize;

        if !self.buf_initialized || self.emu_state.track_ended {
            out[..out_count].fill(0);
            self.emu_state.out_time += count;
            self.emu_state.out_time_scaled += (count as f64 * self.emu_state.tempo / 2.0) as i32;
            return Ok(());
        }

        let mut pos = 0usize;

        if self.emu_state.silence_count > 0 {
            // During a run of silence, run emulator at >=2x speed so it gets ahead
            let ahead_time = self.emu_state.silence_lookahead as i64
                * (self.emu_state.out_time as i64 + out_count as i64 - self.emu_state.silence_time)
                + self.emu_state.silence_time;
            while (self.emu_state.emu_time as i64) < ahead_time
                && self.emu_state.buf_remain == 0
                && !self.emu_state.emu_track_ended
            {
                self.fill_buf()?;
            }

            // Fill with silence
            let n = (self.emu_state.silence_count as usize).min(out_count);
            out[..n].fill(0);
            self.emu_state.silence_count -= n as i64;
            pos = n;

            if !self.emu_state.ignore_silence_flag
                && self.emu_state.emu_time as i64 - self.emu_state.silence_time
                    > SILENCE_MAX_SECS * 2 * self.emu_state.sample_rate as i64
            {
                self.emu_state.track_ended = true;
                self.emu_state.emu_track_ended = true;
                self.emu_state.silence_count = 0;
                self.emu_state.buf_remain = 0;
            }
        }

        if self.emu_state.buf_remain > 0 && pos < out_count {
            // Empty silence buf
            let n = (self.emu_state.buf_remain as usize).min(out_count - pos);
            let buf_start = (SILENCE_BUF_SIZE - self.emu_state.buf_remain) as usize;
            out[pos..pos + n]
                .copy_from_slice(&self.emu_state.silence_buf[buf_start..buf_start + n]);
            self.emu_state.buf_remain -= n as i64;
            pos += n;
        }

        // Generate remaining samples normally
        if pos < out_count {
            let remain = (out_count - pos) as i32;
            // C++ emu_play
            self.emu_state.emu_time += remain;
            // C++ Music_Emu::emu_play: once the track has ended, output
            // hard zeros instead of running the emulator
            if !self.emu_state.emu_track_ended {
                self.classic_play(remain, &mut out[pos..])?;
            } else {
                out[pos..pos + remain as usize].fill(0);
            }
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;

            if !self.emu_state.ignore_silence_flag
                || self.emu_state.out_time > self.emu_state.fade_start
            {
                // Check end for a new run of silence
                let silence = count_silence(&mut out[pos..pos + remain as usize]);
                if silence < remain as i64 {
                    self.emu_state.silence_time = self.emu_state.emu_time as i64 - silence;
                }
                if self.emu_state.emu_time as i64 - self.emu_state.silence_time >= SILENCE_BUF_SIZE
                {
                    self.fill_buf()?; // cause silence detection on next play()
                }
            }
        }

        // Handle fading
        if self.emu_state.fade_start >= 0
            && self.emu_state.out_time > self.emu_state.fade_start
            && self
                .emu_state
                .handle_fade(self.emu_state.out_time, &mut out[..out_count])
        {
            self.emu_state.track_ended = true;
            self.emu_state.emu_track_ended = true;
        }

        self.emu_state.out_time += count;
        self.emu_state.out_time_scaled += (count as f64 * self.emu_state.tempo / 2.0) as i32;
        Ok(())
    }

    /// Transliteration of C++ Music_Emu::skip / Music_Emu::skip_.
    fn skip_impl(&mut self, count: i32) -> GmeResult {
        self.emu_state.out_time += count;
        self.emu_state.out_time_scaled += (count as f64 * self.emu_state.tempo / 2.0) as i32;

        let mut count = count;

        // Remove from silence and buf first
        {
            let n = (count as i64).min(self.emu_state.silence_count);
            self.emu_state.silence_count -= n;
            count -= n as i32;

            let n = (count as i64).min(self.emu_state.buf_remain);
            self.emu_state.buf_remain -= n;
            count -= n as i32;
        }

        if count > 0 && !self.emu_state.emu_track_ended {
            self.emu_state.emu_time += count;

            // C++ Music_Emu::skip_: for long skips, mute sound
            let threshold = 30000;
            let mut scratch = [0i16; SILENCE_BUF_SIZE as usize];
            if count > threshold {
                let saved_mute = self.emu_state.mute_mask;
                self.mute_voices_impl(!0);

                while count > threshold / 2 && !self.emu_state.emu_track_ended {
                    self.classic_play(SILENCE_BUF_SIZE as i32, &mut scratch)?;
                    count -= SILENCE_BUF_SIZE as i32;
                }

                self.mute_voices_impl(saved_mute);
            }

            while count > 0 && !self.emu_state.emu_track_ended {
                let n = (count as i64).min(SILENCE_BUF_SIZE) as i32;
                count -= n;
                self.classic_play(n, &mut scratch)?;
            }
        }

        if self.emu_state.silence_count == 0 && self.emu_state.buf_remain == 0 {
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.tempo = tempo;
        self.apu.set_tempo(tempo);
        self.update_timer();
    }

    fn set_equalizer_impl(&mut self, settings: &Equalizer) {
        let eq = BlipEq::new(settings.treble);
        self.apu.treble_eq(&eq);
        self.stereo_buf.bass_freq(settings.bass as i32);
    }

    fn set_stereo_depth_impl(&mut self, depth: f64) {
        self.stereo_buf.set_depth(depth);
        self.mute_voices_impl(self.emu_state.mute_mask);
    }

    fn mute_voices_impl(&mut self, mask: i32) {
        self.emu_state.mute_mask = mask;
        self.wire_voices(mask);
    }

    fn voice_count(&self) -> i32 {
        4
    }

    fn voice_names(&self) -> &[&str] {
        &["Square 1", "Square 2", "Wave", "Noise"]
    }
}

impl GbsEmu {
    /// Transliteration of C++ Classic_Emu::play_: read samples from the
    /// stereo buffer, running the emulator whenever more are needed.
    fn classic_play(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let count = count as usize;
        let mut remain = count;
        while remain > 0 {
            let read = self
                .stereo_buf
                .read_samples(&mut out[count - remain..], remain as i64);
            remain -= read as usize;
            if remain > 0 {
                let msec = self.stereo_buf.length();
                let mut clocks = msec * CLOCK_RATE / 1000;
                self.run_clocks(&mut clocks)?;
                debug_assert!(clocks > 0);
            }
        }
        Ok(())
    }

    /// Transliteration of C++ Music_Emu::fill_buf: fill the internal buffer
    /// and check it for silence.
    fn fill_buf(&mut self) -> GmeResult {
        debug_assert!(self.emu_state.buf_remain == 0);
        if !self.emu_state.emu_track_ended {
            // C++ emu_play: emulator time advances by what was generated
            self.emu_state.emu_time += SILENCE_BUF_SIZE as i32;
            let mut buf = std::mem::take(&mut self.emu_state.silence_buf);
            let result = self.classic_play(SILENCE_BUF_SIZE as i32, &mut buf);
            let silence = count_silence(&mut buf);
            self.emu_state.silence_buf = buf;
            result?;
            if silence < SILENCE_BUF_SIZE {
                self.emu_state.silence_time = self.emu_state.emu_time as i64 - silence;
                self.emu_state.buf_remain = SILENCE_BUF_SIZE;
                return Ok(());
            }
        }
        self.emu_state.silence_count += SILENCE_BUF_SIZE;
        Ok(())
    }

    /// Run emulation for the given duration.
    /// Matches C++ Gbs_Emu::run_clocks:
    /// - `cpu_time` is reset to 0 at the start of each call (relative time within this call).
    /// - `next_play` is a persistent class member, incremented each call.
    /// - The APU's `end_frame` subtracts the elapsed time from its internal cumulative state,
    ///   converting it back to "delta from start of next call".
    /// - The blip_buffer's `end_frame` is called separately with the same elapsed time.
    fn run_clocks(&mut self, duration: &mut BlipTime) -> GmeResult {
        // Reset cpu_time to 0 (matches C++ Gbs_Emu::run_clocks).
        self.cpu_time = 0;
        let target = *duration;

        while self.cpu_time < target {
            let count = target - self.cpu_time;
            self.cpu_time = target;

            // Use raw pointers to avoid borrow checker issues with closures
            let cpu_ptr = &mut self.cpu as *mut GbCpu;
            let apu_ptr = &mut self.apu as *mut GbApu;
            let ram_ptr = self.ram.as_mut_ptr();
            let rom_ptr = &self.rom as *const RomData;
            let cpu_time_ptr = &mut self.cpu_time as *mut BlipTime;
            let self_ptr = self as *mut Self;

            let result = unsafe {
                (*cpu_ptr).run(
                    count,
                    &|addr: u32| {
                        // Read callback - matches C++ Gbs_Emu::cpu_read.
                        // 1. Read from code_map (ROM/RAM); addr may be up to
                        //    0x10000 (e.g. READ(sp + 1)), landing in the
                        //    extra unmapped page like C++ get_code.
                        let page = ((addr as usize) >> 13).min(8);
                        let val = *(*cpu_ptr).code_map[page].add(addr as usize & 0x1FFF);
                        // 2. Override with APU for APU register range
                        if addr.wrapping_sub(START_ADDR as u32) < REGISTER_COUNT as u32 {
                            let cur_time = *cpu_time_ptr - (*cpu_ptr).remain();
                            return (*apu_ptr).read_register(cur_time, addr as u16) as u8;
                        }
                        val
                    },
                    &mut |addr: u32, data: u8| {
                        // Write callback - matches C++ Gbs_Emu::cpu_write
                        // Compute current time: cpu_time - remain()
                        let offset = addr.wrapping_sub(RAM_ADDR as u32);
                        if offset <= 0xFFFF - RAM_ADDR as u32 {
                            *ram_ptr.add(offset as usize) = data;
                            // C++ check: (addr ^ 0xE000) < 0x1F80
                            if (addr ^ 0xE000) < 0x1F80 {
                                if addr.wrapping_sub(START_ADDR as u32) < REGISTER_COUNT as u32 {
                                    let cur_time = *cpu_time_ptr - (*cpu_ptr).remain();
                                    (*apu_ptr).write_register(cur_time, addr as u16, data);
                                } else if (addr ^ 0xFF06) < 2 {
                                    // Timer register write — recalculate play_period
                                    // Matches C++ Gbs_Emu::cpu_write which calls update_timer()
                                    (*self_ptr).update_timer();
                                } else if addr == JOYPAD_ADDR as u32 {
                                    *ram_ptr.add(offset as usize) = 0;
                                } else {
                                    *ram_ptr.add(offset as usize) = 0xFF;
                                }
                            }
                        } else if (addr ^ 0x2000) < 0x2000 {
                            // Bank switch (C++ Gbs_Emu::set_bank)
                            let mut n = (data as i32) & 0x1F;
                            if n == 0 {
                                n = 1;
                            }
                            let bank_addr = n * BANK_SIZE as i32;
                            if bank_addr <= (*rom_ptr).size() {
                                let ptr = (*rom_ptr).at_addr((*rom_ptr).mask_addr(bank_addr));
                                (*cpu_ptr).map_code(BANK_SIZE as u16, BANK_SIZE as u16, ptr);
                            }
                        }
                    },
                )
            };

            // Adjust cpu_time for remaining cycles
            self.cpu_time -= self.cpu.remain();

            if result {
                if self.cpu.r.pc as u16 == GbCpu::IDLE_ADDR {
                    // Play routine returned
                    if self.next_play > target {
                        self.cpu_time = target;
                        break;
                    }

                    if self.cpu_time < self.next_play {
                        self.cpu_time = self.next_play;
                    }
                    self.next_play += self.play_period;
                    // JSR to play_addr
                    let play_addr = self.header.play_addr;
                    self.cpu_jsr(play_addr);
                } else if self.cpu.r.pc > 0xFFFF {
                    self.cpu.r.pc &= 0xFFFF;
                } else {
                    // Illegal opcode — skip
                    self.cpu.r.pc = (self.cpu.r.pc + 1) & 0xFFFF;
                    self.cpu_time += 6;
                }
            }
        }

        *duration = self.cpu_time;
        self.next_play -= self.cpu_time;
        if self.next_play < 0 {
            self.next_play = 0;
        }
        self.apu.end_frame(self.cpu_time);
        self.stereo_buf.end_frame(self.cpu_time);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_test_gbs() -> GbsEmu {
        let data = std::fs::read("../../assets/test.gbs")
            .or_else(|_| std::fs::read("assets/test.gbs"))
            .expect("test.gbs not found");
        let mut emu = GbsEmu::new();
        emu.load_mem_impl(&data).expect("Failed to load test.gbs");
        emu
    }

    #[test]
    fn test_load_gbs() {
        let emu = load_test_gbs();
        assert!(emu.state.track_count > 0, "Should have tracks");
        assert!(emu.header.load_addr >= 0x400, "Load addr should be valid");
    }

    #[test]
    fn test_set_sample_rate() {
        let mut emu = load_test_gbs();
        emu.set_sample_rate_impl(44100).unwrap();
        assert_eq!(emu.emu_state.sample_rate, 44100);
        assert!(emu.buf_initialized);
    }

    #[test]
    fn test_start_track() {
        let mut emu = load_test_gbs();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();
        assert!(!emu.emu_state.track_ended);
        assert!(!emu.emu_state.emu_track_ended);
    }

    #[test]
    fn test_play_produces_nonzero_output() {
        let mut emu = load_test_gbs();
        emu.set_sample_rate_impl(44100).unwrap();
        emu.start_track_impl(0).unwrap();

        let count = 8820; // 100ms stereo
        let mut out = vec![0i16; count];
        emu.play_impl(count as i32, &mut out).unwrap();

        let nonzero = out.iter().filter(|&&s| s != 0).count();
        assert!(
            nonzero > 0,
            "Expected non-zero audio output, got {} / {} nonzero",
            nonzero,
            count
        );
    }
}
