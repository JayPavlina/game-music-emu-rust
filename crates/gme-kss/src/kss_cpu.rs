// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Kss_Cpu.cpp (2026-06-09 to 2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// Z80 CPU emulator for KSS format.
/// Port of C++ Kss_Cpu.h / Kss_Cpu.cpp.
use gme_core::blip_buffer::BlipTime;

pub type CpuTime = BlipTime;

const PAGE_SHIFT: usize = 13;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT; // 0x2000
const PAGE_COUNT: usize = 0x10000 >> PAGE_SHIFT; // 8
const CPU_PADDING: usize = 0x100;

pub const IDLE_ADDR: u32 = 0xFFFF;

// Flag constants (Z80 flags at bits 7,6,4,2,1,0)
const S_FLAG: u8 = 0x80;
const Z_FLAG: u8 = 0x40;
const H_FLAG: u8 = 0x10;
const P_FLAG: u8 = 0x04; // also V flag
const N_FLAG: u8 = 0x02;
const C_FLAG: u8 = 0x01;

/// Z80 registers.
pub struct Registers {
    pub pc: u16,
    pub sp: u16,
    pub ix: u16,
    pub iy: u16,
    /// Main register set: c,b,e,d,l,h,a,flags (matching C++ little-endian layout)
    pub bregs: [u8; 8],
    /// Alternate register set
    pub alt_bregs: [u8; 8],
    pub iff1: u8,
    pub iff2: u8,
    pub r: u8,
    pub i: u8,
    pub im: u8,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    pub fn new() -> Self {
        Self {
            pc: 0,
            sp: 0,
            ix: 0,
            iy: 0,
            bregs: [0; 8],
            alt_bregs: [0; 8],
            iff1: 0,
            iff2: 0,
            r: 0,
            i: 0,
            im: 0,
        }
    }

    // Register pair accessors (little-endian: low byte first)
    pub fn bc(&self) -> u16 {
        u16::from_le_bytes([self.bregs[0], self.bregs[1]])
    }
    pub fn de(&self) -> u16 {
        u16::from_le_bytes([self.bregs[2], self.bregs[3]])
    }
    pub fn hl(&self) -> u16 {
        u16::from_le_bytes([self.bregs[4], self.bregs[5]])
    }
    pub fn fa(&self) -> u16 {
        u16::from_le_bytes([self.bregs[6], self.bregs[7]])
    }

    pub fn set_bc(&mut self, v: u16) {
        let b = v.to_le_bytes();
        self.bregs[0] = b[0];
        self.bregs[1] = b[1];
    }
    pub fn set_de(&mut self, v: u16) {
        let b = v.to_le_bytes();
        self.bregs[2] = b[0];
        self.bregs[3] = b[1];
    }
    pub fn set_hl(&mut self, v: u16) {
        let b = v.to_le_bytes();
        self.bregs[4] = b[0];
        self.bregs[5] = b[1];
    }
}

struct State {
    read: [*const u8; PAGE_COUNT + 1],
    write: [*mut u8; PAGE_COUNT + 1],
    base: CpuTime,
    time: CpuTime,
}

pub struct KssCpu {
    pub r: Registers,
    state: State,
    end_time: CpuTime,
    szpc: [u8; 0x200],
    #[allow(dead_code)]
    unmapped_read: [u8; CPU_PADDING],
    #[allow(dead_code)]
    unmapped_write: [u8; PAGE_SIZE + 1],
}

// SAFETY: KssCpu only accesses memory through pointers owned by the emulator struct.
unsafe impl Send for KssCpu {}

impl Default for KssCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl KssCpu {
    pub fn new() -> Self {
        let mut cpu = Self {
            r: Registers::new(),
            state: State {
                read: [std::ptr::null(); PAGE_COUNT + 1],
                write: [std::ptr::null_mut(); PAGE_COUNT + 1],
                base: 0,
                time: 0,
            },
            end_time: 0,
            szpc: [0; 0x200],
            unmapped_read: [0xFF; CPU_PADDING],
            unmapped_write: [0; PAGE_SIZE + 1],
        };

        // Build SZPC table
        for i in 0..0x100usize {
            let mut even = 1;
            let mut p = i;
            while p != 0 {
                even ^= p;
                p >>= 1;
            }
            let n =
                (i & (S_FLAG as usize | H_FLAG as usize | 0x08)) | ((even & 1) * P_FLAG as usize);
            cpu.szpc[i] = n as u8;
            cpu.szpc[i + 0x100] = n as u8 | C_FLAG;
        }
        cpu.szpc[0x000] |= Z_FLAG;
        cpu.szpc[0x100] |= Z_FLAG;

        cpu
    }

    pub fn reset(&mut self, unmapped_write: *mut u8, unmapped_read: *const u8) {
        self.state.time = 0;
        self.state.base = 0;
        self.end_time = 0;

        for i in 0..=PAGE_COUNT {
            self.state.read[i] = unmapped_read;
            self.state.write[i] = unmapped_write;
        }

        self.r = Registers::new();
    }

    /// Get current absolute time (equivalent to C++ `time()`).
    pub fn time(&self) -> CpuTime {
        self.state.time + self.state.base
    }

    /// Set absolute time (equivalent to C++ `set_time(t)`).
    pub fn set_time(&mut self, t: CpuTime) {
        self.state.time = t - self.state.base;
    }

    /// Adjust time by delta (equivalent to C++ `adjust_time(delta)`).
    pub fn adjust_time(&mut self, delta: CpuTime) {
        self.state.time += delta;
    }

    /// Map memory pages for the CPU.
    ///
    /// # Safety
    ///
    /// `write` and `read` must be valid pointers to memory regions of at least
    /// `size` bytes. The memory must remain valid for the lifetime of this CPU instance.
    pub unsafe fn map_mem(&mut self, addr: u16, size: u32, write: *mut u8, read: *const u8) {
        debug_assert!((addr as u32).is_multiple_of(PAGE_SIZE as u32));
        debug_assert!(size.is_multiple_of(PAGE_SIZE as u32));

        let first_page = addr as usize / PAGE_SIZE;
        for i in 0..(size as usize / PAGE_SIZE) {
            let offset = i * PAGE_SIZE;
            self.state.read[first_page + i] = unsafe { read.add(offset) };
            self.state.write[first_page + i] = unsafe { write.add(offset) };
        }
    }

    #[inline]
    fn read_mem(&self, addr: u16) -> u8 {
        let page = (addr as usize) >> PAGE_SHIFT;
        let offset = addr as usize & (PAGE_SIZE - 1);
        unsafe { *self.state.read[page].add(offset) }
    }

    #[inline]
    fn write_mem(&mut self, addr: u16, data: u8) {
        let page = (addr as usize) >> PAGE_SHIFT;
        let offset = addr as usize & (PAGE_SIZE - 1);
        unsafe {
            *self.state.write[page].add(offset) = data;
        }
    }

    #[inline]
    fn read_prog(&self, addr: u16) -> u8 {
        self.read_mem(addr)
    }

    /// C++ READ_WORD: GET_LE16( &READ( addr ) ) — both bytes through the page
    /// pointer of `addr`'s page (may read one byte past the page's data).
    #[inline]
    fn read_word(&self, addr: u16) -> u16 {
        let page = (addr as usize) >> PAGE_SHIFT;
        let offset = addr as usize & (PAGE_SIZE - 1);
        unsafe {
            let p = self.state.read[page].add(offset);
            u16::from_le_bytes([*p, *p.add(1)])
        }
    }

    /// C++ WRITE_WORD: SET_LE16( &RW_MEM( addr, write ), data ) — both bytes
    /// through the page pointer of `addr`'s page; does NOT invoke the write hook.
    #[inline]
    fn write_word(&mut self, addr: u16, data: u16) {
        let page = (addr as usize) >> PAGE_SHIFT;
        let offset = addr as usize & (PAGE_SIZE - 1);
        let b = data.to_le_bytes();
        unsafe {
            let p = self.state.write[page].add(offset);
            *p = b[0];
            *p.add(1) = b[1];
        }
    }

    /// Get time relative to s_time + base.
    #[allow(dead_code)]
    #[inline]
    fn time_rel(&self, s_time: CpuTime) -> CpuTime {
        s_time + self.state.base
    }

    /// Read 16-bit register pair by index (0=BC, 1=DE, 2=HL, 3=SP/FA).
    #[allow(dead_code)]
    #[inline]
    fn r16_bc(&self) -> u16 {
        u16::from_le_bytes([self.r.bregs[0], self.r.bregs[1]])
    }
    #[allow(dead_code)]
    #[inline]
    fn r16_de(&self) -> u16 {
        u16::from_le_bytes([self.r.bregs[2], self.r.bregs[3]])
    }
    #[allow(dead_code)]
    #[inline]
    fn r16_hl(&self) -> u16 {
        u16::from_le_bytes([self.r.bregs[4], self.r.bregs[5]])
    }

    #[allow(dead_code)]
    #[inline]
    fn set_r16_bc(&mut self, v: u16) {
        let b = v.to_le_bytes();
        self.r.bregs[0] = b[0];
        self.r.bregs[1] = b[1];
    }
    #[allow(dead_code)]
    #[inline]
    fn set_r16_de(&mut self, v: u16) {
        let b = v.to_le_bytes();
        self.r.bregs[2] = b[0];
        self.r.bregs[3] = b[1];
    }
    #[allow(dead_code)]
    #[inline]
    fn set_r16_hl(&mut self, v: u16) {
        let b = v.to_le_bytes();
        self.r.bregs[4] = b[0];
        self.r.bregs[5] = b[1];
    }

    #[inline]
    fn sz28p(&self, n: u8) -> u8 {
        self.szpc[n as usize]
    }

    #[inline]
    fn sz28pc(&self, n: u8) -> u8 {
        self.szpc[n as usize]
    }

    #[inline]
    fn sz28c(&self, n: u8) -> u8 {
        self.szpc[n as usize] & !P_FLAG
    }

    #[inline]
    fn sz28(&self, n: u8) -> u8 {
        self.sz28c(n)
    }

