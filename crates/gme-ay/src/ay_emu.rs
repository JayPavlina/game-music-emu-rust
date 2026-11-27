//! AY format (ZX Spectrum / Amstrad CPC) music emulator.
//! Port of C++ Ay_Emu.h / Ay_Emu.cpp, using the same Music_Emu/Classic_Emu
//! playback machinery as gme-nsf (proven bit-exact).
use gme_common::format::Format;
use gme_common::gme_file::{GmeFile, GmeFileState};
use gme_common::music_emu::{MusicEmuState, MusicEmulator, count_silence};
use gme_common::track_info::Equalizer;
use gme_common::track_info::TrackInfo;
use gme_core::blip_buffer::{BlipBuffer, BlipEq, BlipTime};
use gme_core::effects_buffer::EffectsBuffer;
use gme_core::error::{GmeError, GmeResult};
use gme_core::multi_buffer::{CHANNEL_MIXED_TYPE, CHANNEL_WAVE_TYPE, MultiBuffer};

use crate::ay_apu::{AMP_RANGE, AyApu, OSC_COUNT as APU_OSC_COUNT};
use crate::ay_cpu::{AyCpu, CPU_PADDING};

const SPECTRUM_CLOCK: u32 = 3_546_900;
const CPC_CLOCK: u32 = 2_000_000;

const RAM_START: usize = 0x4000;
const MEM_SIZE: usize = 0x10000 + CPU_PADDING;
const OSC_COUNT: usize = APU_OSC_COUNT + 1;

const HEADER_SIZE: usize = 0x14;

const SILENCE_BUF_SIZE: i64 = 2048;
const SILENCE_MAX_SECS: i64 = 6;

const VOICE_NAMES: [&str; OSC_COUNT] = ["Wave 1", "Wave 2", "Wave 3", "Beeper"];

/// C++ get_data: resolve a relative big-endian offset field at `ptr` inside
/// the file blob. The offset is signed 16-bit, relative to the field's own
/// position. Returns None for zero offsets or out-of-range targets.
fn get_data(data: &[u8], ptr: usize, min_size: usize) -> Option<usize> {
    let file_size = data.len() as i32;
    debug_assert!(ptr as i32 <= file_size - 2);
    let offset = i16::from_be_bytes([data[ptr], data[ptr + 1]]) as i32;
    let pos = ptr as i32 + offset;
    if offset == 0 || (pos as u32) > (file_size - min_size as i32) as u32 {
        return None;
    }
    Some(pos as usize)
}

fn get_be16(data: &[u8], pos: usize) -> u16 {
    u16::from_be_bytes([data[pos], data[pos + 1]])
}

/// Read a NUL-terminated C string starting at `pos`.
fn c_string(data: &[u8], pos: usize) -> String {
    let end = data[pos..]
        .iter()
        .position(|&b| b == 0)
        .map(|i| pos + i)
        .unwrap_or(data.len());
    String::from_utf8_lossy(&data[pos..end]).into_owned()
}

struct ParsedFile {
    /// Offset of the track list within the file blob.
    tracks: usize,
}

fn parse_header(data: &[u8]) -> Result<ParsedFile, GmeError> {
    if data.len() < HEADER_SIZE {
        return Err(GmeError::WrongFileType);
    }
    if &data[0..8] != b"ZXAYEMUL" {
        return Err(GmeError::WrongFileType);
    }
    let max_track = data[0x10] as usize;
    // h.track_info field is at offset 0x12
    let tracks = get_data(data, 0x12, (max_track + 1) * 4)
        .ok_or(GmeError::Internal("Missing track data"))?;
    Ok(ParsedFile { tracks })
}

/// AY music file emulator.
pub struct AyEmu {
    pub state: GmeFileState,
    pub emu_state: MusicEmuState,
    cpu: AyCpu,
    apu: AyApu,
    /// Entire file blob.
    file_data: Vec<u8>,
    /// Offset of the track list within file_data.
    tracks_offset: usize,
    /// 64K RAM + CPU padding (C++ mem.ram).
    ram: Vec<u8>,
    stereo_buf: EffectsBuffer,
    buf_initialized: bool,
    /// Current clock rate (C++ Classic_Emu::clock_rate_); changes when CPC
    /// hardware is detected.
    clock_rate: u32,
    play_period: BlipTime,
    next_play: BlipTime,
    beeper_delta: i32,
    last_beeper: i32,
    apu_addr: i32,
    cpc_latch: i32,
    spectrum_mode: bool,
    cpc_mode: bool,
    /// Stored mute mask, applied each frame before running the CPU.
    mute_mask: i32,
}

