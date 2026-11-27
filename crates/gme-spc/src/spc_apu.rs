// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2004-2007 Shay Green
// Rust adaptation of Snes_Spc.cpp, Spc_Cpu.cpp and Spc_Cpu.h (2026-06-09 to 2026-06-14), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use crate::spc_dsp::{self, SpcDsp};

pub const ROM_SIZE: usize = 0x40;
pub const ROM_ADDR: u16 = 0xFFC0;
pub const CLOCK_RATE: i32 = 1_024_000;
pub const CLOCKS_PER_SAMPLE: i32 = 32;
pub const SAMPLE_RATE: i32 = 32000;
pub const VOICE_COUNT: i32 = 8;
pub const PORT_COUNT: usize = 4;
pub const TIMER_COUNT: usize = 3;
pub const TEMPO_UNIT: i32 = 0x100;
pub const SPC_FILE_SIZE: usize = 0x10200;
pub const SPC_MIN_FILE_SIZE: usize = 0x10180;
pub const SIGNATURE_SIZE: usize = 35;

/// Opcodes recorded by `cpu_trace`: (pc, opcode, a, x, y, sp, rel_time, psw)
pub type CpuTraceEntry = (u16, u8, u8, u8, u8, u8, i32, u8);

const REG_COUNT: usize = 0x10;
const CPU_PAD_FILL: u8 = 0xFF;
const SKIPPING_TIME: i32 = 127;
// If write isn't preceded by read, data has this added to it (suppresses the
// timer-counter clear in cpu_write_smp_reg_).
const NO_READ_BEFORE_WRITE: i32 = 0x2000;

/// Thin wrapper around a raw pointer to 64K RAM, providing safe Index/IndexMut.
/// SAFETY: The pointer must remain valid for the lifetime of this struct.
struct RamPtr(*mut u8);

impl std::ops::Index<usize> for RamPtr {
    type Output = u8;
    #[inline]
    fn index(&self, addr: usize) -> &u8 {
        unsafe { &*self.0.add(addr & 0xFFFF) }
    }
}

impl std::ops::IndexMut<usize> for RamPtr {
    #[inline]
    fn index_mut(&mut self, addr: usize) -> &mut u8 {
        unsafe { &mut *self.0.add(addr & 0xFFFF) }
    }
}

// SMP register addresses
const R_TEST: usize = 0x0;
const R_CONTROL: usize = 0x1;
const R_DSPADDR: usize = 0x2;
const R_DSPDATA: usize = 0x3;
const R_CPUIO0: usize = 0x4;
#[allow(dead_code)]
const R_CPUIO3: usize = 0x7;
const R_T0TARGET: usize = 0xA;
const R_T1TARGET: usize = 0xB;
const R_T2TARGET: usize = 0xC;
const R_T0OUT: usize = 0xD;
#[allow(dead_code)]
const R_T2OUT: usize = 0xF;

// SPC_LESS_ACCURATE mode DSP register timing
const MAX_REG_TIME: i32 = 29;

static REG_TIMES: [i8; 256] = [
    -1, 0, -11, -10, -15, -11, -2, -2, 4, 3, 14, 14, 26, 26, 14, 22, 2, 3, 0, 1, -12, 0, 1, 1, 7,
    6, 14, 14, 27, 14, 14, 23, 5, 6, 3, 4, -1, 3, 4, 4, 10, 9, 14, 14, 26, -5, 14, 23, 8, 9, 6, 7,
    2, 6, 7, 7, 13, 12, 14, 14, 27, -4, 14, 24, 11, 12, 9, 10, 5, 9, 10, 10, 16, 15, 14, 14, -2,
    -4, 14, 24, 14, 15, 12, 13, 8, 12, 13, 13, 19, 18, 14, 14, -2, -36, 14, 24, 17, 18, 15, 16, 11,
    15, 16, 16, 22, 21, 14, 14, 28, -3, 14, 25, 20, 21, 18, 19, 14, 18, 19, 19, 25, 24, 14, 14, 14,
    29, 14, 25, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
    29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
    29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
    29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
    29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
    29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
];

static CYCLE_TABLE_PACKED: [u8; 128] = [
    0x28, 0x47, 0x34, 0x36, 0x26, 0x54, 0x54, 0x68, 0x48, 0x47, 0x45, 0x56, 0x55, 0x65, 0x22, 0x46,
    0x28, 0x47, 0x34, 0x36, 0x26, 0x54, 0x54, 0x74, 0x48, 0x47, 0x45, 0x56, 0x55, 0x65, 0x22, 0x38,
    0x28, 0x47, 0x34, 0x36, 0x26, 0x44, 0x54, 0x66, 0x48, 0x47, 0x45, 0x56, 0x55, 0x45, 0x22, 0x43,
    0x28, 0x47, 0x34, 0x36, 0x26, 0x44, 0x54, 0x75, 0x48, 0x47, 0x45, 0x56, 0x55, 0x55, 0x22, 0x36,
    0x28, 0x47, 0x34, 0x36, 0x26, 0x54, 0x52, 0x45, 0x48, 0x47, 0x45, 0x56, 0x55, 0x55, 0x22, 0xC5,
    0x38, 0x47, 0x34, 0x36, 0x26, 0x44, 0x52, 0x44, 0x48, 0x47, 0x45, 0x56, 0x55, 0x55, 0x22, 0x34,
    0x38, 0x47, 0x45, 0x47, 0x25, 0x64, 0x52, 0x49, 0x48, 0x47, 0x56, 0x67, 0x45, 0x55, 0x22, 0x83,
    0x28, 0x47, 0x34, 0x36, 0x24, 0x53, 0x43, 0x40, 0x48, 0x47, 0x45, 0x56, 0x34, 0x54, 0x22, 0x60,
];

#[derive(Clone, Copy)]
struct Timer {
    next_time: i32,
    prescaler: i32,
    period: i32,
    divider: u8,
    enabled: i32,
    counter: i32,
}