    /// Run CPU until end_time. Returns true if a warning occurred.
    /// Calls `cpu_out` for OUT instructions, `cpu_in` for IN instructions,
    /// and `cpu_write` for memory writes in the SCC range.
    pub fn run<O, I, W>(
        &mut self,
        end_time: CpuTime,
        cpu_out: &mut O,
        cpu_in: &mut I,
        cpu_write: &mut W,
    ) -> bool
    where
        O: FnMut(CpuTime, u16, u8),
        I: FnMut(CpuTime, u16) -> u8,
        W: FnMut(CpuTime, u16, u8),
    {
        // Set up state (matching C++ set_end_time behavior)
        // C++: set_end_time(end_time) { delta = state.base - end_time; state.base = end_time; state.time += delta; }
        // Then: s_time = state.time; (s_time starts at accumulated budget, counts DOWN toward 0)
        let orig_base = end_time;
        let delta = self.state.base - end_time;
        self.state.base = end_time;
        self.state.time += delta;
        let mut s_time = self.state.time;
        let mut warning = false;

        // Copy registers to locals for speed
        let mut pc = self.r.pc as u32;
        let mut sp = self.r.sp as u32;
        let mut ix = self.r.ix as u32;
        let mut iy = self.r.iy as u32;
        let mut a = self.r.bregs[6];
        let mut flags = self.r.bregs[7];
        let mut b = self.r.bregs[1]; // b is at index 1
        let mut c = self.r.bregs[0]; // c is at index 0
        let mut d = self.r.bregs[3]; // d is at index 3
        let mut e = self.r.bregs[2]; // e is at index 2
        let mut h = self.r.bregs[5]; // h is at index 5
        let mut l = self.r.bregs[4]; // l is at index 4

        // Z80 register access: index 0=B,1=C,2=D,3=E,4=H,5=L,7=A
        // Index 6 ((HL)) is always handled separately, never via these macros.
        macro_rules! r8 {
            ($idx:expr) => {
                match $idx {
                    0 => b,
                    1 => c,
                    2 => d,
                    3 => e,
                    4 => h,
                    5 => l,
                    7 => a,
                    _ => unreachable!(),
                }
            };
        }
        macro_rules! set_r8 {
            ($idx:expr, $val:expr) => {
                match $idx {
                    0 => {
                        b = $val;
                    }
                    1 => {
                        c = $val;
                    }
                    2 => {
                        d = $val;
                    }
                    3 => {
                        e = $val;
                    }
                    4 => {
                        h = $val;
                    }
                    5 => {
                        l = $val;
                    }
                    7 => {
                        a = $val;
                    }
                    _ => {
                        unreachable!()
                    }
                }
            };
        }

        macro_rules! sync_time {
            () => {
                self.state.time = s_time;
            };
        }

        macro_rules! get_time {
            () => {
                s_time + orig_base
            };
        }

        macro_rules! read_mem {
            ($addr:expr) => {{
                let addr = $addr as u16;
                self.read_mem(addr)
            }};
        }

        // C++ WRITE( addr, data ) = (SYNC_TIME(), kss_cpu_write( this, addr, data ))
        // kss_cpu_write writes memory first, then dispatches to the emu hook,
        // which uses time() = s_time + base.
        macro_rules! write_mem {
            ($addr:expr, $data:expr) => {{
                let addr = $addr as u16;
                let data = $data as u8;
                sync_time!();
                self.write_mem(addr, data);
                cpu_write(get_time!(), addr, data);
            }};
        }

        macro_rules! read_word_mem {
            ($addr:expr) => {{
                let addr = $addr as u16;
                self.read_word(addr) as u32
            }};
        }

        // C++ WRITE_WORD writes directly to mapped memory; it does NOT invoke
        // the emulator write hook.
        macro_rules! write_word_mem {
            ($addr:expr, $data:expr) => {{
                let addr = $addr as u16;
                let data = $data as u16;
                self.write_word(addr, data);
            }};
        }

        macro_rules! out_port {
            ($addr:expr, $data:expr) => {{
                let addr = $addr as u16;
                let data = $data as u8;
                cpu_out(get_time!(), addr, data);
            }};
        }

        macro_rules! in_port {
            ($addr:expr) => {{
                let addr = $addr as u16;
                cpu_in(get_time!(), addr)
            }};
        }

        // Timing tables
        const BASE_TIMING: [u8; 256] = [
            4, 10, 7, 6, 4, 4, 7, 4, 4, 11, 7, 6, 4, 4, 7, 4, 13, 10, 7, 6, 4, 4, 7, 4, 12, 11, 7,
            6, 4, 4, 7, 4, 12, 10, 16, 6, 4, 4, 7, 4, 12, 11, 16, 6, 4, 4, 7, 4, 12, 10, 13, 6, 11,
            11, 10, 4, 12, 11, 13, 6, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4,
            4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4,
            4, 7, 4, 7, 7, 7, 7, 7, 7, 4, 7, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4,
            4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7,
            4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, 11, 10, 10,
            10, 17, 11, 7, 11, 11, 10, 10, 8, 17, 17, 7, 11, 11, 10, 10, 11, 17, 11, 7, 11, 11, 4,
            10, 11, 17, 8, 7, 11, 11, 10, 10, 19, 17, 11, 7, 11, 11, 4, 10, 4, 17, 8, 7, 11, 11,
            10, 10, 4, 17, 11, 7, 11, 11, 6, 10, 4, 17, 8, 7, 11,
        ];

        const ED_DD_TIMING: [u8; 256] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x0C, 0x02, 0x00, 0x00, 0x03, 0x00, 0x00, 0x07,
            0x0C, 0x02, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0x0F, 0x0B, 0x00,
            0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x40, 0x70, 0xC0, 0x00, 0x60,
            0x0B, 0x10, 0x40, 0x40, 0x70, 0xC0, 0x00, 0x60, 0x0B, 0x10, 0x40, 0x40, 0x70, 0xC0,
            0x00, 0x60, 0x0B, 0x10, 0x40, 0x40, 0x70, 0xC0, 0x00, 0x60, 0x0B, 0x10, 0x40, 0x40,
            0x70, 0xC0, 0x00, 0x60, 0x0B, 0xA0, 0x40, 0x40, 0x70, 0xC0, 0x00, 0x60, 0x0B, 0xA0,
            0x4B, 0x4B, 0x7B, 0xCB, 0x0B, 0x6B, 0x00, 0x0B, 0x40, 0x40, 0x70, 0xC0, 0x00, 0x60,
            0x0B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x80, 0x80, 0x80, 0x80, 0x00, 0x00, 0x0B, 0x00,
            0x80, 0x80, 0x80, 0x80, 0x00, 0x00, 0x0B, 0x00, 0xD0, 0xD0, 0xD0, 0xD0, 0x00, 0x00,
            0x0B, 0x00, 0xD0, 0xD0, 0xD0, 0xD0, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x06, 0x00, 0x0F, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];

        'main_loop: loop {
            // Check bounds
            debug_assert!(pc < 0x10000);
            debug_assert!(sp < 0x10000);
            debug_assert!((flags as u32) < 0x100);
            debug_assert!(ix < 0x10000);
            debug_assert!(iy < 0x10000);

            // Fetch opcode
            let opcode = self.read_prog(pc as u16);
            pc = pc.wrapping_add(1);

            // Differential-trace hook (KSS_CPU_TRACE_RS=<path>), mirrors C++
            #[cfg(feature = "trace")]
            crate::kss_cpu_trace(pc.wrapping_sub(1) as u16, opcode, s_time + orig_base);

            // Add base timing
            let base_t = BASE_TIMING[opcode as usize] as CpuTime;
            s_time += base_t;

            // Check if we're out of time
            if s_time >= 0 {
                // Possibly out of time - check more carefully
                if s_time < base_t {
                    // Not actually out of time (overflow check)
                } else {
                    s_time -= base_t;
                    // PC was already incremented past the opcode; decrement to re-execute next time
                    pc = pc.wrapping_sub(1);
                    break 'main_loop;
                }
            }

            // Read next byte (operand)
            let data8 = self.read_prog(pc as u16);