// SAFETY: AyEmu's raw pointers all point to data owned by the struct itself
// and are refreshed at the start of each emulation frame.
unsafe impl Send for AyEmu {}

impl Default for AyEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl AyEmu {
    pub fn new() -> Self {
        let mut emu_state = MusicEmuState::new();
        emu_state.silence_lookahead = 6; // C++ Ay_Emu ctor
        Self {
            state: GmeFileState::new(),
            emu_state,
            cpu: AyCpu::new(),
            apu: AyApu::new(),
            file_data: Vec::new(),
            tracks_offset: 0,
            ram: vec![0u8; MEM_SIZE],
            stereo_buf: EffectsBuffer::new(1, false),
            buf_initialized: false,
            clock_rate: SPECTRUM_CLOCK,
            play_period: 0,
            next_play: 0,
            beeper_delta: 0,
            last_beeper: 0,
            apu_addr: 0,
            cpc_latch: 0,
            spectrum_mode: false,
            cpc_mode: false,
            mute_mask: 0,
        }
    }

    /// Initialize audio buffer (C++ Classic_Emu::set_sample_rate_ +
    /// setup_buffer with the Music_Emu default equalizer).
    fn init_audio(&mut self, sample_rate: i32) -> GmeResult {
        self.stereo_buf
            .set_sample_rate(sample_rate, 1000 / 20)
            .map_err(GmeError::Internal)?;
        self.stereo_buf.clock_rate(self.clock_rate);
        // Music_Emu default equalizer: treble -1.0, bass 60.
        self.set_equalizer_impl(&Equalizer::default());

        self.buf_initialized = true;
        Ok(())
    }

    /// C++ Classic_Emu::change_clock_rate.
    fn change_clock_rate(&mut self, rate: u32) {
        self.clock_rate = rate;
        self.stereo_buf.clock_rate(rate);
    }

    /// C++ Ay_Emu::set_tempo_: play_period = clock_rate() / 50 / t.
    fn update_play_period(&mut self) {
        self.play_period =
            ((self.clock_rate as i64 / 50) as f64 / self.emu_state.tempo) as BlipTime;
    }

    fn copy_ay_fields(&self, out: &mut TrackInfo, track: i32) {
        let data = &self.file_data;
        let track = track as usize;
        if let Some(pos) = get_data(data, self.tracks_offset + track * 4, 1) {
            out.song = c_string(data, pos);
        }
        if let Some(ti) = get_data(data, self.tracks_offset + track * 4 + 2, 6) {
            out.length = get_be16(data, ti + 4) as i64 * (1000 / 50); // frames to msec
        }
        if let Some(pos) = get_data(data, 0x0C, 1) {
            out.author = c_string(data, pos);
        }
        if let Some(pos) = get_data(data, 0x0E, 1) {
            out.comment = c_string(data, pos);
        }
    }

    fn enable_cpc(&mut self) {
        if !self.cpc_mode {
            self.cpc_mode = true;
            self.change_clock_rate(CPC_CLOCK);
            self.update_play_period();
        }
    }

    /// C++ Ay_Emu::cpu_out_misc.
    fn cpu_out_misc(&mut self, time: BlipTime, addr: u32, data: u8) {
        if !self.cpc_mode {
            match addr & 0xFEFF {
                0xFEFD => {
                    self.spectrum_mode = true;
                    self.apu_addr = (data & 0x0F) as i32;
                    return;
                }
                0xBEFD => {
                    self.spectrum_mode = true;
                    self.apu.write(time, self.apu_addr as usize, data);
                    return;
                }
                _ => {}
            }
        }

        if !self.spectrum_mode {
            match addr >> 8 {
                0xF6 => match data & 0xC0 {
                    0xC0 => {
                        self.apu_addr = self.cpc_latch & 0x0F;
                        self.enable_cpc();
                    }
                    0x80 => {
                        self.apu
                            .write(time, self.apu_addr as usize, self.cpc_latch as u8);
                        self.enable_cpc();
                    }
                    _ => {}
                },
                0xF4 => {
                    self.cpc_latch = data as i32;
                    self.enable_cpc();
                }
                _ => {}
            }
        }
        // unmapped OUT
    }