impl Timer {
    const fn new() -> Self {
        Self {
            next_time: 0,
            prescaler: 0,
            period: 0,
            divider: 0,
            enabled: 0,
            counter: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct CpuRegs {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub psw: u8,
    pub sp: u8,
}

impl CpuRegs {
    const fn new() -> Self {
        Self {
            pc: 0,
            a: 0,
            x: 0,
            y: 0,
            psw: 0,
            sp: 0,
        }
    }
}

pub struct SpcApu {
    pub dsp: SpcDsp,
    timers: [Timer; TIMER_COUNT],
    smp_regs: [[u8; REG_COUNT]; 2], // [0]=out, [1]=in
    pub cpu_regs: CpuRegs,
    dsp_time: i32,
    spc_time: i32,
    tempo: i32,
    skipped_kon: i32,
    skipped_koff: i32,
    cpu_error: Option<&'static str>,
    /// Reusable discard buffer for skip(); avoids a fresh allocation per call.
    /// When Some, every DSP register write is recorded as (time, addr, data);
    /// mirrors the C++ SPC_DSP_WRITE_HOOK used by the differential tests.
    /// addr 0x100 marks a dsp.run() event, with time = clock_count.
    pub dsp_write_trace: Option<Vec<(i32, u16, u8)>>,
    /// When Some, the first 200k executed opcodes are recorded as
    /// (pc, opcode, a, x, y, sp, rel_time, psw); mirrors the C++ SPC_CPU_OPCODE_HOOK.
    pub cpu_trace: Option<Vec<CpuTraceEntry>>,
    extra_clocks: i32,
    buf_begin_ptr: *mut i16,
    buf_end_ptr: *mut i16,
    extra_pos: usize,
    extra_buf: [i16; spc_dsp::EXTRA_SIZE],
    rom_enabled: i32,
    rom: [u8; ROM_SIZE],
    hi_ram: [u8; ROM_SIZE],
    cycle_table: [u8; 256],
    reg_times: [i8; 256],
    // 64K RAM + padding
    padding1: [u8; 0x100],
    pub ram: [u8; 0x10000],
    ram_tail: [u8; 0x100],
}

impl Default for SpcApu {
    fn default() -> Self {
        Self::new()
    }
}

impl SpcApu {
    pub fn new() -> Self {
        Self {
            dsp: SpcDsp::new(),
            timers: [Timer::new(); TIMER_COUNT],
            smp_regs: [[0; REG_COUNT]; 2],
            cpu_regs: CpuRegs::new(),
            dsp_time: 0,
            spc_time: 0,
            tempo: TEMPO_UNIT,
            skipped_kon: 0,
            skipped_koff: 0,
            cpu_error: None,
            dsp_write_trace: None,
            cpu_trace: None,
            extra_clocks: 0,
            buf_begin_ptr: std::ptr::null_mut(),
            buf_end_ptr: std::ptr::null_mut(),
            extra_pos: 0,
            extra_buf: [0; spc_dsp::EXTRA_SIZE],
            rom_enabled: 0,
            rom: [0; ROM_SIZE],
            hi_ram: [0; ROM_SIZE],
            cycle_table: [0; 256],
            reg_times: REG_TIMES,
            padding1: [CPU_PAD_FILL; 0x100],
            ram: [0; 0x10000],
            ram_tail: [CPU_PAD_FILL; 0x100],
        }
    }

    pub fn init(&mut self) {
        self.dsp.init();
        self.tempo = TEMPO_UNIT;
        // Default IPL ROM: all zeros except reset vector
        // The SPC program disables the ROM overlay and runs from RAM
        self.rom = [0; ROM_SIZE];
        self.rom[0x3E] = 0xFF;
        self.rom[0x3F] = 0xC0;
        // Unpack cycle table
        for (i, &n) in CYCLE_TABLE_PACKED.iter().enumerate() {
            self.cycle_table[i * 2] = n >> 4;
            self.cycle_table[i * 2 + 1] = n & 0x0F;
        }
        self.reg_times.copy_from_slice(&REG_TIMES);
        self.reset();
    }

    pub fn init_rom(&mut self, rom: &[u8; ROM_SIZE]) {
        self.rom.copy_from_slice(rom);
    }

    pub fn set_tempo(&mut self, t: i32) {
        self.tempo = t;
        let timer2_shift = 4i32; // 64 kHz
        let other_shift = 3i32; //  8 kHz
        let t = if t == 0 { 1 } else { t };
        let timer2_rate = 1i32 << timer2_shift;
        let mut rate = (timer2_rate * TEMPO_UNIT + (t >> 1)) / t;
        if rate < timer2_rate / 4 {
            rate = timer2_rate / 4;
        }
        self.timers[2].prescaler = rate;
        self.timers[1].prescaler = rate << other_shift;
        self.timers[0].prescaler = rate << other_shift;
    }

    fn if_0_then_256(n: u8) -> i32 {
        (n.wrapping_sub(1)) as i32 + 1
    }

    fn timers_loaded(&mut self) {
        for i in 0..TIMER_COUNT {
            let t = &mut self.timers[i];
            t.period = Self::if_0_then_256(self.smp_regs[0][R_T0TARGET + i]);
            t.enabled = (self.smp_regs[0][R_CONTROL] >> i) as i32 & 1;
            t.counter = (self.smp_regs[1][R_T0OUT + i] & 0x0F) as i32;
        }
        self.set_tempo(self.tempo);
    }

    fn load_regs(&mut self, in_regs: &[u8; REG_COUNT]) {
        self.smp_regs[0][..REG_COUNT].copy_from_slice(in_regs);
        self.smp_regs[1][..REG_COUNT].copy_from_slice(in_regs);
        self.smp_regs[1][R_TEST] = 0;
        self.smp_regs[1][R_CONTROL] = 0;
        self.smp_regs[1][R_T0TARGET] = 0;
        self.smp_regs[1][R_T1TARGET] = 0;
        self.smp_regs[1][R_T2TARGET] = 0;
    }

    fn ram_loaded(&mut self) {
        self.rom_enabled = 0;
        let mut regs = [0u8; REG_COUNT];
        regs.copy_from_slice(&self.ram[0xF0..0x100]);
        self.load_regs(&regs);
        self.padding1.fill(CPU_PAD_FILL);
        self.ram_tail.fill(CPU_PAD_FILL);
    }

    fn regs_loaded(&mut self) {
        self.enable_rom(self.smp_regs[0][R_CONTROL] as i32 & 0x80);
        self.timers_loaded();
    }

    fn reset_time_regs(&mut self) {
        self.cpu_error = None;
        self.spc_time = 0;
        self.dsp_time = CLOCKS_PER_SAMPLE + 1; // SPC_LESS_ACCURATE

        for t in self.timers.iter_mut() {
            t.next_time = 1;
            t.divider = 0;
        }

        self.regs_loaded();
        self.extra_clocks = 0;
        self.reset_buf();
    }

    fn reset_common(&mut self, timer_counter_init: u8) {
        for i in 0..TIMER_COUNT {
            self.smp_regs[1][R_T0OUT + i] = timer_counter_init;
        }
        self.cpu_regs = CpuRegs::new();
        self.cpu_regs.pc = ROM_ADDR;
        self.smp_regs[0][R_TEST] = 0x0A;
        self.smp_regs[0][R_CONTROL] = 0xB0;
        for i in 0..PORT_COUNT {
            self.smp_regs[1][R_CPUIO0 + i] = 0;
        }
        self.reset_time_regs();
    }

    pub fn soft_reset(&mut self) {
        self.reset_common(0);
        self.dsp.soft_reset();
    }

    pub fn reset(&mut self) {
        self.ram.fill(0xFF);
        self.ram_loaded();
        self.reset_common(0x0F);
        self.dsp.reset();
    }

    fn enable_rom(&mut self, enable: i32) {
        if self.rom_enabled != enable {
            self.rom_enabled = enable;
            if enable != 0 {
                self.hi_ram
                    .copy_from_slice(&self.ram[ROM_ADDR as usize..ROM_ADDR as usize + ROM_SIZE]);
                self.ram[ROM_ADDR as usize..ROM_ADDR as usize + ROM_SIZE]
                    .copy_from_slice(&self.rom);
            } else {
                self.ram[ROM_ADDR as usize..ROM_ADDR as usize + ROM_SIZE]
                    .copy_from_slice(&self.hi_ram);
            }
        }
    }

    fn reset_buf(&mut self) {
        // Start with half extra buffer of silence (C++ Snes_Spc::reset_buf)
        const EXTRA_HALF: usize = spc_dsp::EXTRA_SIZE / 2;
        self.extra_buf[..EXTRA_HALF].fill(0);
        self.extra_pos = EXTRA_HALF;
        self.buf_begin_ptr = std::ptr::null_mut();
        self.buf_end_ptr = std::ptr::null_mut();
        // DSP writes to its own internal extra
        self.dsp.set_output(None);
    }

    pub fn set_output(&mut self, out: Option<&mut [i16]>) {
        self.extra_clocks &= CLOCKS_PER_SAMPLE - 1;
        if let Some(buf) = out {
            debug_assert!(buf.len() & 1 == 0);
            let size = buf.len();
            self.buf_begin_ptr = buf.as_mut_ptr();
            self.buf_end_ptr = unsafe { buf.as_mut_ptr().add(size) };

            // Copy extra to output
            let mut in_idx = 0;
            let mut out_idx = 0;
            while in_idx < self.extra_pos && out_idx < size {
                unsafe {
                    *self.buf_begin_ptr.add(out_idx) = self.extra_buf[in_idx];
                }
                in_idx += 1;
                out_idx += 1;
            }

            if out_idx >= size {
                // Output full already — write remaining extras to DSP extra
                let dsp_extra = self.dsp.extra();
                let mut dp_idx = 0;
                while in_idx < self.extra_pos && dp_idx < spc_dsp::EXTRA_SIZE {
                    dsp_extra[dp_idx] = self.extra_buf[in_idx];
                    in_idx += 1;
                    dp_idx += 1;
                }
                self.dsp.set_output(None);
            } else {
                // DSP writes to remaining space after extras
                let remaining = &mut buf[out_idx..];
                self.dsp.set_output(Some(remaining));
            }
        } else {
            self.reset_buf();
        }
    }

    fn save_extra(&mut self) {
        let sample_count = self.sample_count();
        let dsp_out_pos = self.dsp.out_pos_ptr() as *const i16;
        let main_end = self.buf_end_ptr as *const i16;
        let dsp_extra = self.dsp.extra();

        // Determine actual main_end: min of buf_end and dsp_out_pos
        let actual_main_end =
            if dsp_out_pos >= self.buf_begin_ptr as *const i16 && dsp_out_pos <= main_end {
                dsp_out_pos
            } else {
                main_end
            };

        // Determine dsp_end
        let dsp_end = if dsp_out_pos >= self.buf_begin_ptr as *const i16 && dsp_out_pos <= main_end
        {
            dsp_extra.as_ptr()
        } else {
            dsp_out_pos
        };

        // Copy any extra samples at these ends into extra_buf
        let mut out = 0;
        let mut in_ptr = unsafe { (self.buf_begin_ptr as *const i16).add(sample_count) };
        while in_ptr < actual_main_end && out < spc_dsp::EXTRA_SIZE {
            unsafe {
                self.extra_buf[out] = *in_ptr;
                in_ptr = in_ptr.add(1);
            }
            out += 1;
        }
        let mut dp_ptr = dsp_extra.as_ptr();
        while dp_ptr < dsp_end && out < spc_dsp::EXTRA_SIZE {
            unsafe {
                self.extra_buf[out] = *dp_ptr;
                dp_ptr = dp_ptr.add(1);
            }
            out += 1;
        }
        self.extra_pos = out;
    }

    pub fn sample_count(&self) -> usize {
        (self.extra_clocks as usize >> 5) * 2
    }

    pub fn play(&mut self, count: usize, out: &mut [i16]) -> Option<&'static str> {
        debug_assert!(count & 1 == 0);
        if count > 0 {
            self.set_output(Some(out));
            self.end_frame((count as i32) * (CLOCKS_PER_SAMPLE / 2));
        }

        self.cpu_error.take()
    }

    pub fn skip(&mut self, count: usize) -> Option<&'static str> {
        let mut count = count;
        if count > 2 * SAMPLE_RATE as usize * 2 {
            // C++ set_output( 0, 0 ): the Snes_Spc-level call, which masks
            // extra_clocks and resets the extra-sample buffer
            self.extra_clocks &= CLOCKS_PER_SAMPLE - 1;
            self.reset_buf();

            // C++ fast-forwards all but (count & 3) + sample_rate*2 samples
            // with the DSP detached, then PLAYS only that reduced remainder.
            let total = count;
            count = (count & 3) + SAMPLE_RATE as usize * 2;
            let end = ((total - count) * (CLOCKS_PER_SAMPLE as usize / 2)) as i32;

            self.skipped_kon = 0;
            self.skipped_koff = 0;

            let old_dsp_time = self.dsp_time + self.spc_time;
            self.dsp_time = end - self.spc_time + SKIPPING_TIME;
            self.end_frame(end);
            self.dsp_time = self.dsp_time - SKIPPING_TIME + old_dsp_time;

            self.dsp.write(
                spc_dsp::R_KOFF,
                (self.skipped_koff & !self.skipped_kon) as u8,
            );
            self.dsp.write(spc_dsp::R_KON, self.skipped_kon as u8);
            self.clear_echo();
        }
        // C++ play( count, 0 ): a null output buffer resets the extra-sample
        // accounting (set_output(0, count) → reset_buf) instead of copying
        // pending extras into the output, and end_frame skips save_extra.
        if count > 0 {
            self.extra_clocks &= CLOCKS_PER_SAMPLE - 1;
            self.reset_buf();
            self.end_frame((count as i32) * (CLOCKS_PER_SAMPLE / 2));
        }
        self.cpu_error.take()
    }

    pub fn load_spc(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < SIGNATURE_SIZE {
            return Err("Not an SPC file");
        }
        let sig = b"SNES-SPC700 Sound File Data v0.30\x1A\x1A";
        if data[..27] != sig[..27] {
            return Err("Not an SPC file");
        }
        if data.len() < SPC_MIN_FILE_SIZE {
            return Err("Corrupt SPC file");
        }

        // CPU registers
        self.cpu_regs.pc = data[38] as u16 * 0x100 + data[37] as u16;
        self.cpu_regs.a = data[39];
        self.cpu_regs.x = data[40];
        self.cpu_regs.y = data[41];
        self.cpu_regs.psw = data[42];
        self.cpu_regs.sp = data[43];

        // RAM
        self.ram.copy_from_slice(&data[0x100..0x10100]);
        self.ram_loaded();

        // DSP registers
        let mut dsp_regs = [0u8; spc_dsp::REGISTER_COUNT];
        dsp_regs.copy_from_slice(&data[0x10100..0x10180]);
        self.dsp.load(&dsp_regs);

        self.reset_time_regs();
        Ok(())
    }

    pub fn clear_echo(&mut self) {
        if self.dsp.read(spc_dsp::R_FLG) & 0x20 == 0 {
            let addr = 0x100 * self.dsp.read(spc_dsp::R_ESA) as usize;
            let mut end = addr + 0x800 * (self.dsp.read(spc_dsp::R_EDL) as usize & 0x0F);
            if end > 0x10000 {
                end = 0x10000;
            }
            self.ram[addr..end].fill(0xFF);
        }
    }

    pub fn mute_voices(&mut self, mask: i32) {
        self.dsp.mute_voices(mask);
    }

    pub fn disable_surround(&mut self, disable: bool) {
        self.dsp.disable_surround(disable);
    }

    pub fn disable_echo(&mut self, disable: bool) {
        self.dsp.disable_echo(disable);
    }

    pub fn end_frame(&mut self, end_time: i32) {
        if end_time > self.spc_time {
            self.run_until(end_time);
        }

        self.spc_time -= end_time;
        self.extra_clocks += end_time;

        // Catch timers up to CPU
        for i in 0..TIMER_COUNT {
            self.run_timer(i, 0);
        }

        // Catch DSP up to CPU (matches C++ SPC_LESS_ACCURATE end_frame)
        if self.dsp_time < 0 {
            self.run_dsp(0, MAX_REG_TIME);
        }

        if !self.buf_begin_ptr.is_null() {
            self.save_extra();
        }
    }

    // Timer handling
    fn run_timer_(&mut self, idx: usize, time: i32) {
        let t = &mut self.timers[idx];
        let elapsed = (time - t.next_time) / t.prescaler + 1;
        t.next_time += elapsed * t.prescaler;

        if t.enabled != 0 {
            let remain = Self::if_0_then_256((t.period - t.divider as i32) as u8);
            let divider = t.divider as i32 + elapsed;
            let over = elapsed - remain;
            if over >= 0 {
                let n = over / t.period;
                t.counter = (t.counter + 1 + n) & 0x0F;
                t.divider = (over - n * t.period) as u8;
            } else {
                t.divider = divider as u8;
            }
        }
    }

    fn run_timer(&mut self, idx: usize, time: i32) {
        if time >= self.timers[idx].next_time {
            self.run_timer_(idx, time);
        }
    }

    // DSP synchronization
    fn run_dsp(&mut self, time: i32, offset: i32) {
        let count = time - offset - self.dsp_time;
        if count >= 0 {
            let clock_count = (count & !(CLOCKS_PER_SAMPLE - 1)) + CLOCKS_PER_SAMPLE;
            self.dsp_time += clock_count;
            if let Some(trace) = self.dsp_write_trace.as_mut() {
                trace.push((clock_count, 0x100, 0));
            }
            self.dsp.run(clock_count, &mut self.ram);
        }
    }

    fn dsp_read(&mut self, time: i32) -> u8 {
        self.run_dsp(
            time,
            self.reg_times[self.smp_regs[0][R_DSPADDR] as usize & 0x7F] as i32,
        );
        self.dsp.read(self.smp_regs[0][R_DSPADDR] as usize & 0x7F)
    }

    fn dsp_write(&mut self, data: u8, time: i32) {
        let r = self.smp_regs[0][R_DSPADDR] as usize;

        // RUN_DSP with C++'s else-branch: skipped KON/KOFF are only tracked
        // while skip() has parked dsp_time at SKIPPING_TIME.
        let count = time - self.reg_times[r] as i32 - self.dsp_time;
        if count >= 0 {
            let clock_count = (count & !(CLOCKS_PER_SAMPLE - 1)) + CLOCKS_PER_SAMPLE;
            self.dsp_time += clock_count;
            if let Some(trace) = self.dsp_write_trace.as_mut() {
                trace.push((clock_count, 0x100, 0));
            }
            self.dsp.run(clock_count, &mut self.ram);
        } else if self.dsp_time == SKIPPING_TIME {
            if r == spc_dsp::R_KON {
                self.skipped_kon |= (data as i32) & !(self.dsp.read(spc_dsp::R_KOFF) as i32);
            }
            if r == spc_dsp::R_KOFF {
                self.skipped_koff |= data as i32;
                self.skipped_kon &= !(data as i32);
            }
        }

        if let Some(trace) = self.dsp_write_trace.as_mut() {
            trace.push((self.spc_time + time, r as u16, data));
        }

        if r <= 0x7F {
            self.dsp.write(r, data);
        }
    }

    // CPU memory access.
    // `data` is i32 so it can carry the C++ no_read_before_write marker
    // (0x2000 added to writes not preceded by a read), which suppresses the
    // timer-counter clear on writes to $FD-$FF.
    fn cpu_write_smp_reg_(&mut self, data: i32, time: i32, addr: u16) {
        match addr as usize {
            R_T0TARGET..=R_T2TARGET => {
                let idx = addr as usize - R_T0TARGET;
                let period = Self::if_0_then_256(data as u8);
                if self.timers[idx].period != period {
                    self.run_timer_(idx, time);
                    self.timers[idx].period = period;
                }
            }
            0xD..=0xF
                // Timer counters
                if data < NO_READ_BEFORE_WRITE / 2 => {
                    let idx = addr as usize - 0xD;
                    self.run_timer(idx, time - 1);
                    self.timers[idx].counter = 0;
                }
            0x8 | 0x9 => {
                self.smp_regs[1][addr as usize] = data as u8;
            }
            R_TEST => {
                // Just log
            }
            R_CONTROL => {
                // Port clears
                if data & 0x10 != 0 {
                    self.smp_regs[1][R_CPUIO0] = 0;
                    self.smp_regs[1][R_CPUIO0 + 1] = 0;
                }
                if data & 0x20 != 0 {
                    self.smp_regs[1][R_CPUIO0 + 2] = 0;
                    self.smp_regs[1][R_CPUIO0 + 3] = 0;
                }
                // Timers
                for i in 0..TIMER_COUNT {
                    let enabled = (data >> i) & 1;
                    if self.timers[i].enabled != enabled {
                        self.run_timer(i, time);
                        self.timers[i].enabled = enabled;
                        if enabled != 0 {
                            self.timers[i].divider = 0;
                            self.timers[i].counter = 0;
                        }
                    }
                }
                self.enable_rom(data & 0x80);
            }
            _ => {}
        }
    }

    fn cpu_write_smp_reg(&mut self, data: i32, time: i32, addr: u16) {
        if addr as usize == R_DSPDATA {
            self.dsp_write(data as u8, time);
        } else {
            self.cpu_write_smp_reg_(data, time, addr);
        }
    }

    fn cpu_write_high(&mut self, data: u8, i: u8) {
        self.hi_ram[i as usize] = data;
        if self.rom_enabled != 0 {
            self.ram[ROM_ADDR as usize + i as usize] = self.rom[i as usize];
        }
    }

    /// Process a CPU write to a register in the $F0-$FF range.
    /// This should be called from ALL instruction handlers that write to $F0-$FF.
    fn cpu_write_reg(&mut self, reg: usize, data: i32, time: i32) {
        self.smp_regs[0][reg] = data as u8;
        if reg == R_DSPDATA {
            self.dsp_write(data as u8, time);
        } else if reg != 2 && !(4..=7).contains(&reg) {
            self.cpu_write_smp_reg_(data, time, reg as u16);
        }
    }

    /// Central CPU memory write handler. All instruction writes should go through here.
    /// Matches C++ Snes_Spc::cpu_write: always writes `data` to RAM first, then handles
    /// register/ROM-area side effects.
    fn cpu_write_mem(&mut self, addr: u16, data: i32, time: i32) {
        self.ram[addr as usize] = data as u8;
        if addr >= 0xF0 {
            let reg = (addr - 0xF0) as usize;
            if reg < REG_COUNT {
                self.smp_regs[0][reg] = data as u8;
                if reg != 2 && !(4..=7).contains(&reg) {
                    self.cpu_write_smp_reg(data, time, reg as u16);
                }
            } else if addr >= ROM_ADDR {
                self.cpu_write_high(data as u8, (addr - ROM_ADDR) as u8);
            }
        }
    }

    fn cpu_read_smp_reg(&mut self, reg: usize, time: i32) -> u8 {
        let mut result = self.smp_regs[1][reg];
        let reg_offset = reg.wrapping_sub(R_DSPADDR);
        if reg_offset <= 1 {
            result = self.smp_regs[0][R_DSPADDR];
            if reg_offset == 1 {
                result = self.dsp_read(time);
            }
        }
        result
    }

    fn cpu_read(&mut self, addr: u16, time: i32) -> u8 {
        let result = self.ram[addr as usize];
        if addr >= 0xF0 {
            let reg = (addr - 0xF0) as usize;
            // Timer counters $FD-$FF
            let ti = (addr as usize).wrapping_sub(0xFD);
            if ti < TIMER_COUNT {
                if time >= self.timers[ti].next_time {
                    self.run_timer_(ti, time);
                }
                let counter = self.timers[ti].counter as u8;
                self.timers[ti].counter = 0;
                return counter;
            }
            // SMP registers $F0-$FC
            if reg < REG_COUNT {
                return self.cpu_read_smp_reg(reg, time);
            }
        }
        result
    }

    // Main CPU execution loop
    pub fn run_until(&mut self, end_time: i32) -> *const u8 {
        let mut rel_time = self.spc_time - end_time;
        debug_assert!(rel_time <= 0);
        self.spc_time = end_time;
        self.dsp_time += rel_time;
        self.timers[0].next_time += rel_time;
        self.timers[1].next_time += rel_time;
        self.timers[2].next_time += rel_time;

        let mut ram = RamPtr(self.ram.as_mut_ptr());
        let mut a = self.cpu_regs.a;
        let mut x = self.cpu_regs.x;
        let mut y = self.cpu_regs.y;
        let mut pc = self.cpu_regs.pc;
        let mut sp = self.cpu_regs.sp;

        // Decode PSW into separate variables
        let psw_in = self.cpu_regs.psw;
        let mut psw = psw_in;
        let mut c = (psw_in as i32) << 8;
        let mut dp = ((psw_in as i32) << 3) & 0x100;
        let mut nz = ((psw_in as i32) << 4 & 0x800) | (!(psw_in as i32) & 0x02);

        // Helper closures
        let _smp_regs_0 = &mut self.smp_regs[0] as *mut [u8; REG_COUNT];
        let _smp_regs_1 = &mut self.smp_regs[1] as *mut [u8; REG_COUNT];
        let _timers_ptr = &mut self.timers as *mut [Timer; TIMER_COUNT];
        let _dsp = &mut self.dsp as *mut SpcDsp;
        let _reg_times = &self.reg_times;
        let cycle_table = self.cycle_table;

        // Memory read that handles timer counter registers ($FD-$FF) and SMP registers ($F0-$FC)
        // Matches C++ cpu_read behavior for dp addressing
        #[allow(dead_code)]
        #[inline(always)]
        fn read_mem(
            ram: &RamPtr,
            addr: u16,
            rel_time: i32,
            timers_ptr: *mut [Timer; TIMER_COUNT],
            smp_regs_0: *const [u8; REG_COUNT],
            smp_regs_1: *const [u8; REG_COUNT],
            dsp: *mut SpcDsp,
        ) -> u8 {
            if addr >= 0xF0 {
                let reg = (addr - 0xF0) as usize;
                // Timer counters $FD-$FF (must check before SMP regs)
                let ti = (addr as usize).wrapping_sub(0xFD);
                if ti < TIMER_COUNT {
                    let t = unsafe { &mut (*timers_ptr)[ti] };
                    if rel_time >= t.next_time {
                        let elapsed = (rel_time - t.next_time) / t.prescaler + 1;
                        t.next_time += elapsed * t.prescaler;
                        if t.enabled != 0 {
                            let remain = SpcApu::if_0_then_256((t.period - t.divider as i32) as u8);
                            let divider = t.divider as i32 + elapsed;
                            let over = elapsed - remain;
                            if over >= 0 {
                                let n = over / t.period;
                                t.counter = (t.counter + 1 + n) & 0x0F;
                                t.divider = (over - n * t.period) as u8;
                            } else {
                                t.divider = divider as u8;
                            }
                        }
                    }
                    let counter = t.counter as u8;
                    t.counter = 0;
                    return counter;
                }
                // SMP registers $F0-$FC
                if reg < REG_COUNT {
                    if reg == R_DSPDATA {
                        unsafe {
                            let dsp_addr = (*smp_regs_0)[R_DSPADDR];
                            return (*dsp).read(dsp_addr as usize & 0x7F);
                        }
                    }
                    return unsafe { (*smp_regs_1)[reg] };
                }
            }
            ram[addr as usize]
        }

        // Main CPU execution loop
        'cpu_loop: loop {
            let opcode = ram[pc as usize];
            rel_time += cycle_table[opcode as usize] as i32;
            if rel_time > 0 {
                // undo partial execution of opcode (C++ out_of_time)
                rel_time -= cycle_table[opcode as usize] as i32;
                break 'cpu_loop;
            }

            if let Some(tr) = self.cpu_trace.as_mut()
                && tr.len() < 200_000
            {
                // Recompose PSW the same way the epilogue does (C++ GET_PSW)
                let mut hook_psw = psw & !(0x80 | 0x20 | 0x02 | 0x01);
                hook_psw |= ((c >> 8) & 0x01) as u8;
                hook_psw |= ((dp >> 3) & 0x20) as u8;
                hook_psw |= (((nz >> 4) | nz) & 0x80) as u8;
                if (nz as u8) == 0 {
                    hook_psw |= 0x02;
                }
                tr.push((pc, opcode, a, x, y, sp, rel_time, hook_psw));
            }

            pc = pc.wrapping_add(1);
            let data = ram[pc as usize] as i32;

            match opcode {
                // ===== 1. 8-bit Data Transmission, Group I =====
                0xE8 => {
                    // MOV A,imm
                    a = data as u8;
                    nz = data;
                    pc = pc.wrapping_add(1);
                }
                0xE4 => {
                    // MOV A,dp
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                }
                0xE5 => {
                    // MOV A,abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                }
                0xE6 => {
                    // MOV A,(X)
                    let addr = (x as u16).wrapping_add(dp as u16);
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                }
                0xE7 => {
                    // MOV A,(dp+X)
                    let dp_addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let addr = (ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[dp_addr as usize] as u16;
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                    pc = pc.wrapping_add(1);
                }
                0xF4 => {
                    // MOV A,dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    pc = pc.wrapping_add(1);
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                }
                0xF5 => {
                    // MOV A,abs+X
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                }
                0xF6 => {
                    // MOV A,abs+Y
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                }
                0xF7 => {
                    // MOV A,(dp)+Y
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    let val = self.cpu_read(addr, rel_time);
                    a = val;
                    nz = val as i32;
                    pc = pc.wrapping_add(1);
                }
                0xF8 => {
                    // MOV X,dp
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let val = self.cpu_read(addr, rel_time);
                    x = val;
                    nz = val as i32;
                }
                0xF9 => {
                    // MOV X,dp+Y
                    let addr = ((data as u8).wrapping_add(y)) as u16 + dp as u16;
                    let val = self.cpu_read(addr, rel_time);
                    x = val;
                    nz = val as i32;
                    pc = pc.wrapping_add(1);
                }
                0xE9 => {
                    // MOV X,abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(addr, rel_time);
                    x = val;
                    nz = val as i32;
                }
                0xCD => {
                    // MOV X,imm
                    x = data as u8;
                    nz = data;
                    pc = pc.wrapping_add(1);
                }
                0xEB => {
                    // MOV Y,dp
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let val = self.cpu_read(addr, rel_time);
                    y = val;
                    nz = val as i32;
                }
                0xFB => {
                    // MOV Y,dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let val = self.cpu_read(addr, rel_time);
                    y = val;
                    nz = val as i32;
                    pc = pc.wrapping_add(1);
                }
                0xEC => {
                    // MOV Y,abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(addr, rel_time);
                    y = val;
                    nz = val as i32;
                }
                0x8D => {
                    // MOV Y,imm
                    y = data as u8;
                    nz = data;
                    pc = pc.wrapping_add(1);
                }
                0xBF => {
                    // MOV A,(X)+
                    let addr = (x as u16).wrapping_add(dp as u16);
                    x = x.wrapping_add(1);
                    let val = self.cpu_read(addr, rel_time - 1);
                    a = val;
                    nz = val as i32;
                }
                // ===== 2. 8-bit Data Transmission, Group II =====
                0xC4 => {
                    // MOV dp,a
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    self.cpu_write_mem(addr, a as i32, rel_time);
                }
                0xC8 => {
                    // CMP X,imm
                    nz = x as i32 - data;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                }
                // MOV addr,A
                0xC5 => {
                    // MOV abs,A
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    self.cpu_write_mem(addr, a as i32, rel_time);
                }
                0xC6 => {
                    // MOV (X),A
                    let addr = (x as u16).wrapping_add(dp as u16);
                    self.cpu_write_mem(addr, a as i32, rel_time);
                }
                0xD4 => {
                    // MOV dp+X,A
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    self.cpu_write_mem(addr, a as i32, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0xD5 => {
                    // MOV abs+X,A
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    self.cpu_write_mem(addr, a as i32, rel_time);
                }
                0xD6 => {
                    // MOV abs+Y,A
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    self.cpu_write_mem(addr, a as i32, rel_time);
                }
                0xD7 => {
                    // MOV (dp)+Y,A
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[dp_addr.wrapping_add(1) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    self.cpu_write_mem(addr, a as i32, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0xD8 => {
                    // MOV dp,X
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    self.cpu_write_mem(addr, x as i32, rel_time);
                }
                0xD9 => {
                    // MOV dp+Y,X
                    let addr = ((data as u8).wrapping_add(y)) as u16 + dp as u16;
                    self.cpu_write_mem(addr, x as i32, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0xDB => {
                    // MOV dp+X,Y
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    self.cpu_write_mem(addr, y as i32, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0xCB => {
                    // MOV dp,Y
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    self.cpu_write_mem(addr, y as i32, rel_time);
                }
                0xCC => {
                    // MOV abs,Y
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    self.cpu_write_mem(addr, y as i32, rel_time);
                }
                0xC9 => {
                    // MOV abs,X
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    self.cpu_write_mem(addr, x as i32, rel_time);
                }
                0xAF => {
                    // MOV (X)+,A
                    let addr = (x as u16).wrapping_add(dp as u16);
                    self.cpu_write_mem(addr, a as i32 + NO_READ_BEFORE_WRITE, rel_time);
                    x = x.wrapping_add(1);
                }
                // ===== 3. 8-bit Data Transmission, Group III =====
                0x7D => {
                    a = x;
                    nz = x as i32;
                } // MOV A,X
                0xDD => {
                    a = y;
                    nz = y as i32;
                } // MOV A,Y
                0x5D => {
                    x = a;
                    nz = a as i32;
                } // MOV X,A
                0xFD => {
                    y = a;
                    nz = a as i32;
                } // MOV Y,A
                0x9D => {
                    x = sp;
                    nz = sp as i32;
                } // MOV X,SP
                0xBD => {
                    sp = x;
                } // MOV SP,X
                // ===== 5. 8-bit Logic =====
                // AND
                0x28 => {
                    a &= data as u8;
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                } // AND imm
                0x38 => {
                    // AND dp,imm (encoding: 38 imm dp_addr)
                    let dp_addr = ram[(pc.wrapping_add(1)) as usize];
                    pc = pc.wrapping_add(2);
                    let addr = (dp_addr as u16).wrapping_add(dp as u16);
                    nz = data & self.cpu_read(addr, rel_time - 1) as i32;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x29 => {
                    // AND dp,dp
                    let src = (data as u16).wrapping_add(dp as u16);
                    let dst_addr = ram[(pc.wrapping_add(1)) as usize] as u16 + dp as u16;
                    pc = pc.wrapping_add(2);
                    let src_val = self.cpu_read(src, rel_time - 3) as i32;
                    nz = src_val & self.cpu_read(dst_addr, rel_time - 1) as i32;
                    self.cpu_write_mem(dst_addr, nz, rel_time);
                }
                0x39 => {
                    // AND (X),(Y)
                    let src = (y as u16).wrapping_add(dp as u16);
                    let dst = (x as u16).wrapping_add(dp as u16);
                    let src_val = self.cpu_read(src, rel_time - 2) as i32;
                    nz = src_val & self.cpu_read(dst, rel_time - 1) as i32;
                    self.cpu_write_mem(dst, nz, rel_time);
                }
                // OR
                0x08 => {
                    a |= data as u8;
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                } // OR imm
                0x18 => {
                    // OR dp,imm (encoding: 18 imm dp_addr)
                    let dp_addr = ram[(pc.wrapping_add(1)) as usize];
                    pc = pc.wrapping_add(2);
                    let addr = (dp_addr as u16).wrapping_add(dp as u16);
                    nz = data | self.cpu_read(addr, rel_time - 1) as i32;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x09 => {
                    // OR dp,dp
                    let src = (data as u16).wrapping_add(dp as u16);
                    let dst_addr = ram[(pc.wrapping_add(1)) as usize] as u16 + dp as u16;
                    pc = pc.wrapping_add(2);
                    let src_val = self.cpu_read(src, rel_time - 3) as i32;
                    nz = src_val | self.cpu_read(dst_addr, rel_time - 1) as i32;
                    self.cpu_write_mem(dst_addr, nz, rel_time);
                }
                0x19 => {
                    // OR (X),(Y)
                    let src = (y as u16).wrapping_add(dp as u16);
                    let dst = (x as u16).wrapping_add(dp as u16);
                    let src_val = self.cpu_read(src, rel_time - 2) as i32;
                    nz = src_val | self.cpu_read(dst, rel_time - 1) as i32;
                    self.cpu_write_mem(dst, nz, rel_time);
                }
                // EOR
                0x48 => {
                    a ^= data as u8;
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                } // EOR imm
                0x58 => {
                    // EOR dp,imm (encoding: 58 imm dp_addr)
                    let dp_addr = ram[(pc.wrapping_add(1)) as usize];
                    pc = pc.wrapping_add(2);
                    let addr = (dp_addr as u16).wrapping_add(dp as u16);
                    nz = data ^ self.cpu_read(addr, rel_time - 1) as i32;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x49 => {
                    // EOR dp,dp
                    let src = (data as u16).wrapping_add(dp as u16);
                    let dst_addr = ram[(pc.wrapping_add(1)) as usize] as u16 + dp as u16;
                    pc = pc.wrapping_add(2);
                    let src_val = self.cpu_read(src, rel_time - 3) as i32;
                    nz = src_val ^ self.cpu_read(dst_addr, rel_time - 1) as i32;
                    self.cpu_write_mem(dst_addr, nz, rel_time);
                }
                0x59 => {
                    // EOR (X),(Y)
                    let src = (y as u16).wrapping_add(dp as u16);
                    let dst = (x as u16).wrapping_add(dp as u16);
                    let src_val = self.cpu_read(src, rel_time - 2) as i32;
                    nz = src_val ^ self.cpu_read(dst, rel_time - 1) as i32;
                    self.cpu_write_mem(dst, nz, rel_time);
                }
                // ===== 4. 8-bit Arithmetic =====
                // CMP
                0x68 => {
                    nz = a as i32 - data;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                } // CMP imm
                0x78 => {
                    // CMP dp,imm (encoding: 78 imm dp_addr)
                    let dp_addr = ram[(pc.wrapping_add(1)) as usize] as u16;
                    pc = pc.wrapping_add(2);
                    let addr = dp_addr.wrapping_add(dp as u16);
                    nz = self.cpu_read(addr, rel_time - 1) as i32 - data;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x69 => {
                    // CMP dp,dp
                    let src = (data as u16).wrapping_add(dp as u16);
                    let dst_addr = ram[(pc.wrapping_add(1)) as usize] as u16 + dp as u16;
                    pc = pc.wrapping_add(2);
                    let src_val = self.cpu_read(src, rel_time - 3);
                    let dst_val = self.cpu_read(dst_addr, rel_time - 1);
                    nz = dst_val as i32 - src_val as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x79 => {
                    // CMP (X),(Y)
                    let src = (y as u16).wrapping_add(dp as u16);
                    let dst = (x as u16).wrapping_add(dp as u16);
                    let src_val = self.cpu_read(src, rel_time - 2) as i32;
                    nz = self.cpu_read(dst, rel_time - 1) as i32 - src_val;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x3E => {
                    // CMP X,dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    nz = x as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                }
                0x1E => {
                    // CMP X,abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    nz = x as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                0xAD => {
                    nz = y as i32 - data;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                } // CMP Y,imm
                0x7E => {
                    // CMP Y,dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    nz = y as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                }
                0x5E => {
                    // CMP Y,abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    nz = y as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                // ADC/SBC
                0x88 => {
                    // ADC imm
                    let d = data;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                0xA8 => {
                    // SBC imm
                    let d = data ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                0x98 => {
                    // ADC dp,imm (encoding: 98 imm dp_addr)
                    let dp_addr = ram[(pc.wrapping_add(1)) as usize] as u16;
                    pc = pc.wrapping_add(2);
                    let addr = dp_addr.wrapping_add(dp as u16);
                    let d = self.cpu_read(addr, rel_time - 1) as i32;
                    let flags = d ^ data;
                    nz = d + data + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0xB8 => {
                    // SBC dp,imm (encoding: B8 imm dp_addr)
                    let dp_addr = ram[(pc.wrapping_add(1)) as usize] as u16;
                    pc = pc.wrapping_add(2);
                    let addr = dp_addr.wrapping_add(dp as u16);
                    let d = self.cpu_read(addr, rel_time - 1) as i32;
                    let inv = data ^ 0xFF;
                    let flags = d ^ inv;
                    nz = d + inv + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x89 => {
                    // ADC dp,dp
                    let src = (data as u16).wrapping_add(dp as u16);
                    let dst_addr = ram[(pc.wrapping_add(1)) as usize] as u16 + dp as u16;
                    pc = pc.wrapping_add(2);
                    let d = self.cpu_read(src, rel_time - 3) as i32;
                    let nd = self.cpu_read(dst_addr, rel_time - 1) as i32;
                    let flags = d ^ nd;
                    nz = nd + d + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    self.cpu_write_mem(dst_addr, nz, rel_time);
                }
                0xA9 => {
                    // SBC dp,dp
                    let src = (data as u16).wrapping_add(dp as u16);
                    let dst_addr = ram[(pc.wrapping_add(1)) as usize] as u16 + dp as u16;
                    pc = pc.wrapping_add(2);
                    let d = self.cpu_read(src, rel_time - 3) as i32;
                    let nd = self.cpu_read(dst_addr, rel_time - 1) as i32;
                    let inv = d ^ 0xFF;
                    let flags = nd ^ inv;
                    nz = nd + inv + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    self.cpu_write_mem(dst_addr, nz, rel_time);
                }
                0x99 => {
                    // ADC (X),(Y)
                    let src = (y as u16).wrapping_add(dp as u16);
                    let dst = (x as u16).wrapping_add(dp as u16);
                    let d = self.cpu_read(src, rel_time - 2) as i32;
                    let nd = self.cpu_read(dst, rel_time - 1) as i32;
                    let flags = d ^ nd;
                    nz = nd + d + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    self.cpu_write_mem(dst, nz, rel_time);
                }
                0xB9 => {
                    // SBC (X),(Y)
                    let src = (y as u16).wrapping_add(dp as u16);
                    let dst = (x as u16).wrapping_add(dp as u16);
                    let d = self.cpu_read(src, rel_time - 2) as i32;
                    let nd = self.cpu_read(dst, rel_time - 1) as i32;
                    let inv = d ^ 0xFF;
                    let flags = nd ^ inv;
                    nz = nd + inv + ((c >> 8) & 1);
                    let flags2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags2 >> 1) as u8 & 0x08)
                        | (((flags2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    self.cpu_write_mem(dst, nz, rel_time);
                }
                // ===== 6. INC/DEC =====
                0xBC => {
                    a = a.wrapping_add(1);
                    nz = a as i32;
                } // INC A
                0x3D => {
                    x = x.wrapping_add(1);
                    nz = x as i32;
                } // INC X
                0xFC => {
                    // INC Y
                    y = y.wrapping_add(1);
                    nz = y as i32;
                }
                0x9C => {
                    a = a.wrapping_sub(1);
                    nz = a as i32;
                } // DEC A
                0x1D => {
                    x = x.wrapping_sub(1);
                    nz = x as i32;
                } // DEC X
                0xDC => {
                    y = y.wrapping_sub(1);
                    nz = y as i32;
                } // DEC Y
                0xAB => {
                    // INC dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    nz = self.cpu_read(addr, rel_time - 1) as i32 + 1;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x8B => {
                    // DEC dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    nz = self.cpu_read(addr, rel_time - 1) as i32 - 1;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0xAC => {
                    // INC abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    nz = self.cpu_read(addr, rel_time - 1) as i32 + 1;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x8C => {
                    // DEC abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    nz = self.cpu_read(addr, rel_time - 1) as i32 - 1;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0xBB => {
                    // INC dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    nz = self.cpu_read(addr, rel_time - 1) as i32 + 1;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x9B => {
                    // DEC dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    nz = self.cpu_read(addr, rel_time - 1) as i32 - 1;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                // ===== 7. Shift/Rotate =====
                0x1C => {
                    // ASL A
                    c = (a as i32) << 1;
                    nz = c;
                    a = nz as u8;
                }
                0x5C => {
                    // LSR A
                    c = (a as i32) << 8;
                    nz = (a as i32) >> 1;
                    a = nz as u8;
                }
                0x3C => {
                    // ROL A
                    let t = (c >> 8) & 1;
                    c = (a as i32) << 1;
                    nz = c | t;
                    a = nz as u8;
                }
                0x7C => {
                    // ROR A
                    let t = (c >> 1) & 0x80;
                    nz = t | ((a as i32) >> 1);
                    c = (a as i32) << 8;
                    a = nz as u8;
                }
                0x0B => {
                    // ASL dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    c = (self.cpu_read(addr, rel_time - 1) as i32) << 1;
                    nz = c;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x4B => {
                    // LSR dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let temp = self.cpu_read(addr, rel_time - 1) as i32;
                    nz = temp >> 1;
                    c = temp << 8;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x2B => {
                    // ROL dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let t = (c >> 8) & 1;
                    c = (self.cpu_read(addr, rel_time - 1) as i32) << 1;
                    nz = c | t;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x6B => {
                    // ROR dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let temp = self.cpu_read(addr, rel_time - 1) as i32;
                    let t = (c >> 1) & 0x80;
                    nz = t | (temp >> 1);
                    c = temp << 8;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x0C => {
                    // ASL abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    c = (self.cpu_read(addr, rel_time - 1) as i32) << 1;
                    nz = c;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x4C => {
                    // LSR abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let temp = self.cpu_read(addr, rel_time - 1) as i32;
                    nz = temp >> 1;
                    c = temp << 8;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x2C => {
                    // ROL abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let t = (c >> 8) & 1;
                    c = (self.cpu_read(addr, rel_time - 1) as i32) << 1;
                    nz = c | t;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x6C => {
                    // ROR abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let temp = self.cpu_read(addr, rel_time - 1) as i32;
                    let t = (c >> 1) & 0x80;
                    nz = t | (temp >> 1);
                    c = temp << 8;
                    self.cpu_write_mem(addr, nz, rel_time);
                }
                0x1B => {
                    // ASL dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    c = (self.cpu_read(addr, rel_time - 1) as i32) << 1;
                    nz = c;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x5B => {
                    // LSR dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let temp = self.cpu_read(addr, rel_time - 1) as i32;
                    nz = temp >> 1;
                    c = temp << 8;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x3B => {
                    // ROL dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let t = (c >> 8) & 1;
                    c = (self.cpu_read(addr, rel_time - 1) as i32) << 1;
                    nz = c | t;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x7B => {
                    // ROR dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let temp = self.cpu_read(addr, rel_time - 1) as i32;
                    let t = (c >> 1) & 0x80;
                    nz = t | (temp >> 1);
                    c = temp << 8;
                    self.cpu_write_mem(addr, nz, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x9F => {
                    a = (a >> 4) | (a.wrapping_shl(4));
                    nz = a as i32;
                } // XCN
                // ===== 8. 16-bit Transmission =====
                0xBA => {
                    // MOVW YA,dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    a = self.cpu_read(addr, rel_time - 2);
                    nz = ((a as i32) & 0x7F) | ((a as i32) >> 1);
                    let addr2 = ((data as u8).wrapping_add(1)) as u16 + dp as u16;
                    y = self.cpu_read(addr2, rel_time);
                    nz |= y as i32;
                    pc = pc.wrapping_add(1);
                }
                0xDA => {
                    // MOVW dp,YA
                    let addr = (data as u16).wrapping_add(dp as u16);
                    self.cpu_write_mem(addr, a as i32, rel_time - 1);
                    let addr2 = ((data as u8).wrapping_add(1)) as u16 + dp as u16;
                    self.cpu_write_mem(addr2, y as i32 + NO_READ_BEFORE_WRITE, rel_time);
                    pc = pc.wrapping_add(1);
                }
                // ===== 9. 16-bit Operations =====
                0x3A => {
                    // INCW dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let mut temp = self.cpu_read(addr, rel_time - 3) as i32 + 1;
                    nz = ((temp >> 1) | temp) & 0x7F;
                    self.cpu_write_mem(addr, temp, rel_time - 2);
                    let addr2 = ((data as u8).wrapping_add(1)) as u16 + dp as u16;
                    temp = ((temp >> 8) + self.cpu_read(addr2, rel_time - 1) as i32) as u8 as i32;
                    nz |= temp;
                    self.cpu_write_mem(addr2, temp, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x1A => {
                    // DECW dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let mut temp = self.cpu_read(addr, rel_time - 3) as i32 - 1;
                    nz = ((temp >> 1) | temp) & 0x7F;
                    self.cpu_write_mem(addr, temp, rel_time - 2);
                    let addr2 = ((data as u8).wrapping_add(1)) as u16 + dp as u16;
                    temp = ((temp >> 8) + self.cpu_read(addr2, rel_time - 1) as i32) as u8 as i32;
                    nz |= temp;
                    self.cpu_write_mem(addr2, temp, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x7A => {
                    // ADDW YA,dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let lo = self.cpu_read(addr, rel_time - 2) as i32;
                    let addr2 = ((data as u8).wrapping_add(1)) as u16 + dp as u16;
                    let hi = self.cpu_read(addr2, rel_time) as i32;
                    let lo_r = lo + a as i32;
                    let result = y as i32 + hi + (lo_r >> 8);
                    let flags = hi ^ y as i32 ^ result;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags >> 1) as u8 & 0x08)
                        | (((flags + 0x80) >> 2) as u8 & 0x40);
                    c = result;
                    a = lo_r as u8;
                    y = result as u8;
                    nz = (((lo_r >> 1) | lo_r) & 0x7F) | (result & 0xFF);
                    pc = pc.wrapping_add(1);
                }
                0x9A => {
                    // SUBW YA,dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let lo = ((self.cpu_read(addr, rel_time - 2) as i32) ^ 0xFF) + 1;
                    let addr2 = ((data as u8).wrapping_add(1)) as u16 + dp as u16;
                    let hi = (self.cpu_read(addr2, rel_time) as i32) ^ 0xFF;
                    let lo_r = lo + a as i32;
                    let result = y as i32 + hi + (lo_r >> 8);
                    let flags = hi ^ y as i32 ^ result;
                    psw = (psw & !(0x40 | 0x08))
                        | ((flags >> 1) as u8 & 0x08)
                        | (((flags + 0x80) >> 2) as u8 & 0x40);
                    c = result;
                    a = lo_r as u8;
                    y = result as u8;
                    nz = (((lo_r >> 1) | lo_r) & 0x7F) | (result & 0xFF);
                    pc = pc.wrapping_add(1);
                }
                0x5A => {
                    // CMPW YA,dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let mut temp = a as i32 - self.cpu_read(addr, rel_time - 1) as i32;
                    nz = ((temp >> 1) | temp) & 0x7F;
                    let addr2 = ((data as u8).wrapping_add(1)) as u16 + dp as u16;
                    temp = y as i32 + (temp >> 8);
                    temp -= self.cpu_read(addr2, rel_time) as i32;
                    nz |= temp;
                    c = !temp;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                }
                // ===== 10. Multiply/Divide =====
                0xCF => {
                    // MUL YA
                    let temp = y as u32 * a as u32;
                    a = temp as u8;
                    nz = ((temp >> 1) | temp) as i32 & 0x7F;
                    y = (temp >> 8) as u8;
                    nz |= y as i32;
                }
                0x9E => {
                    // DIV YA,X
                    let ya = y as u32 * 0x100 + a as u32;
                    psw &= !(0x08 | 0x40);
                    if y >= x {
                        psw |= 0x40;
                    }
                    if (y & 15) >= (x & 15) {
                        psw |= 0x08;
                    }
                    if (y as u32) < x as u32 * 2 {
                        let q = ya / x as u32;
                        y = (ya - q * x as u32) as u8;
                        a = q as u8;
                        nz = q as u8 as i32;
                    } else {
                        let q = 255u32.wrapping_sub((ya - x as u32 * 0x200) / (256 - x as u32));
                        y = (x as u32 + (ya - x as u32 * 0x200) % (256 - x as u32)) as u8;
                        a = q as u8;
                        nz = q as u8 as i32;
                    }
                }
                // ===== 11. Decimal Adjust =====
                0xDF => {
                    // DAA
                    if a > 0x99 || c & 0x100 != 0 {
                        a = a.wrapping_add(0x60);
                        c = 0x100;
                    }
                    if (a & 0x0F) > 9 || psw & 0x08 != 0 {
                        a = a.wrapping_add(0x06);
                    }
                    nz = a as i32;
                }
                0xBE => {
                    // DAS
                    if a > 0x99 || c & 0x100 == 0 {
                        a = a.wrapping_sub(0x60);
                        c = 0;
                    }
                    if (a & 0x0F) > 9 || psw & 0x08 == 0 {
                        a = a.wrapping_sub(0x06);
                    }
                    nz = a as i32;
                }
                // ===== 12. Branching =====
                0x2F => {
                    // BRA rel
                    pc = pc.wrapping_add(1).wrapping_add(data as i8 as u16);
                }
                0xF0 => {
                    // BEQ
                    pc = pc.wrapping_add(1);
                    if (nz as u8) == 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0xD0 => {
                    // BNE
                    pc = pc.wrapping_add(1);
                    if (nz as u8) != 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0x30 => {
                    // BMI (nz_neg_mask: either bit indicates N set)
                    pc = pc.wrapping_add(1);
                    if nz & 0x880 != 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0x10 => {
                    // BPL (nz_neg_mask)
                    pc = pc.wrapping_add(1);
                    if nz & 0x880 == 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0xB0 => {
                    // BCS
                    pc = pc.wrapping_add(1);
                    if c & 0x100 != 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0x90 => {
                    // BCC
                    pc = pc.wrapping_add(1);
                    if c & 0x100 == 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0x70 => {
                    // BVS
                    pc = pc.wrapping_add(1);
                    if psw & 0x40 != 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0x50 => {
                    // BVC
                    pc = pc.wrapping_add(1);
                    if psw & 0x40 == 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                // BBS/BBC
                0x03 | 0x23 | 0x43 | 0x63 | 0x83 | 0xA3 | 0xC3 | 0xE3 => {
                    // BBS
                    let bit = 1 << (opcode >> 5);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    pc = pc.wrapping_add(1);
                    if self.cpu_read(addr, rel_time - 4) & bit != 0 {
                        pc = pc.wrapping_add(ram[pc as usize] as i8 as u16);
                        pc = pc.wrapping_add(1);
                    } else {
                        rel_time -= 2;
                        pc = pc.wrapping_add(1);
                    }
                }
                0x13 | 0x33 | 0x53 | 0x73 | 0x93 | 0xB3 | 0xD3 | 0xF3 => {
                    // BBC
                    let bit = 1 << (opcode >> 5);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    pc = pc.wrapping_add(1);
                    if self.cpu_read(addr, rel_time - 4) & bit == 0 {
                        pc = pc.wrapping_add(ram[pc as usize] as i8 as u16);
                        pc = pc.wrapping_add(1);
                    } else {
                        rel_time -= 2;
                        pc = pc.wrapping_add(1);
                    }
                }
                0x2E => {
                    // CBNE dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    pc = pc.wrapping_add(1);
                    if self.cpu_read(addr, rel_time - 4) != a {
                        pc = pc.wrapping_add(ram[pc as usize] as i8 as u16);
                        pc = pc.wrapping_add(1);
                    } else {
                        rel_time -= 2;
                        pc = pc.wrapping_add(1);
                    }
                }
                0xDE => {
                    // CBNE dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    pc = pc.wrapping_add(1);
                    if self.cpu_read(addr, rel_time - 4) != a {
                        pc = pc.wrapping_add(ram[pc as usize] as i8 as u16);
                        pc = pc.wrapping_add(1);
                    } else {
                        rel_time -= 2;
                        pc = pc.wrapping_add(1);
                    }
                }
                0x6E => {
                    // DBNZ dp
                    let addr = (data as u16).wrapping_add(dp as u16);
                    pc = pc.wrapping_add(1);
                    let val = self.cpu_read(addr, rel_time - 4).wrapping_sub(1);
                    self.cpu_write_mem(addr, val as i32 + NO_READ_BEFORE_WRITE, rel_time - 3);
                    if val != 0 {
                        pc = pc.wrapping_add(ram[pc as usize] as i8 as u16);
                        pc = pc.wrapping_add(1);
                    } else {
                        rel_time -= 2;
                        pc = pc.wrapping_add(1);
                    }
                }
                0xFE => {
                    // DBNZ Y
                    pc = pc.wrapping_add(1);
                    y = y.wrapping_sub(1);
                    if y != 0 {
                        pc = pc.wrapping_add(data as i8 as u16);
                    } else {
                        rel_time -= 2;
                    }
                }
                0x5F => {
                    // JMP abs
                    pc = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                }
                0x1F => {
                    // JMP [abs+X]
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100
                        + data as u16
                        + x as u16;
                    pc = (ram[(addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[addr as usize] as u16;
                }
                // ===== 13. Subroutine =====
                0x3F => {
                    // CALL
                    let ret = pc.wrapping_add(2);
                    pc = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    ram[0x100 + sp as usize] = (ret >> 8) as u8;
                    sp = sp.wrapping_sub(1);
                    ram[0x100 + sp as usize] = ret as u8;
                    sp = sp.wrapping_sub(1);
                }
                0x6F => {
                    // RET
                    sp = sp.wrapping_add(1);
                    let lo = ram[0x100 + sp as usize] as u16;
                    sp = sp.wrapping_add(1);
                    let hi = ram[0x100 + sp as usize] as u16;
                    pc = hi * 0x100 + lo;
                }
                0x4F => {
                    // PCALL
                    let ret = pc.wrapping_add(1);
                    pc = 0xFF00 | data as u16;
                    ram[0x100 + sp as usize] = (ret >> 8) as u8;
                    sp = sp.wrapping_sub(1);
                    ram[0x100 + sp as usize] = ret as u8;
                    sp = sp.wrapping_sub(1);
                }
                0x01 | 0x11 | 0x21 | 0x31 | 0x41 | 0x51 | 0x61 | 0x71 | 0x81 | 0x91 | 0xA1
                | 0xB1 | 0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                    // TCALL n
                    let ret = pc;
                    let vec = 0xFFDE - ((opcode as u16) >> 3);
                    pc = (ram[(vec.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[vec as usize] as u16;
                    ram[0x100 + sp as usize] = (ret >> 8) as u8;
                    sp = sp.wrapping_sub(1);
                    ram[0x100 + sp as usize] = ret as u8;
                    sp = sp.wrapping_sub(1);
                }
                0x0F => {
                    // BRK
                    let ret = pc;
                    pc = (ram[0xFFDF] as u16) * 0x100 + ram[0xFFDE] as u16;
                    ram[0x100 + sp as usize] = (ret >> 8) as u8;
                    sp = sp.wrapping_sub(1);
                    ram[0x100 + sp as usize] = ret as u8;
                    sp = sp.wrapping_sub(1);
                    let temp_psw;
                    {
                        let mut out = psw & !(0x80 | 0x20 | 0x02 | 0x01);
                        out |= ((c >> 8) & 0x01) as u8;
                        out |= ((dp >> 3) & 0x20) as u8;
                        out |= (((nz >> 4) | nz) & 0x80) as u8;
                        if (nz as u8) == 0 {
                            out |= 0x02;
                        }
                        temp_psw = out;
                    }
                    ram[0x100 + sp as usize] = temp_psw;
                    sp = sp.wrapping_sub(1);
                    psw = (psw | 0x10) & !0x04;
                }
                // ===== 14. Stack Operations =====
                0x7F => {
                    // RET1
                    sp = sp.wrapping_add(1);
                    let temp_psw = ram[0x100 + sp as usize];
                    sp = sp.wrapping_add(1);
                    let lo = ram[0x100 + sp as usize] as u16;
                    sp = sp.wrapping_add(1);
                    let hi = ram[0x100 + sp as usize] as u16;
                    pc = hi * 0x100 + lo;
                    // SET_PSW
                    psw = temp_psw;
                    c = (temp_psw as i32) << 8;
                    dp = (temp_psw as i32) << 3 & 0x100;
                    nz = ((temp_psw as i32) << 4 & 0x800) | (!(temp_psw as i32) & 0x02);
                }
                0x8E => {
                    // POP PSW
                    sp = sp.wrapping_add(1);
                    let temp_psw = ram[0x100 + sp as usize];
                    psw = temp_psw;
                    c = (temp_psw as i32) << 8;
                    dp = (temp_psw as i32) << 3 & 0x100;
                    nz = ((temp_psw as i32) << 4 & 0x800) | (!(temp_psw as i32) & 0x02);
                }
                0x0D => {
                    // PUSH PSW
                    let temp_psw;
                    {
                        let mut out = psw & !(0x80 | 0x20 | 0x02 | 0x01);
                        out |= ((c >> 8) & 0x01) as u8;
                        out |= ((dp >> 3) & 0x20) as u8;
                        out |= (((nz >> 4) | nz) & 0x80) as u8;
                        if (nz as u8) == 0 {
                            out |= 0x02;
                        }
                        temp_psw = out;
                    }
                    ram[0x100 + sp as usize] = temp_psw;
                    sp = sp.wrapping_sub(1);
                }
                0x2D => {
                    ram[0x100 + sp as usize] = a;
                    sp = sp.wrapping_sub(1);
                } // PUSH A
                0x4D => {
                    ram[0x100 + sp as usize] = x;
                    sp = sp.wrapping_sub(1);
                } // PUSH X
                0x6D => {
                    ram[0x100 + sp as usize] = y;
                    sp = sp.wrapping_sub(1);
                } // PUSH Y
                0xAE => {
                    sp = sp.wrapping_add(1);
                    a = ram[0x100 + sp as usize];
                } // POP A
                0xCE => {
                    sp = sp.wrapping_add(1);
                    x = ram[0x100 + sp as usize];
                } // POP X
                0xEE => {
                    sp = sp.wrapping_add(1);
                    y = ram[0x100 + sp as usize];
                } // POP Y
                // ===== 15. Bit Operations =====
                0x02 | 0x22 | 0x42 | 0x62 | 0x82 | 0xA2 | 0xC2 | 0xE2 | 0x12 | 0x32 | 0x52
                | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2 => {
                    // SET1 / CLR1: 2-byte read-modify-write through the
                    // register-aware handlers.
                    // C++: WRITE( 0, data, (READ( -1, data ) & mask) | bit )
                    let bit_full: u8 = 1 << (opcode >> 5);
                    let mask = !bit_full;
                    let bit = if opcode & 0x10 != 0 { 0 } else { bit_full };
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let val = (self.cpu_read(addr, rel_time - 1) & mask) | bit;
                    self.cpu_write_mem(addr, val as i32, rel_time);
                    pc = pc.wrapping_add(1);
                }
                0x0E => {
                    // TSET1 abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(addr, rel_time - 2);
                    nz = a.wrapping_sub(val) as i32;
                    self.cpu_write_mem(addr, (val | a) as i32, rel_time);
                }
                0x4E => {
                    // TCLR1 abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(addr, rel_time - 2);
                    nz = a.wrapping_sub(val) as i32;
                    self.cpu_write_mem(addr, (val & !a) as i32, rel_time);
                }
                // AND1/OR1/EOR1/NOT1/MOV1
                0x4A => {
                    // AND1 C,mem.bit
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = addr >> 13;
                    let t = ((self.cpu_read(addr & 0x1FFF, rel_time) as i32) >> bit) << 8 & 0x100;
                    c &= t;
                }
                0x6A => {
                    // AND1 C,/mem.bit
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = addr >> 13;
                    let t = ((self.cpu_read(addr & 0x1FFF, rel_time) as i32) >> bit) << 8 & 0x100;
                    c &= !t;
                }
                0x0A => {
                    // OR1 C,mem.bit
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = addr >> 13;
                    let t =
                        ((self.cpu_read(addr & 0x1FFF, rel_time - 1) as i32) >> bit) << 8 & 0x100;
                    c |= t;
                }
                0x2A => {
                    // OR1 C,/mem.bit
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = addr >> 13;
                    let t =
                        ((self.cpu_read(addr & 0x1FFF, rel_time - 1) as i32) >> bit) << 8 & 0x100;
                    c |= !t;
                }
                0x8A => {
                    // EOR1 C,mem.bit
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = addr >> 13;
                    let t =
                        ((self.cpu_read(addr & 0x1FFF, rel_time - 1) as i32) >> bit) << 8 & 0x100;
                    c ^= t;
                }
                0xEA => {
                    // NOT1 mem.bit
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = 1 << (addr >> 13);
                    let temp = self.cpu_read(addr & 0x1FFF, rel_time - 1) as i32 ^ bit;
                    self.cpu_write_mem(addr & 0x1FFF, temp, rel_time);
                }
                0xCA => {
                    // MOV1 mem.bit,C
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = addr >> 13;
                    let mask: i32 = !(1 << bit);
                    let mut temp = self.cpu_read(addr & 0x1FFF, rel_time - 2) as i32 & mask;
                    temp |= ((c >> 8) & 1) << bit;
                    self.cpu_write_mem(addr & 0x1FFF, temp + NO_READ_BEFORE_WRITE, rel_time);
                }
                0xAA => {
                    // MOV1 C,mem.bit
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let bit = addr >> 13;
                    c = ((self.cpu_read(addr & 0x1FFF, rel_time) as i32) >> bit) << 8 & 0x100;
                }
                // ===== 16. PSW Flag Operations =====
                0x60 => {
                    c = 0;
                } // CLRC
                0x80 => {
                    c = !0;
                } // SETC
                0xED => {
                    c ^= 0x100;
                } // NOTC
                0xE0 => {
                    psw &= !(0x40 | 0x08);
                } // CLRV
                0x20 => {
                    dp = 0;
                } // CLRP
                0x40 => {
                    dp = 0x100;
                } // SETP
                0xA0 => {
                    psw |= 0x04;
                } // EI
                0xC0 => {
                    psw &= !0x04;
                } // DI
                // ===== 17. Other =====
                0x00 => {} // NOP
                0xFF | 0xEF => {
                    // STOP / SLEEP
                    pc = pc.wrapping_sub(1);
                    rel_time = 0;
                    self.cpu_error = Some("SPC emulation error");
                    break 'cpu_loop;
                }
                // ===== MOV dp,#imm =====
                // SPC700 format: 8F imm dp (value first, then dp address)
                0x8F => {
                    let dp_addr = ram[(pc.wrapping_add(1)) as usize];
                    pc = pc.wrapping_add(2);
                    let addr = (dp_addr as u16).wrapping_add(dp as u16);
                    ram[addr as usize] = data as u8;
                    if (0xF0..0x100).contains(&addr) {
                        self.cpu_write_reg((addr - 0xF0) as usize, data, rel_time);
                    }
                }
                // ===== MOV dp,dp =====
                0xFA => {
                    let src_addr = (data as u16).wrapping_add(dp as u16);
                    let dst_data = ram[(pc.wrapping_add(1)) as usize];
                    pc = pc.wrapping_add(2);
                    let val = self.cpu_read(src_addr, rel_time - 2) as i32 + NO_READ_BEFORE_WRITE;
                    let dst_addr = (dst_data as u16).wrapping_add(dp as u16);
                    ram[dst_addr as usize] = val as u8;
                    if (0xF0..0x100).contains(&dst_addr) {
                        self.cpu_write_reg((dst_addr - 0xF0) as usize, val, rel_time);
                    }
                }
                // Remaining addressing mode variants for AND/OR/EOR
                // AND dp
                0x24 => {
                    // AND A,dp
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x34 => {
                    // AND A,dp+X
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                0x25 => {
                    // AND A,abs
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x35 => {
                    // AND A,abs+X
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x36 => {
                    // AND A,abs+Y
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x26 => {
                    // AND A,(X)
                    let addr = (x as u16).wrapping_add(dp as u16);
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x37 => {
                    // AND A,(dp)+Y
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                0x27 => {
                    // AND A,(dp+X)
                    let dp_addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let addr = (ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[dp_addr as usize] as u16;
                    a &= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                // OR A
                0x04 => {
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    a |= ram[addr as usize];
                    nz = a as i32;
                }
                0x14 => {
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    a |= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                0x05 => {
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    a |= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x15 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    a |= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x16 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    a |= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x06 => {
                    let addr = (x as u16).wrapping_add(dp as u16);
                    a |= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x17 => {
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    a |= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                0x07 => {
                    let dp_addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let addr = (ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[dp_addr as usize] as u16;
                    a |= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                // EOR A
                0x44 => {
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x54 => {
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                0x45 => {
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x55 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x56 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x46 => {
                    let addr = (x as u16).wrapping_add(dp as u16);
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                }
                0x57 => {
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                0x47 => {
                    let dp_addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let addr = (ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[dp_addr as usize] as u16;
                    a ^= self.cpu_read(addr, rel_time);
                    nz = a as i32;
                    pc = pc.wrapping_add(1);
                }
                // CMP A
                0x64 => {
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x74 => {
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                }
                0x65 => {
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x75 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x76 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x66 => {
                    let addr = (x as u16).wrapping_add(dp as u16);
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                }
                0x77 => {
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                }
                0x67 => {
                    let dp_addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let addr = (ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[dp_addr as usize] as u16;
                    nz = a as i32 - self.cpu_read(addr, rel_time) as i32;
                    c = !nz;
                    nz &= 0xFF;
                    pc = pc.wrapping_add(1);
                }
                // ADC A
                0x84 => {
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0x94 => {
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                0x85 => {
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0x95 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0x96 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0x86 => {
                    let addr = (x as u16).wrapping_add(dp as u16);
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0x97 => {
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                0x87 => {
                    let dp_addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let addr = (ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[dp_addr as usize] as u16;
                    let d = self.cpu_read(addr, rel_time) as i32;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                // SBC A
                0xA4 => {
                    pc = pc.wrapping_add(1);
                    let addr = (data as u16).wrapping_add(dp as u16);
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0xB4 => {
                    let addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                0xA5 => {
                    let addr = (ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100 + data as u16;
                    pc = pc.wrapping_add(2);
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0xB5 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(x as u16);
                    pc = pc.wrapping_add(2);
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0xB6 => {
                    let addr = ((ram[(pc.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(data as u16)
                        .wrapping_add(y as u16);
                    pc = pc.wrapping_add(2);
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0xA6 => {
                    let addr = (x as u16).wrapping_add(dp as u16);
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                }
                0xB7 => {
                    let dp_addr = (data as u16).wrapping_add(dp as u16);
                    let addr = ((ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100)
                        .wrapping_add(ram[dp_addr as usize] as u16)
                        .wrapping_add(y as u16);
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                0xA7 => {
                    let dp_addr = ((data as u8).wrapping_add(x)) as u16 + dp as u16;
                    let addr = (ram[(dp_addr.wrapping_add(1)) as usize] as u16) * 0x100
                        + ram[dp_addr as usize] as u16;
                    let d = (self.cpu_read(addr, rel_time) as i32) ^ 0xFF;
                    let flags = d ^ a as i32;
                    nz = a as i32 + d + ((c >> 8) & 1);
                    let f2 = flags ^ nz;
                    psw = (psw & !(0x40 | 0x08))
                        | ((f2 >> 1) as u8 & 0x08)
                        | (((f2 + 0x80) >> 2) as u8 & 0x40);
                    c = nz;
                    a = nz as u8;
                    pc = pc.wrapping_add(1);
                }
                _ => {
                    // Unhandled opcode — treat as NOP
                }
            }
        }

        // Uncache registers
        {
            let temp_psw;
            {
                let mut out = psw & !(0x80 | 0x20 | 0x02 | 0x01);
                out |= ((c >> 8) & 0x01) as u8;
                out |= ((dp >> 3) & 0x20) as u8;
                out |= (((nz >> 4) | nz) & 0x80) as u8;
                if (nz as u8) == 0 {
                    out |= 0x02;
                }
                temp_psw = out;
            }
            self.cpu_regs.psw = temp_psw;
        }
        self.cpu_regs.a = a;
        self.cpu_regs.x = x;
        self.cpu_regs.y = y;
        self.cpu_regs.pc = pc;
        self.cpu_regs.sp = sp;

        self.spc_time += rel_time;
        self.dsp_time -= rel_time;
        self.timers[0].next_time -= rel_time;
        self.timers[1].next_time -= rel_time;
        self.timers[2].next_time -= rel_time;

        &self.smp_regs[0][R_CPUIO0] as *const u8
    }
}