            match opcode {
                // NOP
                0x00 => {}
                // LD r,r (excluding (HL) cases)
                0x40 => {} // LD B,B
                0x49 => {} // LD C,C
                0x52 => {} // LD D,D
                0x5B => {} // LD E,E
                0x64 => {} // LD H,H
                0x6D => {} // LD L,L
                0x7F => {} // LD A,A

                // EX AF,AF'
                0x08 => {
                    let tmp_a = a;
                    let tmp_f = flags;
                    a = self.r.alt_bregs[6];
                    flags = self.r.alt_bregs[7];
                    self.r.alt_bregs[6] = tmp_a;
                    self.r.alt_bregs[7] = tmp_f;
                }

                // OUT (imm),A
                0xD3 => {
                    pc = pc.wrapping_add(1);
                    out_port!(data8 as u16 + (a as u16) * 0x100, a);
                }

                // LD L,imm
                0x2E => {
                    pc = pc.wrapping_add(1);
                    l = data8;
                }

                // LD A,imm
                0x3E => {
                    pc = pc.wrapping_add(1);
                    a = data8;
                }

                // LD A,(addr)
                0x3A => {
                    let addr = self.read_word(pc as u16);
                    pc = pc.wrapping_add(2);
                    a = read_mem!(addr);
                }

                // JR cond,disp
                0x20 => {
                    pc = pc.wrapping_add(1);
                    if flags & Z_FLAG == 0 {
                        pc = (pc as i32 + (data8 as i8 as i32)) as u32 & 0xFFFF;
                    } else {
                        s_time -= 5;
                    }
                }
                0x28 => {
                    pc = pc.wrapping_add(1);
                    if flags & Z_FLAG != 0 {
                        pc = (pc as i32 + (data8 as i8 as i32)) as u32 & 0xFFFF;
                    } else {
                        s_time -= 5;
                    }
                }
                0x30 => {
                    pc = pc.wrapping_add(1);
                    if flags & C_FLAG == 0 {
                        pc = (pc as i32 + (data8 as i8 as i32)) as u32 & 0xFFFF;
                    } else {
                        s_time -= 5;
                    }
                }
                0x38 => {
                    pc = pc.wrapping_add(1);
                    if flags & C_FLAG != 0 {
                        pc = (pc as i32 + (data8 as i8 as i32)) as u32 & 0xFFFF;
                    } else {
                        s_time -= 5;
                    }
                }
                // JR disp
                0x18 => {
                    pc = pc.wrapping_add(1);
                    pc = (pc as i32 + (data8 as i8 as i32)) as u32 & 0xFFFF;
                }

                // DJNZ disp
                0x10 => {
                    pc = pc.wrapping_add(1);
                    b = b.wrapping_sub(1);
                    if b != 0 {
                        pc = (pc as i32 + (data8 as i8 as i32)) as u32 & 0xFFFF;
                    } else {
                        s_time -= 5;
                    }
                }

                // JP cond,addr
                0xC2 => {
                    if flags & Z_FLAG == 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                0xCA => {
                    if flags & Z_FLAG != 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                0xD2 => {
                    if flags & C_FLAG == 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                0xDA => {
                    if flags & C_FLAG != 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                0xE2 => {
                    if flags & P_FLAG == 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                0xEA => {
                    if flags & P_FLAG != 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                0xF2 => {
                    if flags & S_FLAG == 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                0xFA => {
                    if flags & S_FLAG != 0 {
                        pc = read_word_mem!(pc);
                    } else {
                        pc = pc.wrapping_add(2);
                    }
                }
                // JP addr
                0xC3 => {
                    pc = read_word_mem!(pc);
                }
                // JP HL
                0xE9 => {
                    pc = (h as u32) << 8 | l as u32;
                }

                // RET cond
                0xC0 => {
                    if flags & Z_FLAG == 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                0xC8 => {
                    if flags & Z_FLAG != 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                0xD0 => {
                    if flags & C_FLAG == 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                0xD8 => {
                    if flags & C_FLAG != 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                0xE0 => {
                    if flags & P_FLAG == 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                0xE8 => {
                    if flags & P_FLAG != 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                0xF0 => {
                    if flags & S_FLAG == 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                0xF8 => {
                    if flags & S_FLAG != 0 {
                        pc = read_word_mem!(sp);
                        sp = sp.wrapping_add(2);
                    } else {
                        s_time -= 6;
                    }
                }
                // RET
                0xC9 => {
                    pc = read_word_mem!(sp);
                    sp = sp.wrapping_add(2);
                }

                // CALL cond,addr
                0xC4 => {
                    if flags & Z_FLAG == 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                0xCC => {
                    if flags & Z_FLAG != 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                0xD4 => {
                    if flags & C_FLAG == 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                0xDC => {
                    if flags & C_FLAG != 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                0xE4 => {
                    if flags & P_FLAG == 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                0xEC => {
                    if flags & P_FLAG != 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                0xF4 => {
                    if flags & S_FLAG == 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                0xFC => {
                    if flags & S_FLAG != 0 {
                        let addr = read_word_mem!(pc);
                        sp = sp.wrapping_sub(2);
                        write_word_mem!(sp, pc.wrapping_add(2) as u16);
                        pc = addr;
                    } else {
                        pc = pc.wrapping_add(2);
                        s_time -= 7;
                    }
                }
                // CALL addr
                0xCD => {
                    let addr = read_word_mem!(pc);
                    sp = sp.wrapping_sub(2);
                    write_word_mem!(sp, pc.wrapping_add(2) as u16);
                    pc = addr;
                }

                // RST
                0xFF => {
                    if pc > IDLE_ADDR {
                        // Hit idle addr
                        s_time -= 11;
                        pc = pc.wrapping_sub(1);
                        break 'main_loop;
                    }
                    let data16 = pc as u16;
                    sp = sp.wrapping_sub(2);
                    write_word_mem!(sp, data16);
                    pc = opcode as u32 & 0x38;
                }
                0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 => {
                    let data16 = pc as u16;
                    sp = sp.wrapping_sub(2);
                    write_word_mem!(sp, data16);
                    pc = opcode as u32 & 0x38;
                }

                // PUSH/POP
                0xF5 => {
                    sp = sp.wrapping_sub(2);
                    write_word_mem!(sp, (a as u16) << 8 | flags as u16);
                }
                0xC5 => {
                    sp = sp.wrapping_sub(2);
                    write_word_mem!(sp, (b as u16) << 8 | c as u16);
                }
                0xD5 => {
                    sp = sp.wrapping_sub(2);
                    write_word_mem!(sp, (d as u16) << 8 | e as u16);
                }
                0xE5 => {
                    sp = sp.wrapping_sub(2);
                    write_word_mem!(sp, (h as u16) << 8 | l as u16);
                }

                0xF1 => {
                    let v = read_word_mem!(sp);
                    flags = (v & 0xFF) as u8;
                    a = (v >> 8) as u8;
                    sp = sp.wrapping_add(2);
                }
                0xC1 => {
                    let v = read_word_mem!(sp);
                    c = (v & 0xFF) as u8;
                    b = (v >> 8) as u8;
                    sp = sp.wrapping_add(2);
                }
                0xD1 => {
                    let v = read_word_mem!(sp);
                    e = (v & 0xFF) as u8;
                    d = (v >> 8) as u8;
                    sp = sp.wrapping_add(2);
                }
                0xE1 => {
                    let v = read_word_mem!(sp);
                    l = (v & 0xFF) as u8;
                    h = (v >> 8) as u8;
                    sp = sp.wrapping_add(2);
                }

                // SUB/ADD/SBC/ADC (HL)
                0x96 | 0x86 => {
                    let v = read_mem!((h as u16) << 8 | l as u16);
                    let op = if opcode == 0x96 || opcode == 0x9E {
                        N_FLAG
                    } else {
                        0
                    };
                    flags = op;
                    let result = if op != 0 {
                        a as i32 - v as i32
                    } else {
                        a as i32 + v as i32
                    };
                    // Simplified flag calculation
                    let xor = (a ^ v) as i32;
                    let r8 = (result & 0xFF) as u8;
                    flags |= self.sz28(r8);
                    flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                    flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                    if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                        flags |= P_FLAG;
                    }
                    a = r8;
                }
                0x9E | 0x8E => {
                    let v = read_mem!((h as u16) << 8 | l as u16);
                    let carry = if opcode == 0x9E || opcode == 0x8E {
                        flags & C_FLAG
                    } else {
                        0
                    };
                    let sub = opcode >= 0x90;
                    flags = if sub { N_FLAG } else { 0 };
                    let result = if sub {
                        a as i32 - v as i32 - carry as i32
                    } else {
                        a as i32 + v as i32 + carry as i32
                    };
                    let xor = (a ^ v) as i32;
                    let r8 = (result & 0xFF) as u8;
                    flags |= self.sz28(r8);
                    flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                    flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                    if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                        flags |= P_FLAG;
                    }
                    a = r8;
                }

                // SUB/ADD/SBC/ADC imm
                0xD6 | 0xC6 => {
                    pc = pc.wrapping_add(1);
                    let v = data8;
                    let sub = opcode == 0xD6;
                    flags = if sub { N_FLAG } else { 0 };
                    let result = if sub {
                        a as i32 - v as i32
                    } else {
                        a as i32 + v as i32
                    };
                    let xor = (a ^ v) as i32;
                    let r8 = (result & 0xFF) as u8;
                    flags |= self.sz28(r8);
                    flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                    flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                    if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                        flags |= P_FLAG;
                    }
                    a = r8;
                }
                0xDE | 0xCE => {
                    pc = pc.wrapping_add(1);
                    let v = data8;
                    let carry = flags & C_FLAG;
                    let sub = opcode == 0xDE;
                    flags = if sub { N_FLAG } else { 0 };
                    let result = if sub {
                        a as i32 - v as i32 - carry as i32
                    } else {
                        a as i32 + v as i32 + carry as i32
                    };
                    let xor = (a ^ v) as i32;
                    let r8 = (result & 0xFF) as u8;
                    flags |= self.sz28(r8);
                    flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                    flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                    if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                        flags |= P_FLAG;
                    }
                    a = r8;
                }

                // SUB/ADD/SBC/ADC r
                0x90..=0x97 | 0x80..=0x87 => {
                    let reg_idx = (opcode & 7) as usize;
                    let v = r8!(reg_idx);
                    let sub = opcode >= 0x90;
                    flags = if sub { N_FLAG } else { 0 };
                    let result = if sub {
                        a as i32 - v as i32
                    } else {
                        a as i32 + v as i32
                    };
                    let xor = (a ^ v) as i32;
                    let r8 = (result & 0xFF) as u8;
                    flags |= self.sz28(r8);
                    flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                    flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                    if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                        flags |= P_FLAG;
                    }
                    a = r8;
                }
                0x98..=0x9F | 0x88..=0x8F => {
                    let reg_idx = (opcode & 7) as usize;
                    let v = r8!(reg_idx);
                    let carry = flags & C_FLAG;
                    let sub = opcode >= 0x98;
                    flags = if sub { N_FLAG } else { 0 };
                    let result = if sub {
                        a as i32 - v as i32 - carry as i32
                    } else {
                        a as i32 + v as i32 + carry as i32
                    };
                    let xor = (a ^ v) as i32;
                    let r8 = (result & 0xFF) as u8;
                    flags |= self.sz28(r8);
                    flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                    flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                    if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                        flags |= P_FLAG;
                    }
                    a = r8;
                }

                // CP (HL)
                0xBE => {
                    let v = read_mem!((h as u16) << 8 | l as u16);
                    let result = a as i32 - v as i32;
                    flags = N_FLAG | (v & (0x20 | 0x08)) | ((result >> 8) as u8 & C_FLAG);
                    let xor = (a ^ v) as i32;
                    flags |= (((result ^ a as i32) & xor) >> 5 & P_FLAG as i32) as u8;
                    flags |= (((xor & H_FLAG as i32) ^ result) as u8) & (S_FLAG | H_FLAG);
                    if (result & 0xFF) == 0 {
                        flags |= Z_FLAG;
                    }
                }
                // CP imm
                0xFE => {
                    pc = pc.wrapping_add(1);
                    let v = data8;
                    let result = a as i32 - v as i32;
                    flags = N_FLAG | (v & (0x20 | 0x08)) | ((result >> 8) as u8 & C_FLAG);
                    let xor = (a ^ v) as i32;
                    flags |= (((result ^ a as i32) & xor) >> 5 & P_FLAG as i32) as u8;
                    flags |= (((xor & H_FLAG as i32) ^ result) as u8) & (S_FLAG | H_FLAG);
                    if (result & 0xFF) == 0 {
                        flags |= Z_FLAG;
                    }
                }
                // CP r
                0xB8..=0xBF => {
                    let reg_idx = (opcode & 7) as usize;
                    let v = r8!(reg_idx);
                    let result = a as i32 - v as i32;
                    flags = N_FLAG | (v & (0x20 | 0x08)) | ((result >> 8) as u8 & C_FLAG);
                    let xor = (a ^ v) as i32;
                    flags |= (((result ^ a as i32) & xor) >> 5 & P_FLAG as i32) as u8;
                    flags |= (((xor & H_FLAG as i32) ^ result) as u8) & (S_FLAG | H_FLAG);
                    if (result & 0xFF) == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ADD HL,rp
                0x09 => {
                    let hl = (h as u32) << 8 | l as u32;
                    let rp = (b as u32) << 8 | c as u32;
                    let sum = hl + rp;
                    h = (sum >> 8) as u8;
                    l = sum as u8;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                        | ((sum >> 16) as u8 & C_FLAG)
                        | ((sum >> 8) as u8 & (0x20 | 0x08));
                }
                0x19 => {
                    let hl = (h as u32) << 8 | l as u32;
                    let rp = (d as u32) << 8 | e as u32;
                    let sum = hl + rp;
                    h = (sum >> 8) as u8;
                    l = sum as u8;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                        | ((sum >> 16) as u8 & C_FLAG)
                        | ((sum >> 8) as u8 & (0x20 | 0x08));
                }
                0x29 => {
                    let hl = (h as u32) << 8 | l as u32;
                    let sum = hl + hl;
                    h = (sum >> 8) as u8;
                    l = sum as u8;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                        | ((sum >> 16) as u8 & C_FLAG)
                        | ((sum >> 8) as u8 & (0x20 | 0x08));
                }
                0x39 => {
                    let hl = (h as u32) << 8 | l as u32;
                    let sum = hl + sp;
                    h = (sum >> 8) as u8;
                    l = sum as u8;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                        | ((sum >> 16) as u8 & C_FLAG)
                        | ((sum >> 8) as u8 & (0x20 | 0x08));
                }

                // DAA
                0x27 => {
                    let mut aa = a as i32;
                    if aa > 0x99 {
                        flags |= C_FLAG;
                    }
                    let mut adjust = if flags & C_FLAG != 0 { 0x60 } else { 0 };
                    if flags & H_FLAG != 0 || (aa & 0x0F) > 9 {
                        adjust |= 0x06;
                    }
                    if flags & N_FLAG != 0 {
                        adjust = -adjust;
                    }
                    aa += adjust;
                    flags = (flags & (C_FLAG | N_FLAG))
                        | ((a ^ aa as u8) & H_FLAG)
                        | self.sz28p(aa as u8);
                    a = aa as u8;
                }

                // INC/DEC (HL)
                0x34 => {
                    let addr = (h as u16) << 8 | l as u16;
                    let v = read_mem!(addr).wrapping_add(1);
                    write_mem!(addr, v);
                    flags =
                        (flags & C_FLAG) | (((v as i32 - 1) & 0x0F) as u8 & H_FLAG) | self.sz28(v);
                    if v == 0x80 {
                        flags |= P_FLAG;
                    }
                }
                0x35 => {
                    let addr = (h as u16) << 8 | l as u16;
                    let v = read_mem!(addr).wrapping_sub(1);
                    write_mem!(addr, v);
                    flags = (flags & C_FLAG)
                        | N_FLAG
                        | (((v as i32 + 1) & 0x0F) as u8 & H_FLAG)
                        | self.sz28(v);
                    if v == 0x7F {
                        flags |= P_FLAG;
                    }
                }

                // INC r
                0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => {
                    let reg_idx = ((opcode >> 3) & 7) as usize;
                    let v = r8!(reg_idx).wrapping_add(1);
                    set_r8!(reg_idx, v);
                    flags =
                        (flags & C_FLAG) | (((v as i32 - 1) & 0x0F) as u8 & H_FLAG) | self.sz28(v);
                    if v == 0x80 {
                        flags |= P_FLAG;
                    }
                }
                // DEC r
                0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => {
                    let reg_idx = ((opcode >> 3) & 7) as usize;
                    let v = r8!(reg_idx).wrapping_sub(1);
                    set_r8!(reg_idx, v);
                    flags = (flags & C_FLAG)
                        | N_FLAG
                        | (((v as i32 + 1) & 0x0F) as u8 & H_FLAG)
                        | self.sz28(v);
                    if v == 0x7F {
                        flags |= P_FLAG;
                    }
                }

                // INC rp
                0x03 => {
                    let v = ((b as u16) << 8 | c as u16).wrapping_add(1);
                    b = (v >> 8) as u8;
                    c = v as u8;
                }
                0x13 => {
                    let v = ((d as u16) << 8 | e as u16).wrapping_add(1);
                    d = (v >> 8) as u8;
                    e = v as u8;
                }
                0x23 => {
                    let v = ((h as u16) << 8 | l as u16).wrapping_add(1);
                    h = (v >> 8) as u8;
                    l = v as u8;
                }
                0x33 => {
                    sp = sp.wrapping_add(1) & 0xFFFF;
                }

                // DEC rp
                0x0B => {
                    let v = ((b as u16) << 8 | c as u16).wrapping_sub(1);
                    b = (v >> 8) as u8;
                    c = v as u8;
                }
                0x1B => {
                    let v = ((d as u16) << 8 | e as u16).wrapping_sub(1);
                    d = (v >> 8) as u8;
                    e = v as u8;
                }
                0x2B => {
                    let v = ((h as u16) << 8 | l as u16).wrapping_sub(1);
                    h = (v >> 8) as u8;
                    l = v as u8;
                }
                0x3B => {
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                }

                // AND
                0xA6 => {
                    let v = read_mem!((h as u16) << 8 | l as u16);
                    a &= v;
                    flags = self.sz28p(a) | H_FLAG;
                }
                0xE6 => {
                    pc = pc.wrapping_add(1);
                    a &= data8;
                    flags = self.sz28p(a) | H_FLAG;
                }
                0xA0..=0xA7 => {
                    let reg_idx = (opcode & 7) as usize;
                    a &= r8!(reg_idx);
                    flags = self.sz28p(a) | H_FLAG;
                }

                // OR
                0xB6 => {
                    let v = read_mem!((h as u16) << 8 | l as u16);
                    a |= v;
                    flags = self.sz28p(a);
                }
                0xF6 => {
                    pc = pc.wrapping_add(1);
                    a |= data8;
                    flags = self.sz28p(a);
                }
                0xB0..=0xB7 => {
                    let reg_idx = (opcode & 7) as usize;
                    a |= r8!(reg_idx);
                    flags = self.sz28p(a);
                }

                // XOR
                0xAE => {
                    let v = read_mem!((h as u16) << 8 | l as u16);
                    a ^= v;
                    flags = self.sz28p(a);
                }
                0xEE => {
                    pc = pc.wrapping_add(1);
                    a ^= data8;
                    flags = self.sz28p(a);
                }
                0xA8..=0xAF => {
                    let reg_idx = (opcode & 7) as usize;
                    a ^= r8!(reg_idx);
                    flags = self.sz28p(a);
                }

                // LD (HL),r
                0x70..=0x75 | 0x77 => {
                    let reg_idx = (opcode & 7) as usize;
                    let v = r8!(reg_idx);
                    let addr = (h as u16) << 8 | l as u16;
                    write_mem!(addr, v);
                }
                // LD r,r
                0x41..=0x45 | 0x47 => {
                    let dst = ((opcode >> 3) & 7) as usize;
                    let src = (opcode & 7) as usize;
                    let v = r8!(src);
                    set_r8!(dst, v);
                }
                0x48..=0x4A | 0x4B..=0x4D | 0x4F => {
                    let dst = ((opcode >> 3) & 7) as usize;
                    let src = (opcode & 7) as usize;
                    let v = r8!(src);
                    set_r8!(dst, v);
                }
                0x50..=0x53 | 0x54 | 0x55 | 0x57 => {
                    let dst = ((opcode >> 3) & 7) as usize;
                    let src = (opcode & 7) as usize;
                    let v = r8!(src);
                    set_r8!(dst, v);
                }
                0x58..=0x59 | 0x5A | 0x5C | 0x5D | 0x5F => {
                    let dst = ((opcode >> 3) & 7) as usize;
                    let src = (opcode & 7) as usize;
                    let v = r8!(src);
                    set_r8!(dst, v);
                }
                0x60..=0x63 | 0x65 | 0x67 => {
                    let dst = ((opcode >> 3) & 7) as usize;
                    let src = (opcode & 7) as usize;
                    let v = r8!(src);
                    set_r8!(dst, v);
                }
                0x68..=0x6B | 0x6C | 0x6F => {
                    let dst = ((opcode >> 3) & 7) as usize;
                    let src = (opcode & 7) as usize;
                    let v = r8!(src);
                    set_r8!(dst, v);
                }
                0x78..=0x7D => {
                    let dst = ((opcode >> 3) & 7) as usize;
                    let src = (opcode & 7) as usize;
                    let v = r8!(src);
                    set_r8!(dst, v);
                }

                // LD r,imm
                0x06 | 0x0E | 0x16 | 0x1E | 0x26 => {
                    pc = pc.wrapping_add(1);
                    let reg_idx = ((opcode >> 3) & 7) as usize;
                    set_r8!(reg_idx, data8);
                }
                // LD (HL),imm
                0x36 => {
                    pc = pc.wrapping_add(1);
                    let addr = (h as u16) << 8 | l as u16;
                    write_mem!(addr, data8);
                }
                // LD r,(HL)
                0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                    let addr = (h as u16) << 8 | l as u16;
                    let v = read_mem!(addr);
                    let reg_idx = ((opcode >> 3) & 7) as usize;
                    set_r8!(reg_idx, v);
                }

                // LD rp,imm
                0x01 | 0x11 | 0x21 => {
                    let v = read_word_mem!(pc);
                    pc = pc.wrapping_add(2);
                    match opcode {
                        0x01 => {
                            b = (v >> 8) as u8;
                            c = v as u8;
                        }
                        0x11 => {
                            d = (v >> 8) as u8;
                            e = v as u8;
                        }
                        0x21 => {
                            h = (v >> 8) as u8;
                            l = v as u8;
                        }
                        _ => {}
                    }
                }
                0x31 => {
                    sp = read_word_mem!(pc);
                    pc = pc.wrapping_add(2);
                }

                // LD HL,(addr)
                0x2A => {
                    let addr = read_word_mem!(pc);
                    pc = pc.wrapping_add(2);
                    let v = read_word_mem!(addr);
                    l = v as u8;
                    h = (v >> 8) as u8;
                }

                // LD (addr),A
                0x32 => {
                    let addr = read_word_mem!(pc);
                    pc = pc.wrapping_add(2);
                    write_mem!(addr, a);
                }

                // LD (addr),HL
                0x22 => {
                    let addr = read_word_mem!(pc);
                    pc = pc.wrapping_add(2);
                    write_word_mem!(addr, (h as u16) << 8 | l as u16);
                }

                // LD (BC),A / LD (DE),A
                0x02 => {
                    let addr = (b as u16) << 8 | c as u16;
                    write_mem!(addr, a);
                }
                0x12 => {
                    let addr = (d as u16) << 8 | e as u16;
                    write_mem!(addr, a);
                }

                // LD A,(BC) / LD A,(DE)
                0x0A => {
                    let addr = (b as u16) << 8 | c as u16;
                    a = read_mem!(addr);
                }
                0x1A => {
                    let addr = (d as u16) << 8 | e as u16;
                    a = read_mem!(addr);
                }

                // LD SP,HL
                0xF9 => {
                    sp = (h as u32) << 8 | l as u32;
                }

                // Rotate A
                0x07 => {
                    // RLCA
                    let tmp = a as u32;
                    let tmp = (tmp << 1) | (tmp >> 7);
                    flags =
                        (flags & (S_FLAG | Z_FLAG | P_FLAG)) | (tmp as u8 & (0x20 | 0x08 | C_FLAG));
                    a = tmp as u8;
                }
                0x0F => {
                    // RRCA
                    let tmp = a as u32;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG)) | (a & C_FLAG);
                    let tmp = (tmp << 7) | (tmp >> 1);
                    flags |= tmp as u8 & (0x20 | 0x08);
                    a = tmp as u8;
                }
                0x17 => {
                    // RLA
                    let tmp = (a as u32) << 1 | (flags & C_FLAG) as u32;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                        | (tmp as u8 & (0x20 | 0x08))
                        | ((tmp >> 8) as u8 & C_FLAG);
                    a = tmp as u8;
                }
                0x1F => {
                    // RRA
                    let tmp = ((flags & C_FLAG) as u32) << 7 | (a as u32) >> 1;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                        | (tmp as u8 & (0x20 | 0x08))
                        | (a & C_FLAG);
                    a = tmp as u8;
                }

                // CPL
                0x2F => {
                    a = !a;
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG | C_FLAG))
                        | (a & (0x20 | 0x08))
                        | (H_FLAG | N_FLAG);
                }

                // CCF
                0x37 => {
                    // SCF
                    flags = (flags & (S_FLAG | Z_FLAG | P_FLAG)) | C_FLAG | (a & (0x20 | 0x08));
                }
                0x3F => {
                    flags = ((flags & (S_FLAG | Z_FLAG | P_FLAG | C_FLAG)) ^ C_FLAG)
                        | (flags << 4 & H_FLAG)
                        | (a & (0x20 | 0x08));
                }

                // IN A,(imm)
                0xDB => {
                    pc = pc.wrapping_add(1);
                    a = in_port!(data8 as u16 + (a as u16) * 0x100);
                }

                // EX (SP),HL
                0xE3 => {
                    let tmp = read_word_mem!(sp);
                    write_word_mem!(sp, (h as u16) << 8 | l as u16);
                    l = tmp as u8;
                    h = (tmp >> 8) as u8;
                }

                // EX DE,HL
                0xEB => {
                    let tmp_d = d;
                    let tmp_e = e;
                    d = h;
                    e = l;
                    h = tmp_d;
                    l = tmp_e;
                }

                // EXX
                0xD9 => {
                    for i in 0..8 {
                        std::mem::swap(&mut self.r.bregs[i], &mut self.r.alt_bregs[i]);
                    }
                    // Update local copies
                    c = self.r.bregs[0];
                    b = self.r.bregs[1];
                    e = self.r.bregs[2];
                    d = self.r.bregs[3];
                    l = self.r.bregs[4];
                    h = self.r.bregs[5];
                }

                // DI
                0xF3 => {
                    self.r.iff1 = 0;
                    self.r.iff2 = 0;
                }
                // EI
                0xFB => {
                    self.r.iff1 = 1;
                    self.r.iff2 = 1;
                }
                // HALT
                0x76 => {
                    // In KSS, HALT is treated as hitting idle
                    s_time &= 3;
                    pc = pc.wrapping_sub(1);
                    break 'main_loop;
                }

                // CB prefix
                0xCB => {
                    let cb_op = data8;
                    pc = pc.wrapping_add(1);
                    let addr_hl = (h as u16) << 8 | l as u16;

                    match cb_op {
                        // RLC (HL)
                        0x06 => {
                            s_time += 7;
                            let mut v = read_mem!(addr_hl);
                            let bit7 = v >> 7;
                            v = (v << 1) | bit7;
                            write_mem!(addr_hl, v);
                            flags = self.sz28p(v) | (bit7 & C_FLAG);
                        }
                        // RLC r
                        0x00..=0x05 | 0x07 => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit7 = v >> 7;
                            v = (v << 1) | bit7;
                            set_r8!(reg_idx, v);
                            flags = self.sz28p(v) | (bit7 & C_FLAG);
                        }

                        // RL (HL)
                        0x16 => {
                            s_time += 7;
                            let mut v = read_mem!(addr_hl);
                            let bit7 = v >> 7;
                            v = (v << 1) | (flags & C_FLAG);
                            write_mem!(addr_hl, v);
                            flags = self.sz28pc(v) | (bit7 & C_FLAG);
                        }
                        // RL r
                        0x10..=0x15 | 0x17 => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit7 = v >> 7;
                            v = (v << 1) | (flags & C_FLAG);
                            set_r8!(reg_idx, v);
                            flags = self.sz28pc(v) | (bit7 & C_FLAG);
                        }

                        // SLA (HL)
                        0x26 => {
                            s_time += 7;
                            let mut v = read_mem!(addr_hl);
                            let bit7 = v >> 7;
                            v <<= 1;
                            write_mem!(addr_hl, v);
                            flags = self.sz28pc(v) | (bit7 & C_FLAG);
                        }
                        // SLA r
                        0x20..=0x25 | 0x27 => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit7 = v >> 7;
                            v <<= 1;
                            set_r8!(reg_idx, v);
                            flags = self.sz28pc(v) | (bit7 & C_FLAG);
                        }

                        // SLL (HL) - undocumented
                        0x36 => {
                            s_time += 7;
                            let mut v = read_mem!(addr_hl);
                            let bit7 = v >> 7;
                            v = (v << 1) | 1;
                            write_mem!(addr_hl, v);
                            flags = self.sz28pc(v) | (bit7 & C_FLAG);
                        }
                        // SLL r - undocumented
                        0x30..=0x35 | 0x37 => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit7 = v >> 7;
                            v = (v << 1) | 1;
                            set_r8!(reg_idx, v);
                            flags = self.sz28pc(v) | (bit7 & C_FLAG);
                        }

                        // RRC (HL)
                        0x0E => {
                            s_time += 7;
                            let mut v = read_mem!(addr_hl);
                            let bit0 = v & C_FLAG;
                            v = (v >> 1) | (bit0 << 7);
                            write_mem!(addr_hl, v);
                            flags = self.sz28p(v) | bit0;
                        }
                        // RRC r
                        0x08..=0x0D | 0x0F => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit0 = v & C_FLAG;
                            v = (v >> 1) | (bit0 << 7);
                            set_r8!(reg_idx, v);
                            flags = self.sz28p(v) | bit0;
                        }

                        // RR (HL)
                        0x1E => {
                            s_time += 7;
                            let mut v = read_mem!(addr_hl);
                            let bit0 = v & C_FLAG;
                            v = (flags << 7) | (v >> 1);
                            write_mem!(addr_hl, v);
                            flags = self.sz28p(v) | bit0;
                        }
                        // RR r
                        0x18..=0x1D | 0x1F => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit0 = v & C_FLAG;
                            v = (flags << 7) | (v >> 1);
                            set_r8!(reg_idx, v);
                            flags = self.sz28p(v) | bit0;
                        }

                        // SRA (HL)
                        0x2E => {
                            let mut v = read_mem!(addr_hl);
                            let bit0 = v & C_FLAG;
                            v = (v & 0x80) | (v >> 1);
                            write_mem!(addr_hl, v);
                            flags = self.sz28p(v) | bit0;
                        }
                        // SRA r
                        0x28..=0x2D | 0x2F => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit0 = v & C_FLAG;
                            v = (v & 0x80) | (v >> 1);
                            set_r8!(reg_idx, v);
                            flags = self.sz28p(v) | bit0;
                        }

                        // SRL (HL)
                        0x3E => {
                            let mut v = read_mem!(addr_hl);
                            let bit0 = v & C_FLAG;
                            v >>= 1;
                            write_mem!(addr_hl, v);
                            flags = self.sz28p(v) | bit0;
                        }
                        // SRL r
                        0x38..=0x3D | 0x3F => {
                            let reg_idx = (cb_op & 7) as usize;
                            let mut v = r8!(reg_idx);
                            let bit0 = v & C_FLAG;
                            v >>= 1;
                            set_r8!(reg_idx, v);
                            flags = self.sz28p(v) | bit0;
                        }

                        // BIT b,(HL)
                        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x76 | 0x7E => {
                            s_time += 4;
                            let v = read_mem!(addr_hl);
                            let bit = (cb_op >> 3) & 7;
                            let masked = v & (1 << bit);
                            flags = (flags & C_FLAG)
                                | H_FLAG
                                | (masked & S_FLAG)
                                | if masked == 0 { Z_FLAG | P_FLAG } else { 0 };
                        }
                        // BIT b,r
                        0x40..=0x45
                        | 0x47
                        | 0x48..=0x4D
                        | 0x4F
                        | 0x50..=0x55
                        | 0x57
                        | 0x58..=0x5D
                        | 0x5F
                        | 0x60..=0x65
                        | 0x67
                        | 0x68..=0x6D
                        | 0x6F
                        | 0x70..=0x75
                        | 0x77
                        | 0x78..=0x7D
                        | 0x7F => {
                            let v = r8!((cb_op & 7) as usize);
                            let bit = (cb_op >> 3) & 7;
                            let masked = v & (1 << bit);
                            flags = (flags & C_FLAG)
                                | H_FLAG
                                | (masked & S_FLAG)
                                | if masked == 0 { Z_FLAG | P_FLAG } else { 0 };
                        }

                        // RES b,(HL)
                        0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                            s_time += 7;
                            let v = read_mem!(addr_hl) & !(1 << ((cb_op >> 3) & 7));
                            write_mem!(addr_hl, v);
                        }
                        // SET b,(HL)
                        0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                            s_time += 7;
                            let v = read_mem!(addr_hl) | (1 << ((cb_op >> 3) & 7));
                            write_mem!(addr_hl, v);
                        }

                        // RES/SET b,r
                        _ => {
                            let bit = (cb_op >> 3) & 7;
                            let reg_idx = (cb_op & 7) as usize;
                            if (0x80..0xC0).contains(&cb_op) {
                                // RES
                                set_r8!(reg_idx, r8!(reg_idx) & !(1 << bit));
                            } else if cb_op >= 0xC0 {
                                // SET
                                set_r8!(reg_idx, r8!(reg_idx) | (1 << bit));
                            }
                        }
                    }
                }

                // ED prefix
                0xED => {
                    pc = pc.wrapping_add(1);
                    s_time += (ED_DD_TIMING[data8 as usize] >> 4) as CpuTime;

                    match data8 {
                        // SBC HL,rp / ADC HL,rp
                        0x42 | 0x52 | 0x62 | 0x72 | 0x4A | 0x5A | 0x6A | 0x7A => {
                            let rp_val = match data8 & 0x30 {
                                0x00 => (b as u32) << 8 | c as u32,
                                0x10 => (d as u32) << 8 | e as u32,
                                0x20 => (h as u32) << 8 | l as u32,
                                0x30 => sp,
                                _ => 0,
                            };
                            let sub = data8 & 0x08 == 0;
                            let carry = flags & C_FLAG as u32 as u8;
                            let hl = (h as u32) << 8 | l as u32;
                            let result = if sub {
                                hl.wrapping_sub(rp_val).wrapping_sub(carry as u32)
                            } else {
                                hl.wrapping_add(rp_val).wrapping_add(carry as u32)
                            };
                            let xor = hl ^ rp_val;
                            flags = if sub { N_FLAG } else { 0 };
                            flags |= ((result >> 16) as u8 & C_FLAG)
                                | (((xor ^ result) >> 12) as u8 & H_FLAG)
                                | ((result >> 8) as u8 & (S_FLAG | 0x20 | 0x08))
                                | if (result & 0xFFFF) == 0 { Z_FLAG } else { 0 };
                            if ((result ^ hl) & (result ^ xor) & 0x8000) != 0 {
                                flags |= P_FLAG;
                            }
                            h = (result >> 8) as u8;
                            l = result as u8;
                        }

                        // IN r,(C)
                        0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                            let v = in_port!((b as u16) << 8 | c as u16);
                            let reg_idx = ((data8 >> 3) & 7) as usize;
                            set_r8!(reg_idx, v);
                            flags = (flags & C_FLAG) | self.sz28p(v);
                        }

                        // OUT (C),r / OUT (C),0
                        0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x71 | 0x79 => {
                            let v = if data8 == 0x71 {
                                0
                            } else {
                                r8!(((data8 >> 3) & 7) as usize)
                            };
                            out_port!((b as u16) << 8 | c as u16, v);
                        }

                        // LD (addr),rp
                        0x43 | 0x53 | 0x73 => {
                            let addr = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                            let v = match data8 & 0x30 {
                                0x00 => (b as u16) << 8 | c as u16,
                                0x10 => (d as u16) << 8 | e as u16,
                                0x30 => sp as u16,
                                _ => 0,
                            };
                            write_word_mem!(addr, v);
                        }

                        // LD rp,(addr)
                        0x4B | 0x5B | 0x7B => {
                            let addr = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                            let v = read_word_mem!(addr);
                            match data8 & 0x30 {
                                0x00 => {
                                    b = (v >> 8) as u8;
                                    c = v as u8;
                                }
                                0x10 => {
                                    d = (v >> 8) as u8;
                                    e = v as u8;
                                }
                                0x30 => {
                                    sp = v;
                                }
                                _ => {}
                            }
                        }

                        // NEG
                        0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => {
                            let v = a;
                            let result = 0i32 - v as i32;
                            flags = N_FLAG;
                            let r8 = (result & 0xFF) as u8;
                            flags |= self.sz28(r8);
                            flags |= (result >> 4) as u8 & H_FLAG;
                            flags |= (result >> 8) as u8 & C_FLAG;
                            if v == 0x80 {
                                flags |= P_FLAG;
                            }
                            a = r8;
                        }

                        // RRD
                        0x67 => {
                            let addr = (h as u16) << 8 | l as u16;
                            let v = read_mem!(addr);
                            write_mem!(addr, (a << 4) | (v >> 4));
                            a = (a & 0xF0) | (v & 0x0F);
                            flags = (flags & C_FLAG) | self.sz28p(a);
                        }
                        // RLD
                        0x6F => {
                            let addr = (h as u16) << 8 | l as u16;
                            let v = read_mem!(addr);
                            write_mem!(addr, (v << 4) | (a & 0x0F));
                            a = (a & 0xF0) | (v >> 4);
                            flags = (flags & C_FLAG) | self.sz28p(a);
                        }

                        // LDI/LDD/LDIR/LDDR
                        0xA0 | 0xA8 | 0xB0 | 0xB8 => {
                            let inc = if data8 & 0x08 == 0 { 1i32 } else { -1 };
                            let mut addr = (h as u32) << 8 | l as u32;
                            let v = read_mem!(addr as u16);
                            addr = (addr as i32 + inc) as u32 & 0xFFFF;
                            h = (addr >> 8) as u8;
                            l = addr as u8;
                            let mut de = (d as u32) << 8 | e as u32;
                            write_mem!(de as u16, v);
                            de = (de as i32 + inc) as u32 & 0xFFFF;
                            d = (de >> 8) as u8;
                            e = de as u8;
                            let mut bc = (b as u32) << 8 | c as u32;
                            bc = bc.wrapping_sub(1);
                            b = (bc >> 8) as u8;
                            c = bc as u8;
                            let tmp = v.wrapping_add(a);
                            flags = (flags & (S_FLAG | Z_FLAG | C_FLAG))
                                | (tmp & 0x08)
                                | ((tmp & 0x02) << 4);
                            if bc != 0 {
                                flags |= P_FLAG;
                            }
                            if data8 >= 0xB0 && bc != 0 {
                                pc = pc.wrapping_sub(2);
                                s_time += 5;
                            }
                        }

                        // CPI/CPD/CPIR/CPDR
                        0xA1 | 0xA9 | 0xB1 | 0xB9 => {
                            let inc = if data8 & 0x08 == 0 { 1i32 } else { -1 };
                            let mut addr = (h as u32) << 8 | l as u32;
                            let v = read_mem!(addr as u16);
                            addr = (addr as i32 + inc) as u32 & 0xFFFF;
                            h = (addr >> 8) as u8;
                            l = addr as u8;
                            let result = a as i32 - v as i32;
                            let mut bc = (b as u32) << 8 | c as u32;
                            bc = bc.wrapping_sub(1);
                            b = (bc >> 8) as u8;
                            c = bc as u8;
                            flags = (flags & C_FLAG)
                                | N_FLAG
                                | ((((v ^ a) & H_FLAG) ^ result as u8) & (S_FLAG | H_FLAG));
                            if (result & 0xFF) == 0 {
                                flags |= Z_FLAG;
                            }
                            let tmp = result - ((flags & H_FLAG) >> 4) as i32;
                            flags |= (tmp as u8 & 0x08) | ((tmp as u8 & 0x02) << 4);
                            if bc != 0 {
                                flags |= P_FLAG;
                            }
                            if data8 >= 0xB0 && (flags & Z_FLAG == 0) && bc != 0 {
                                pc = pc.wrapping_sub(2);
                                s_time += 5;
                            }
                        }

                        // INI/IND/INIR/INDR
                        0xA2 | 0xAA | 0xB2 | 0xBA => {
                            let inc = if data8 & 0x08 == 0 { 1i32 } else { -1 };
                            let v = in_port!((b as u16) << 8 | c as u16);
                            b = b.wrapping_sub(1);
                            let mut addr = (h as u32) << 8 | l as u32;
                            write_mem!(addr as u16, v);
                            addr = (addr as i32 + inc) as u32 & 0xFFFF;
                            h = (addr >> 8) as u8;
                            l = addr as u8;
                            flags = (v >> 6 & N_FLAG) | self.sz28(b);
                            if data8 >= 0xB0 && b != 0 {
                                pc = pc.wrapping_sub(2);
                                s_time += 5;
                            }
                        }

                        // OUTI/OUTD/OTIR/OTDR
                        0xA3 | 0xAB | 0xB3 | 0xBB => {
                            let inc = if data8 & 0x08 == 0 { 1i32 } else { -1 };
                            b = b.wrapping_sub(1);
                            let mut addr = (h as u32) << 8 | l as u32;
                            let v = read_mem!(addr as u16);
                            addr = (addr as i32 + inc) as u32 & 0xFFFF;
                            h = (addr >> 8) as u8;
                            l = addr as u8;
                            flags = (v >> 6 & N_FLAG) | self.sz28(b);
                            // C++ applies the repeat penalty BEFORE the OUT, so
                            // repeated block-output writes carry it in their time
                            if data8 >= 0xB0 && b != 0 {
                                pc = pc.wrapping_sub(2);
                                s_time += 5;
                            }
                            out_port!((b as u16) << 8 | c as u16, v);
                        }

                        // LD I,A / LD R,A
                        0x47 => {
                            self.r.i = a;
                        }
                        0x4F => {
                            self.r.r = a;
                            warning = true;
                        }
                        // LD A,I / LD A,R
                        0x57 => {
                            a = self.r.i;
                            flags = (flags & C_FLAG) | self.sz28(a) | (self.r.iff2 << 2 & P_FLAG);
                        }
                        0x5F => {
                            a = self.r.r;
                            flags = (flags & C_FLAG) | self.sz28(a) | (self.r.iff2 << 2 & P_FLAG);
                            warning = true;
                        }

                        // RETI/RETN
                        0x45 | 0x4D | 0x55 | 0x5D | 0x65 | 0x6D | 0x75 | 0x7D => {
                            self.r.iff1 = self.r.iff2;
                            pc = read_word_mem!(sp);
                            sp = sp.wrapping_add(2);
                        }

                        // IM
                        0x46 | 0x4E | 0x66 | 0x6E => {
                            self.r.im = 0;
                        }
                        0x56 | 0x76 => {
                            self.r.im = 1;
                        }
                        0x5E | 0x7E => {
                            self.r.im = 2;
                        }

                        _ => {
                            // Unknown ED opcode
                            warning = true;
                        }
                    }
                }

                // DD prefix (IX)
                0xDD => {
                    pc = pc.wrapping_add(1);
                    s_time += (ED_DD_TIMING[data8 as usize] & 0x0F) as CpuTime;
                    let data16 = self.read_prog(pc as u16);

                    match data8 {
                        // ADD/ADC/SUB/SBC (IX+disp)
                        0x86 | 0x8E | 0x96 | 0x9E => {
                            pc = pc.wrapping_add(1);
                            let addr = (ix as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr);
                            let sub = data8 >= 0x90;
                            let with_carry = data8 & 0x08 != 0;
                            let carry = if with_carry { flags & C_FLAG } else { 0 };
                            flags = if sub { N_FLAG } else { 0 };
                            let result = if sub {
                                a as i32 - v as i32 - carry as i32
                            } else {
                                a as i32 + v as i32 + carry as i32
                            };
                            let xor = (a ^ v) as i32;
                            let r8 = (result & 0xFF) as u8;
                            flags |= self.sz28(r8);
                            flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                            flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                            if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                                flags |= P_FLAG;
                            }
                            a = r8;
                        }

                        // ADD/ADC/SUB/SBC HXY/LXY
                        0x84 | 0x8C | 0x94 | 0x9C => {
                            let v = (ix >> 8) as u8;
                            let sub = data8 >= 0x90;
                            let with_carry = data8 & 0x08 != 0;
                            let carry = if with_carry { flags & C_FLAG } else { 0 };
                            flags = if sub { N_FLAG } else { 0 };
                            let result = if sub {
                                a as i32 - v as i32 - carry as i32
                            } else {
                                a as i32 + v as i32 + carry as i32
                            };
                            let xor = (a ^ v) as i32;
                            let r8 = (result & 0xFF) as u8;
                            flags |= self.sz28(r8);
                            flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                            flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                            if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                                flags |= P_FLAG;
                            }
                            a = r8;
                        }
                        0x85 | 0x8D | 0x95 | 0x9D => {
                            let v = (ix & 0xFF) as u8;
                            let sub = data8 >= 0x90;
                            let with_carry = data8 & 0x08 != 0;
                            let carry = if with_carry { flags & C_FLAG } else { 0 };
                            flags = if sub { N_FLAG } else { 0 };
                            let result = if sub {
                                a as i32 - v as i32 - carry as i32
                            } else {
                                a as i32 + v as i32 + carry as i32
                            };
                            let xor = (a ^ v) as i32;
                            let r8 = (result & 0xFF) as u8;
                            flags |= self.sz28(r8);
                            flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                            flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                            if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                                flags |= P_FLAG;
                            }
                            a = r8;
                        }

                        // ADD IX,rp
                        0x09 | 0x19 | 0x29 | 0x39 => {
                            let rp_val = match data8 & 0x30 {
                                0x00 => (b as u32) << 8 | c as u32,
                                0x10 => (d as u32) << 8 | e as u32,
                                0x20 => ix,
                                0x30 => sp,
                                _ => 0,
                            };
                            let sum = ix.wrapping_add(rp_val);
                            flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                                | ((sum >> 16) as u8 & C_FLAG)
                                | ((sum >> 8) as u8 & (0x20 | 0x08));
                            ix = sum & 0xFFFF;
                        }

                        // AND/OR/XOR/CP (IX+disp)
                        0xA6 | 0xB6 | 0xAE | 0xBE => {
                            pc = pc.wrapping_add(1);
                            let addr = (ix as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr);
                            match data8 {
                                0xA6 => {
                                    a &= v;
                                    flags = self.sz28p(a) | H_FLAG;
                                }
                                0xB6 => {
                                    a |= v;
                                    flags = self.sz28p(a);
                                }
                                0xAE => {
                                    a ^= v;
                                    flags = self.sz28p(a);
                                }
                                0xBE => {
                                    let result = a as i32 - v as i32;
                                    flags = N_FLAG
                                        | (v & (0x20 | 0x08))
                                        | ((result >> 8) as u8 & C_FLAG);
                                    let xor = (a ^ v) as i32;
                                    flags |=
                                        (((result ^ a as i32) & xor) >> 5 & P_FLAG as i32) as u8;
                                    flags |= (((xor & H_FLAG as i32) ^ result) as u8)
                                        & (S_FLAG | H_FLAG);
                                    if (result & 0xFF) == 0 {
                                        flags |= Z_FLAG;
                                    }
                                }
                                _ => {}
                            }
                        }

                        // AND/OR/XOR/CP HXY/LXY
                        0xA4 | 0xA5 | 0xB4 | 0xB5 | 0xAC | 0xAD | 0xBC | 0xBD => {
                            let v = if data8 & 1 == 0 {
                                (ix >> 8) as u8
                            } else {
                                (ix & 0xFF) as u8
                            };
                            match data8 {
                                0xA4 | 0xA5 => {
                                    a &= v;
                                    flags = self.sz28p(a) | H_FLAG;
                                }
                                0xB4 | 0xB5 => {
                                    a |= v;
                                    flags = self.sz28p(a);
                                }
                                0xAC | 0xAD => {
                                    a ^= v;
                                    flags = self.sz28p(a);
                                }
                                0xBC | 0xBD => {
                                    // CP
                                    let result = a as i32 - v as i32;
                                    flags = N_FLAG
                                        | (v & (0x20 | 0x08))
                                        | ((result >> 8) as u8 & C_FLAG);
                                    let xor = (a ^ v) as i32;
                                    flags |=
                                        (((result ^ a as i32) & xor) >> 5 & P_FLAG as i32) as u8;
                                    flags |= (((xor & H_FLAG as i32) ^ result) as u8)
                                        & (S_FLAG | H_FLAG);
                                    if (result & 0xFF) == 0 {
                                        flags |= Z_FLAG;
                                    }
                                }
                                _ => {}
                            }
                        }

                        // LD (IX+disp),r / LD (IX+disp),imm
                        0x70..=0x75 | 0x77 => {
                            pc = pc.wrapping_add(1);
                            let addr = (ix as i32 + data16 as i8 as i32) as u16;
                            let v = r8!((data8 & 7) as usize);
                            write_mem!(addr, v);
                        }
                        0x36 => {
                            let disp = data16 as i8;
                            pc = pc.wrapping_add(1);
                            let imm = self.read_prog(pc as u16);
                            pc = pc.wrapping_add(1);
                            let addr = (ix as i32 + disp as i32) as u16;
                            write_mem!(addr, imm);
                        }

                        // LD r,(IX+disp)
                        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                            pc = pc.wrapping_add(1);
                            let addr = (ix as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr);
                            let reg_idx = ((data8 >> 3) & 7) as usize;
                            set_r8!(reg_idx, v);
                        }