    /// C++ ay_cpu_out.
    fn cpu_out(&mut self, time: BlipTime, addr: u32, data: u8, beeper_output: *mut BlipBuffer) {
        if (addr & 0xFF) == 0xFE && !self.cpc_mode {
            let delta = self.beeper_delta;
            let data = (data & 0x10) as i32;
            if self.last_beeper != data {
                self.last_beeper = data;
                self.beeper_delta = -delta;
                self.spectrum_mode = true;
                if !beeper_output.is_null() {
                    unsafe {
                        self.apu.synth.offset_at(time, delta, &mut *beeper_output);
                    }
                }
            }
        } else {
            self.cpu_out_misc(time, addr, data);
        }
    }

    /// C++ Ay_Emu::run_clocks.
    fn run_clocks(&mut self, duration: &mut BlipTime) -> GmeResult {
        // Refresh raw pointers that go stale if the emulator struct moves.
        let mem_ptr = self.ram.as_mut_ptr();
        self.cpu.set_mem(mem_ptr);
        self.mute_voices_impl(self.mute_mask);
        let beeper_output = if self.mute_mask & (1 << APU_OSC_COUNT) == 0 {
            self.stereo_buf
                .channel(APU_OSC_COUNT as i32, CHANNEL_MIXED_TYPE)
                .center
        } else {
            std::ptr::null_mut()
        };

        self.cpu.set_time(0);
        if !(self.spectrum_mode | self.cpc_mode) {
            *duration /= 2; // until mode is set, leave room for halved clock rate
        }

        while self.cpu.time() < *duration {
            let end = (*duration).min(self.next_play);
            let self_ptr = self as *mut Self;
            // SAFETY: the out/in hooks only touch APU/mode/timing state, never
            // the CPU register/memory state being mutated by run().
            self.cpu.run(
                end,
                &mut |time, addr, data| unsafe {
                    (*self_ptr).cpu_out(time, addr, data, beeper_output);
                },
                &mut |addr| {
                    // C++ ay_cpu_in: keyboard read and other things
                    if (addr & 0xFF) == 0xFE {
                        return 0xFF; // other values break some beeper tunes
                    }
                    0xFF
                },
            );

            if self.cpu.time() >= self.next_play {
                self.next_play += self.play_period;

                if self.cpu.r.iff1 != 0 {
                    let mut pc = self.cpu.r.pc;
                    if self.ram[pc as usize] == 0x76 {
                        pc = pc.wrapping_add(1);
                    }

                    self.cpu.r.iff1 = 0;
                    self.cpu.r.iff2 = 0;

                    let mut sp = self.cpu.r.sp;
                    sp = sp.wrapping_sub(1);
                    self.ram[sp as usize] = (pc >> 8) as u8;
                    sp = sp.wrapping_sub(1);
                    self.ram[sp as usize] = pc as u8;
                    self.cpu.r.sp = sp;
                    self.cpu.r.pc = 0x38;
                    self.cpu.adjust_time(12);
                    if self.cpu.r.im == 2 {
                        self.cpu.adjust_time(6);
                        let addr = self.cpu.r.i as u32 * 0x100 + 0xFF;
                        self.cpu.r.pc = (self.ram[((addr + 1) & 0xFFFF) as usize] as u16) * 0x100
                            + self.ram[addr as usize] as u16;
                    }
                }
            }
        }

        let dur = self.cpu.time();
        *duration = dur;
        self.next_play -= dur;
        debug_assert!(self.next_play >= 0);
        self.cpu.adjust_time(-dur);

        self.apu.end_frame(dur);
        // C++ Classic_Emu::play_ calls buf->end_frame after run_clocks.
        self.stereo_buf.end_frame(dur);

        Ok(())
    }