                        // LD r,HXY/LXY
                        0x44 | 0x4C | 0x54 | 0x5C | 0x7C => {
                            let reg_idx = ((data8 >> 3) & 7) as usize;
                            set_r8!(reg_idx, (ix >> 8) as u8);
                        }
                        0x45 | 0x4D | 0x55 | 0x5D | 0x7D => {
                            let reg_idx = ((data8 >> 3) & 7) as usize;
                            set_r8!(reg_idx, (ix & 0xFF) as u8);
                        }

                        // LD HXY,r / LD LXY,r
                        0x60 | 0x61 | 0x62 | 0x63 | 0x67 => {
                            let v = r8!((data8 & 7) as usize);
                            ix = (ix & 0xFF) | ((v as u32) << 8);
                        }
                        0x64 | 0x6D => {} // LD HXY,HXY / LD LXY,LXY
                        0x65 | 0x68..=0x6B | 0x6F => {
                            let v = r8!((data8 & 7) as usize);
                            ix = (ix & 0xFF00) | v as u32;
                        }

                        // LD HXY,imm / LD LXY,imm
                        0x26 => {
                            pc = pc.wrapping_add(1);
                            ix = (ix & 0xFF) | ((data16 as u32) << 8);
                        }
                        0x2E => {
                            pc = pc.wrapping_add(1);
                            ix = (ix & 0xFF00) | data16 as u32;
                        }

                        // LD IXY,imm
                        0x21 => {
                            ix = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                        }
                        // LD (addr),IXY
                        0x22 => {
                            let addr = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                            write_word_mem!(addr, ix as u16);
                        }
                        // LD IXY,(addr)
                        0x2A => {
                            let addr = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                            ix = read_word_mem!(addr);
                        }
                        // LD SP,IXY
                        0xF9 => {
                            sp = ix;
                        }

                        // PUSH/POP IXY
                        0xE5 => {
                            sp = sp.wrapping_sub(2);
                            write_word_mem!(sp, ix as u16);
                        }
                        0xE1 => {
                            ix = read_word_mem!(sp);
                            sp = sp.wrapping_add(2);
                        }

                        // JP (IXY)
                        0xE9 => {
                            pc = ix;
                        }

                        // EX (SP),IXY
                        0xE3 => {
                            let tmp = read_word_mem!(sp);
                            write_word_mem!(sp, ix as u16);
                            ix = tmp;
                        }

                        // INC/DEC IXY
                        0x23 => {
                            ix = ix.wrapping_add(1) & 0xFFFF;
                        }
                        0x2B => {
                            ix = ix.wrapping_sub(1) & 0xFFFF;
                        }

                        // INC/DEC (IX+disp)
                        0x34 => {
                            pc = pc.wrapping_add(1);
                            let addr = (ix as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr).wrapping_add(1);
                            write_mem!(addr, v);
                            flags = (flags & C_FLAG)
                                | (((v as i32 - 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x80 {
                                flags |= P_FLAG;
                            }
                        }
                        0x35 => {
                            pc = pc.wrapping_add(1);
                            let addr = (ix as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr).wrapping_sub(1);
                            write_mem!(addr, v);
                            flags = (flags & C_FLAG)
                                | N_FLAG
                                | (((v as i32 + 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x7F {
                                flags |= P_FLAG;
                            }
                        }

                        // INC/DEC HXY/LXY
                        0x24 => {
                            ix = (ix & 0xFF) | ((ix.wrapping_add(0x100)) & 0xFF00);
                            flags = (flags & C_FLAG)
                                | ((((ix >> 8) as i32 - 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28((ix >> 8) as u8);
                            if (ix >> 8) == 0x80 {
                                flags |= P_FLAG;
                            }
                        }
                        0x2C => {
                            let v = (ix as u8).wrapping_add(1);
                            ix = (ix & 0xFF00) | v as u32;
                            flags = (flags & C_FLAG)
                                | (((v as i32 - 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x80 {
                                flags |= P_FLAG;
                            }
                        }
                        0x25 => {
                            ix = (ix.wrapping_sub(0x100)) & 0xFFFF | (ix & 0xFF);
                            flags = (flags & C_FLAG)
                                | N_FLAG
                                | ((((ix >> 8) as i32 + 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28((ix >> 8) as u8);
                            if (ix >> 8) == 0x7F {
                                flags |= P_FLAG;
                            }
                        }
                        0x2D => {
                            let v = (ix as u8).wrapping_sub(1);
                            ix = (ix & 0xFF00) | v as u32;
                            flags = (flags & C_FLAG)
                                | N_FLAG
                                | (((v as i32 + 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x7F {
                                flags |= P_FLAG;
                            }
                        }

                        // DD CB prefix
                        0xCB => {
                            let disp = data16 as i8;
                            let addr = (ix as i32 + disp as i32) as u16;
                            pc = pc.wrapping_add(1);
                            let cb_op = self.read_prog(pc as u16);
                            pc = pc.wrapping_add(1);

                            match cb_op {
                                0x06 => {
                                    // RLC
                                    let mut v = read_mem!(addr);
                                    let bit7 = v >> 7;
                                    v = (v << 1) | bit7;
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | (bit7 & C_FLAG);
                                }
                                0x16 => {
                                    // RL
                                    let mut v = read_mem!(addr);
                                    let bit7 = v >> 7;
                                    v = (v << 1) | (flags & C_FLAG);
                                    write_mem!(addr, v);
                                    flags = self.sz28pc(v) | (bit7 & C_FLAG);
                                }
                                0x26 => {
                                    // SLA
                                    let mut v = read_mem!(addr);
                                    let bit7 = v >> 7;
                                    v <<= 1;
                                    write_mem!(addr, v);
                                    flags = self.sz28pc(v) | (bit7 & C_FLAG);
                                }
                                0x36 => {
                                    // SLL
                                    let mut v = read_mem!(addr);
                                    let bit7 = v >> 7;
                                    v = (v << 1) | 1;
                                    write_mem!(addr, v);
                                    flags = self.sz28pc(v) | (bit7 & C_FLAG);
                                }
                                0x0E => {
                                    // RRC
                                    let mut v = read_mem!(addr);
                                    let bit0 = v & C_FLAG;
                                    v = (v >> 1) | (bit0 << 7);
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | bit0;
                                }
                                0x1E => {
                                    // RR
                                    let mut v = read_mem!(addr);
                                    let bit0 = v & C_FLAG;
                                    v = (flags << 7) | (v >> 1);
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | bit0;
                                }
                                0x2E => {
                                    // SRA
                                    let mut v = read_mem!(addr);
                                    let bit0 = v & C_FLAG;
                                    v = (v & 0x80) | (v >> 1);
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | bit0;
                                }
                                0x3E => {
                                    // SRL
                                    let mut v = read_mem!(addr);
                                    let bit0 = v & C_FLAG;
                                    v >>= 1;
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | bit0;
                                }
                                // BIT b,(IX+disp)
                                0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x76 | 0x7E => {
                                    let v = read_mem!(addr);
                                    let bit = (cb_op >> 3) & 7;
                                    let masked = v & (1 << bit);
                                    flags = (flags & C_FLAG)
                                        | H_FLAG
                                        | (masked & S_FLAG)
                                        | if masked == 0 { Z_FLAG | P_FLAG } else { 0 };
                                }
                                // RES b,(IX+disp)
                                0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                                    let v = read_mem!(addr) & !(1 << ((cb_op >> 3) & 7));
                                    write_mem!(addr, v);
                                }
                                // SET b,(IX+disp)
                                0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                                    let v = read_mem!(addr) | (1 << ((cb_op >> 3) & 7));
                                    write_mem!(addr, v);
                                }
                                _ => {
                                    warning = true;
                                }
                            }
                        }

                        _ => {
                            // Unnecessary DD prefix
                            warning = true;
                            pc = pc.wrapping_sub(1);
                        }
                    }
                }

                // FD prefix (IY)
                0xFD => {
                    pc = pc.wrapping_add(1);
                    s_time += (ED_DD_TIMING[data8 as usize] & 0x0F) as CpuTime;
                    let data16 = self.read_prog(pc as u16);

                    match data8 {
                        // ADD/ADC/SUB/SBC (IY+disp)
                        0x86 | 0x8E | 0x96 | 0x9E => {
                            pc = pc.wrapping_add(1);
                            let addr = (iy as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr);
                            let sub = data8 >= 0x90;
                            let with_carry = data8 & 0x08 != 0;
                            let carry = if with_carry { flags & C_FLAG } else { 0 };
                            flags = if sub { N_FLAG } else { 0 };
                            let result = if sub {
                                a as i32 - v as i32 - carry as i32
                            } else {
                                a as i32 + v as i32 + carry as i32
                            };
                            let xor = (a ^ v) as i32;
                            let r8 = (result & 0xFF) as u8;
                            flags |= self.sz28(r8);
                            flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                            flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                            if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                                flags |= P_FLAG;
                            }
                            a = r8;
                        }

                        // ADD/ADC/SUB/SBC HXY/LXY
                        0x84 | 0x8C | 0x94 | 0x9C => {
                            let v = (iy >> 8) as u8;
                            let sub = data8 >= 0x90;
                            let with_carry = data8 & 0x08 != 0;
                            let carry = if with_carry { flags & C_FLAG } else { 0 };
                            flags = if sub { N_FLAG } else { 0 };
                            let result = if sub {
                                a as i32 - v as i32 - carry as i32
                            } else {
                                a as i32 + v as i32 + carry as i32
                            };
                            let xor = (a ^ v) as i32;
                            let r8 = (result & 0xFF) as u8;
                            flags |= self.sz28(r8);
                            flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                            flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                            if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                                flags |= P_FLAG;
                            }
                            a = r8;
                        }
                        0x85 | 0x8D | 0x95 | 0x9D => {
                            let v = (iy & 0xFF) as u8;
                            let sub = data8 >= 0x90;
                            let with_carry = data8 & 0x08 != 0;
                            let carry = if with_carry { flags & C_FLAG } else { 0 };
                            flags = if sub { N_FLAG } else { 0 };
                            let result = if sub {
                                a as i32 - v as i32 - carry as i32
                            } else {
                                a as i32 + v as i32 + carry as i32
                            };
                            let xor = (a ^ v) as i32;
                            let r8 = (result & 0xFF) as u8;
                            flags |= self.sz28(r8);
                            flags |= ((xor ^ result) >> 4 & H_FLAG as i32) as u8;
                            flags |= (((xor ^ result) >> 8) & C_FLAG as i32) as u8;
                            if ((result ^ a as i32) & (result ^ xor) & 0x80) != 0 {
                                flags |= P_FLAG;
                            }
                            a = r8;
                        }

                        // ADD IY,rp
                        0x09 | 0x19 | 0x29 | 0x39 => {
                            let rp_val = match data8 & 0x30 {
                                0x00 => (b as u32) << 8 | c as u32,
                                0x10 => (d as u32) << 8 | e as u32,
                                0x20 => iy,
                                0x30 => sp,
                                _ => 0,
                            };
                            let sum = iy.wrapping_add(rp_val);
                            flags = (flags & (S_FLAG | Z_FLAG | P_FLAG))
                                | ((sum >> 16) as u8 & C_FLAG)
                                | ((sum >> 8) as u8 & (0x20 | 0x08));
                            iy = sum & 0xFFFF;
                        }

                        // AND/OR/XOR/CP (IY+disp)
                        0xA6 | 0xB6 | 0xAE | 0xBE => {
                            pc = pc.wrapping_add(1);
                            let addr = (iy as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr);
                            match data8 {
                                0xA6 => {
                                    a &= v;
                                    flags = self.sz28p(a) | H_FLAG;
                                }
                                0xB6 => {
                                    a |= v;
                                    flags = self.sz28p(a);
                                }
                                0xAE => {
                                    a ^= v;
                                    flags = self.sz28p(a);
                                }
                                0xBE => {
                                    let result = a as i32 - v as i32;
                                    flags = N_FLAG
                                        | (v & (0x20 | 0x08))
                                        | ((result >> 8) as u8 & C_FLAG);
                                    let xor = (a ^ v) as i32;
                                    flags |=
                                        (((result ^ a as i32) & xor) >> 5 & P_FLAG as i32) as u8;
                                    flags |= (((xor & H_FLAG as i32) ^ result) as u8)
                                        & (S_FLAG | H_FLAG);
                                    if (result & 0xFF) == 0 {
                                        flags |= Z_FLAG;
                                    }
                                }
                                _ => {}
                            }
                        }

                        // AND/OR/XOR/CP HXY/LXY
                        0xA4 | 0xA5 | 0xB4 | 0xB5 | 0xAC | 0xAD | 0xBC | 0xBD => {
                            let v = if data8 & 1 == 0 {
                                (iy >> 8) as u8
                            } else {
                                (iy & 0xFF) as u8
                            };
                            match data8 {
                                0xA4 | 0xA5 => {
                                    a &= v;
                                    flags = self.sz28p(a) | H_FLAG;
                                }
                                0xB4 | 0xB5 => {
                                    a |= v;
                                    flags = self.sz28p(a);
                                }
                                0xAC | 0xAD => {
                                    a ^= v;
                                    flags = self.sz28p(a);
                                }
                                0xBC | 0xBD => {
                                    let result = a as i32 - v as i32;
                                    flags = N_FLAG
                                        | (v & (0x20 | 0x08))
                                        | ((result >> 8) as u8 & C_FLAG);
                                    let xor = (a ^ v) as i32;
                                    flags |=
                                        (((result ^ a as i32) & xor) >> 5 & P_FLAG as i32) as u8;
                                    flags |= (((xor & H_FLAG as i32) ^ result) as u8)
                                        & (S_FLAG | H_FLAG);
                                    if (result & 0xFF) == 0 {
                                        flags |= Z_FLAG;
                                    }
                                }
                                _ => {}
                            }
                        }

                        // LD (IY+disp),r / LD (IY+disp),imm
                        0x70..=0x75 | 0x77 => {
                            pc = pc.wrapping_add(1);
                            let addr = (iy as i32 + data16 as i8 as i32) as u16;
                            let v = r8!((data8 & 7) as usize);
                            write_mem!(addr, v);
                        }
                        0x36 => {
                            let disp = data16 as i8;
                            pc = pc.wrapping_add(1);
                            let imm = self.read_prog(pc as u16);
                            pc = pc.wrapping_add(1);
                            let addr = (iy as i32 + disp as i32) as u16;
                            write_mem!(addr, imm);
                        }

                        // LD r,(IY+disp)
                        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                            pc = pc.wrapping_add(1);
                            let addr = (iy as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr);
                            let reg_idx = ((data8 >> 3) & 7) as usize;
                            set_r8!(reg_idx, v);
                        }

                        // LD r,HXY/LXY
                        0x44 | 0x4C | 0x54 | 0x5C | 0x7C => {
                            let reg_idx = ((data8 >> 3) & 7) as usize;
                            set_r8!(reg_idx, (iy >> 8) as u8);
                        }
                        0x45 | 0x4D | 0x55 | 0x5D | 0x7D => {
                            let reg_idx = ((data8 >> 3) & 7) as usize;
                            set_r8!(reg_idx, (iy & 0xFF) as u8);
                        }

                        // LD HXY,r / LD LXY,r
                        0x60 | 0x61 | 0x62 | 0x63 | 0x67 => {
                            let v = r8!((data8 & 7) as usize);
                            iy = (iy & 0xFF) | ((v as u32) << 8);
                        }
                        0x64 | 0x6D => {}
                        0x65 | 0x68..=0x6B | 0x6F => {
                            let v = r8!((data8 & 7) as usize);
                            iy = (iy & 0xFF00) | v as u32;
                        }

                        // LD HXY,imm / LD LXY,imm
                        0x26 => {
                            pc = pc.wrapping_add(1);
                            iy = (iy & 0xFF) | ((data16 as u32) << 8);
                        }
                        0x2E => {
                            pc = pc.wrapping_add(1);
                            iy = (iy & 0xFF00) | data16 as u32;
                        }

                        // LD IXY,imm
                        0x21 => {
                            iy = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                        }
                        // LD (addr),IY
                        0x22 => {
                            let addr = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                            write_word_mem!(addr, iy as u16);
                        }
                        // LD IY,(addr)
                        0x2A => {
                            let addr = read_word_mem!(pc);
                            pc = pc.wrapping_add(2);
                            iy = read_word_mem!(addr);
                        }
                        // LD SP,IY
                        0xF9 => {
                            sp = iy;
                        }

                        // PUSH/POP IY
                        0xE5 => {
                            sp = sp.wrapping_sub(2);
                            write_word_mem!(sp, iy as u16);
                        }
                        0xE1 => {
                            iy = read_word_mem!(sp);
                            sp = sp.wrapping_add(2);
                        }

                        // JP (IY)
                        0xE9 => {
                            pc = iy;
                        }

                        // EX (SP),IY
                        0xE3 => {
                            let tmp = read_word_mem!(sp);
                            write_word_mem!(sp, iy as u16);
                            iy = tmp;
                        }

                        // INC/DEC IY
                        0x23 => {
                            iy = iy.wrapping_add(1) & 0xFFFF;
                        }
                        0x2B => {
                            iy = iy.wrapping_sub(1) & 0xFFFF;
                        }

                        // INC/DEC (IY+disp)
                        0x34 => {
                            pc = pc.wrapping_add(1);
                            let addr = (iy as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr).wrapping_add(1);
                            write_mem!(addr, v);
                            flags = (flags & C_FLAG)
                                | (((v as i32 - 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x80 {
                                flags |= P_FLAG;
                            }
                        }
                        0x35 => {
                            pc = pc.wrapping_add(1);
                            let addr = (iy as i32 + data16 as i8 as i32) as u16;
                            let v = read_mem!(addr).wrapping_sub(1);
                            write_mem!(addr, v);
                            flags = (flags & C_FLAG)
                                | N_FLAG
                                | (((v as i32 + 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x7F {
                                flags |= P_FLAG;
                            }
                        }

                        // INC/DEC HXY/LXY
                        0x24 => {
                            iy = (iy & 0xFF) | ((iy.wrapping_add(0x100)) & 0xFF00);
                            flags = (flags & C_FLAG)
                                | ((((iy >> 8) as i32 - 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28((iy >> 8) as u8);
                            if (iy >> 8) == 0x80 {
                                flags |= P_FLAG;
                            }
                        }
                        0x2C => {
                            let v = (iy as u8).wrapping_add(1);
                            iy = (iy & 0xFF00) | v as u32;
                            flags = (flags & C_FLAG)
                                | (((v as i32 - 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x80 {
                                flags |= P_FLAG;
                            }
                        }
                        0x25 => {
                            iy = (iy.wrapping_sub(0x100)) & 0xFFFF | (iy & 0xFF);
                            flags = (flags & C_FLAG)
                                | N_FLAG
                                | ((((iy >> 8) as i32 + 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28((iy >> 8) as u8);
                            if (iy >> 8) == 0x7F {
                                flags |= P_FLAG;
                            }
                        }
                        0x2D => {
                            let v = (iy as u8).wrapping_sub(1);
                            iy = (iy & 0xFF00) | v as u32;
                            flags = (flags & C_FLAG)
                                | N_FLAG
                                | (((v as i32 + 1) & 0x0F) as u8 & H_FLAG)
                                | self.sz28(v);
                            if v == 0x7F {
                                flags |= P_FLAG;
                            }
                        }

                        // FD CB prefix
                        0xCB => {
                            let disp = data16 as i8;
                            let addr = (iy as i32 + disp as i32) as u16;
                            pc = pc.wrapping_add(1);
                            let cb_op = self.read_prog(pc as u16);
                            pc = pc.wrapping_add(1);

                            match cb_op {
                                0x06 => {
                                    let mut v = read_mem!(addr);
                                    let b7 = v >> 7;
                                    v = (v << 1) | b7;
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | (b7 & C_FLAG);
                                }
                                0x16 => {
                                    let mut v = read_mem!(addr);
                                    let b7 = v >> 7;
                                    v = (v << 1) | (flags & C_FLAG);
                                    write_mem!(addr, v);
                                    flags = self.sz28pc(v) | (b7 & C_FLAG);
                                }
                                0x26 => {
                                    let mut v = read_mem!(addr);
                                    let b7 = v >> 7;
                                    v <<= 1;
                                    write_mem!(addr, v);
                                    flags = self.sz28pc(v) | (b7 & C_FLAG);
                                }
                                0x36 => {
                                    let mut v = read_mem!(addr);
                                    let b7 = v >> 7;
                                    v = (v << 1) | 1;
                                    write_mem!(addr, v);
                                    flags = self.sz28pc(v) | (b7 & C_FLAG);
                                }
                                0x0E => {
                                    let mut v = read_mem!(addr);
                                    let b0 = v & C_FLAG;
                                    v = (v >> 1) | (b0 << 7);
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | b0;
                                }
                                0x1E => {
                                    let mut v = read_mem!(addr);
                                    let b0 = v & C_FLAG;
                                    v = (flags << 7) | (v >> 1);
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | b0;
                                }
                                0x2E => {
                                    let mut v = read_mem!(addr);
                                    let b0 = v & C_FLAG;
                                    v = (v & 0x80) | (v >> 1);
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | b0;
                                }
                                0x3E => {
                                    let mut v = read_mem!(addr);
                                    let b0 = v & C_FLAG;
                                    v >>= 1;
                                    write_mem!(addr, v);
                                    flags = self.sz28p(v) | b0;
                                }
                                0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x76 | 0x7E => {
                                    let v = read_mem!(addr);
                                    let bit = (cb_op >> 3) & 7;
                                    let masked = v & (1 << bit);
                                    flags = (flags & C_FLAG)
                                        | H_FLAG
                                        | (masked & S_FLAG)
                                        | if masked == 0 { Z_FLAG | P_FLAG } else { 0 };
                                }
                                0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                                    let v = read_mem!(addr) & !(1 << ((cb_op >> 3) & 7));
                                    write_mem!(addr, v);
                                }
                                0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                                    let v = read_mem!(addr) | (1 << ((cb_op >> 3) & 7));
                                    write_mem!(addr, v);
                                }
                                _ => {
                                    warning = true;
                                }
                            }
                        }

                        _ => {
                            warning = true;
                            pc = pc.wrapping_sub(1);
                        }
                    }
                }
            }
        }

        // out_of_time: save state back
        self.state.time = s_time;
        self.r.bregs[6] = a;
        self.r.bregs[7] = flags;
        self.r.bregs[1] = b;
        self.r.bregs[0] = c;
        self.r.bregs[3] = d;
        self.r.bregs[2] = e;
        self.r.bregs[5] = h;
        self.r.bregs[4] = l;
        self.r.pc = pc as u16;
        self.r.sp = sp as u16;
        self.r.ix = ix as u16;
        self.r.iy = iy as u16;

        warning
    }
}