    /// C++ Classic_Emu::play_.
    fn raw_play(&mut self, count: i32, out: &mut [i16]) -> GmeResult {
        let mut remaining = count;
        let mut pos = 0usize;
        while remaining > 0 {
            let avail = self.stereo_buf.samples_avail();
            if avail > 0 {
                let to_read = remaining.min(avail as i32);
                let read = self
                    .stereo_buf
                    .read_samples(&mut out[pos..], to_read as i64);
                pos += read as usize;
                remaining -= read as i32;
            }

            if remaining > 0 {
                let msec = self.stereo_buf.length();
                let mut clocks = msec * self.clock_rate as i32 / 1000;
                self.run_clocks(&mut clocks)?;
                debug_assert!(clocks > 0);
            }
        }
        Ok(())
    }

    /// C++ Music_Emu::fill_buf.
    fn fill_buf(&mut self) -> GmeResult {
        debug_assert!(self.emu_state.buf_remain == 0);
        if !self.emu_state.emu_track_ended {
            self.emu_state.emu_time += SILENCE_BUF_SIZE as i32;
            let mut buf = std::mem::take(&mut self.emu_state.silence_buf);
            let result = self.raw_play(SILENCE_BUF_SIZE as i32, &mut buf);
            self.emu_state.silence_buf = buf;
            result?;
            let silence = count_silence(&mut self.emu_state.silence_buf);
            if silence < SILENCE_BUF_SIZE {
                self.emu_state.silence_time = (self.emu_state.emu_time as i64) - silence;
                self.emu_state.buf_remain = SILENCE_BUF_SIZE;
                return Ok(());
            }
        }
        self.emu_state.silence_count += SILENCE_BUF_SIZE;
        Ok(())
    }
}

impl GmeFile for AyEmu {
    fn format(&self) -> Format {
        Format::Ay
    }

    fn raw_track_count(&self) -> i32 {
        self.state.raw_track_count
    }

    fn set_raw_track_count(&mut self, n: i32) {
        self.state.set_track_count(n);
    }

    fn load_mem_impl(&mut self, data: &[u8]) -> GmeResult {
        let parsed = parse_header(data)?;
        self.file_data = data.to_vec();
        self.tracks_offset = parsed.tracks;
        self.state.set_track_count(data[0x10] as i32 + 1);

        if data[8] > 2 {
            self.state.set_warning("Unknown file version");
        }

        self.emu_state.voice_count = OSC_COUNT as i32;
        self.apu.volume(self.emu_state.gain);

        // C++ setup_buffer( spectrum_clock )
        self.clock_rate = SPECTRUM_CLOCK;
        if self.buf_initialized {
            self.stereo_buf.clock_rate(SPECTRUM_CLOCK);
        }
        // C++ Music_Emu::post_load_ -> set_tempo( tempo_ )
        self.update_play_period();
        Ok(())
    }

    fn track_info_impl(&self, info: &mut TrackInfo, track: i32) -> GmeResult {
        info.system = "ZX Spectrum".to_string();
        self.copy_ay_fields(info, track);
        Ok(())
    }
}

impl MusicEmulator for AyEmu {
    fn format(&self) -> Format {
        Format::Ay
    }

    fn set_sample_rate_impl(&mut self, sample_rate: i32) -> GmeResult {
        self.emu_state.sample_rate = sample_rate;
        self.init_audio(sample_rate)
    }

    fn start_track_impl(&mut self, track: i32) -> GmeResult {
        self.emu_state.clear_track_vars();
        self.emu_state.current_track = track;

        // C++ Classic_Emu::start_track_: clear buffer first.
        self.stereo_buf.clear();

        // memset blocks from C++ Ay_Emu::start_track_
        self.ram[0x0000..0x0100].fill(0xC9); // fill RST vectors with RET
        self.ram[0x0100..0x4000].fill(0xFF);
        self.ram[RAM_START..MEM_SIZE].fill(0x00);
        self.ram[0x10000..MEM_SIZE].fill(0xFF);

        let file = &self.file_data;
        let file_size = file.len();
        let track = track as usize;

        // locate data blocks
        let data = get_data(file, self.tracks_offset + track * 4 + 2, 14)
            .ok_or(GmeError::Internal("File data missing"))?;
        let more_data =
            get_data(file, data + 10, 6).ok_or(GmeError::Internal("File data missing"))?;
        let mut blocks =
            get_data(file, data + 12, 8).ok_or(GmeError::Internal("File data missing"))?;

        // initial addresses
        self.cpu.reset(self.ram.as_mut_ptr());
        self.cpu.r.sp = get_be16(file, more_data);
        let hi = file[data + 8];
        let lo = file[data + 9];
        self.cpu.r.a = hi;
        self.cpu.r.b = hi;
        self.cpu.r.d = hi;
        self.cpu.r.h = hi;
        self.cpu.r.flags = lo;
        self.cpu.r.c = lo;
        self.cpu.r.e = lo;
        self.cpu.r.l = lo;
        self.cpu.r.alt_a = hi;
        self.cpu.r.alt_b = hi;
        self.cpu.r.alt_d = hi;
        self.cpu.r.alt_h = hi;
        self.cpu.r.alt_flags = lo;
        self.cpu.r.alt_c = lo;
        self.cpu.r.alt_e = lo;
        self.cpu.r.alt_l = lo;
        let hl = self.cpu.r.hl();
        self.cpu.r.ix = hl;
        self.cpu.r.iy = hl;

        let mut addr = get_be16(file, blocks) as usize;
        if addr == 0 {
            return Err(GmeError::Internal("File data missing"));
        }

        let mut init = get_be16(file, more_data + 2) as usize;
        if init == 0 {
            init = addr;
        }

        // copy blocks into memory
        loop {
            blocks += 2;
            let mut len = get_be16(file, blocks) as usize;
            blocks += 2;
            if addr + len > 0x10000 {
                self.state.set_warning("Bad data block size");
                len = 0x10000 - addr;
            }
            let input = get_data(file, blocks, 0);
            blocks += 2;
            match input {
                Some(input) => {
                    if len > file_size - input {
                        self.state.set_warning("Missing file data");
                        len = file_size - input;
                    }
                    self.ram[addr..addr + len].copy_from_slice(&file[input..input + len]);
                }
                None => {
                    // C++ would read from a null pointer here; treat as no data.
                    self.state.set_warning("Missing file data");
                }
            }

            if file_size - blocks < 8 {
                self.state.set_warning("Missing file data");
                break;
            }

            addr = get_be16(file, blocks) as usize;
            if addr == 0 {
                break;
            }
        }

        // copy and configure driver
        const PASSIVE: [u8; 10] = [
            0xF3, // DI
            0xCD, 0, 0, // CALL init
            0xED, 0x5E, // LOOP: IM 2
            0xFB, // EI
            0x76, // HALT
            0x18, 0xFA, // JR LOOP
        ];
        const ACTIVE: [u8; 13] = [
            0xF3, // DI
            0xCD, 0, 0, // CALL init
            0xED, 0x56, // LOOP: IM 1
            0xFB, // EI
            0x76, // HALT
            0xCD, 0, 0, // CALL play
            0x18, 0xF7, // JR LOOP
        ];
        self.ram[..PASSIVE.len()].copy_from_slice(&PASSIVE);
        let play_addr = get_be16(file, more_data + 4);
        if play_addr != 0 {
            self.ram[..ACTIVE.len()].copy_from_slice(&ACTIVE);
            self.ram[9] = play_addr as u8;
            self.ram[10] = (play_addr >> 8) as u8;
        }
        self.ram[2] = init as u8;
        self.ram[3] = (init >> 8) as u8;

        self.ram[0x38] = 0xFB; // Put EI at interrupt vector (followed by RET)

        // some code wraps around (ugh)
        self.ram.copy_within(0..0x80, 0x10000);

        self.beeper_delta = (AMP_RANGE as f64 * 0.65) as i32;
        self.last_beeper = 0;
        self.apu.reset();
        self.next_play = self.play_period;

        // start at spectrum speed
        self.change_clock_rate(SPECTRUM_CLOCK);
        self.update_play_period();

        self.spectrum_mode = false;
        self.cpc_mode = false;
        self.cpc_latch = 0;

        // ---- Music_Emu::start_track machinery (copied from gme-nsf) ----
        self.emu_state.emu_track_ended = false;
        self.emu_state.track_ended = false;
        self.emu_state.emu_time = 0;
        self.emu_state.buf_remain = 0;
        self.emu_state.silence_time = 0;
        self.emu_state.silence_count = 0;
        self.emu_state.out_time = 0;
        self.emu_state.out_time_scaled = 0;

        // Pre-fill: run emulator until non-silent audio found.
        if !self.emu_state.ignore_silence_flag {
            let end =
                self.emu_state.max_initial_silence as i64 * 2 * self.emu_state.sample_rate as i64;
            while (self.emu_state.emu_time as i64) < end {
                self.fill_buf()?;
                if self.emu_state.buf_remain > 0 || self.emu_state.emu_track_ended {
                    break;
                }
            }
            self.emu_state.emu_time = self.emu_state.buf_remain as i32;
            self.emu_state.out_time = 0;
            self.emu_state.out_time_scaled = 0;
            self.emu_state.silence_time = 0;
            self.emu_state.silence_count = 0;
        }
        Ok(())
    }

    // C++ Music_Emu::play (structure proven bit-exact by gme-spc/gme-nsf)
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
            // during a run of silence, run emulator at >=2x speed so it gets ahead
            let ahead_time = self.emu_state.silence_lookahead as i64
                * (self.emu_state.out_time as i64 + out_count as i64 - self.emu_state.silence_time)
                + self.emu_state.silence_time;
            while (self.emu_state.emu_time as i64) < ahead_time
                && self.emu_state.buf_remain == 0
                && !self.emu_state.emu_track_ended
            {
                self.fill_buf()?;
            }

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

        // Return held-back buffer first
        if self.emu_state.buf_remain > 0 && pos < out_count {
            let n = (self.emu_state.buf_remain as usize).min(out_count - pos);
            let buf_start = (SILENCE_BUF_SIZE - self.emu_state.buf_remain) as usize;
            let end = buf_start + n;
            out[pos..pos + n].copy_from_slice(&self.emu_state.silence_buf[buf_start..end]);
            self.emu_state.buf_remain -= n as i64;
            pos += n;
        }

        // Generate remaining samples
        if pos < out_count {
            let remain = (out_count - pos) as i32;
            self.emu_state.emu_time += remain;
            // C++ Music_Emu::emu_play: once the track has ended, output
            // hard zeros instead of running the emulator
            if !self.emu_state.emu_track_ended {
                self.raw_play(remain, &mut out[pos..])?;
            } else {
                out[pos..pos + remain as usize].fill(0);
            }
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;

            if !self.emu_state.ignore_silence_flag
                || self.emu_state.out_time > self.emu_state.fade_start
            {
                // check end for a new run of silence
                let silence = count_silence(&mut out[pos..pos + remain as usize]);
                if silence < remain as i64 {
                    self.emu_state.silence_time = self.emu_state.emu_time as i64 - silence;
                }
                if self.emu_state.emu_time as i64 - self.emu_state.silence_time > SILENCE_BUF_SIZE {
                    self.fill_buf()?; // cause silence detection on next play()
                }
            }
        }

        // Handle fading
        if self.emu_state.fade_start >= 0
            && self.emu_state.out_time > self.emu_state.fade_start
            && self.emu_state.handle_fade(self.emu_state.out_time, out)
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
                let saved_mute = self.mute_mask;
                self.mute_voices_impl(!0);

                while count > threshold / 2 && !self.emu_state.emu_track_ended {
                    self.raw_play(SILENCE_BUF_SIZE as i32, &mut scratch)?;
                    count -= SILENCE_BUF_SIZE as i32;
                }

                self.mute_voices_impl(saved_mute);
            }

            while count > 0 && !self.emu_state.emu_track_ended {
                let n = (count as i64).min(SILENCE_BUF_SIZE) as i32;
                count -= n;
                self.raw_play(n, &mut scratch)?;
            }
        }

        if self.emu_state.silence_count == 0 && self.emu_state.buf_remain == 0 {
            self.emu_state.track_ended |= self.emu_state.emu_track_ended;
        }
        Ok(())
    }

    fn set_tempo_impl(&mut self, tempo: f64) {
        self.emu_state.tempo = tempo;
        self.update_play_period();
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
        self.mute_mask = mask;
        for i in 0..APU_OSC_COUNT {
            let out = if mask & (1 << i) != 0 {
                std::ptr::null_mut()
            } else {
                self.stereo_buf
                    .channel(i as i32, CHANNEL_WAVE_TYPE | i as i32)
                    .center
            };
            self.apu.osc_output(i, out);
        }
    }

    fn voice_count(&self) -> i32 {
        OSC_COUNT as i32
    }

    fn voice_names(&self) -> &[&str] {
        &VOICE_NAMES
    }
}
