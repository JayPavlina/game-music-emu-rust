// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2006 Shay Green
// Rust adaptation of Ay_Cpu.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! Z80 CPU emulator for the AY format.
//! Port of C++ Ay_Cpu.h / Ay_Cpu.cpp (NOT the same core as Kss_Cpu: this one
//! uses a flat 64K memory pointer and no read/write hooks; only IN/OUT are
//! dispatched to the emulator).
use gme_core::blip_buffer::BlipTime;

pub type CpuTime = BlipTime;

/// CPU can read this far past end of memory.
pub const CPU_PADDING: usize = 0x100;

// Flags, named with hex value for clarity (i32 to match C++ int arithmetic).
const S80: i32 = 0x80;
const Z40: i32 = 0x40;
const F20: i32 = 0x20;
const H10: i32 = 0x10;
const F08: i32 = 0x08;
const V04: i32 = 0x04;
const P04: i32 = 0x04;
const N02: i32 = 0x02;
const C01: i32 = 0x01;

/// Z80 registers. Mirrors C++ Ay_Cpu::registers_t with the byte registers
/// split into named fields instead of a union.
#[derive(Default, Clone)]
pub struct Registers {
    pub pc: u16,
    pub sp: u16,
    pub ix: u16,
    pub iy: u16,
    pub a: u8,
    pub flags: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub alt_a: u8,
    pub alt_flags: u8,
    pub alt_b: u8,
    pub alt_c: u8,
    pub alt_d: u8,
    pub alt_e: u8,
    pub alt_h: u8,
    pub alt_l: u8,
    pub iff1: u8,
    pub iff2: u8,
    pub r: u8,
    pub i: u8,
    pub im: u8,
}

impl Registers {
    pub fn hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }
}

// High four bits are $ED time - 8, low four bits are $DD/$FD time - 8.
#[rustfmt::skip]
const ED_DD_TIMING: [u8; 0x100] = [
//  0    1    2    3    4    5    6    7    8    9    A    B    C    D    E    F
0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x07,0x00,0x00,0x00,0x00,0x00,0x00,
0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x07,0x00,0x00,0x00,0x00,0x00,0x00,
0x00,0x06,0x0C,0x02,0x00,0x00,0x03,0x00,0x00,0x07,0x0C,0x02,0x00,0x00,0x03,0x00,
0x00,0x00,0x00,0x00,0x0F,0x0F,0x0B,0x00,0x00,0x07,0x00,0x00,0x00,0x00,0x00,0x00,
0x40,0x40,0x70,0xC0,0x00,0x60,0x0B,0x10,0x40,0x40,0x70,0xC0,0x00,0x60,0x0B,0x10,
0x40,0x40,0x70,0xC0,0x00,0x60,0x0B,0x10,0x40,0x40,0x70,0xC0,0x00,0x60,0x0B,0x10,
0x40,0x40,0x70,0xC0,0x00,0x60,0x0B,0xA0,0x40,0x40,0x70,0xC0,0x00,0x60,0x0B,0xA0,
0x4B,0x4B,0x7B,0xCB,0x0B,0x6B,0x00,0x0B,0x40,0x40,0x70,0xC0,0x00,0x60,0x0B,0x00,
0x00,0x00,0x00,0x00,0x00,0x00,0x0B,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x0B,0x00,
0x00,0x00,0x00,0x00,0x00,0x00,0x0B,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x0B,0x00,
0x80,0x80,0x80,0x80,0x00,0x00,0x0B,0x00,0x80,0x80,0x80,0x80,0x00,0x00,0x0B,0x00,
0xD0,0xD0,0xD0,0xD0,0x00,0x00,0x0B,0x00,0xD0,0xD0,0xD0,0xD0,0x00,0x00,0x0B,0x00,
0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x0F,0x00,0x00,0x00,0x00,
0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
0x00,0x06,0x00,0x0F,0x00,0x07,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x02,0x00,0x00,0x00,0x00,0x00,0x00,
];

#[rustfmt::skip]
const BASE_TIMING: [u8; 0x100] = [
//   0  1  2  3  4  5  6  7  8  9  A  B  C  D  E  F
     4,10, 7, 6, 4, 4, 7, 4, 4,11, 7, 6, 4, 4, 7, 4, // 0
    13,10, 7, 6, 4, 4, 7, 4,12,11, 7, 6, 4, 4, 7, 4, // 1
    12,10,16, 6, 4, 4, 7, 4,12,11,16, 6, 4, 4, 7, 4, // 2
    12,10,13, 6,11,11,10, 4,12,11,13, 6, 4, 4, 7, 4, // 3
     4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, // 4
     4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, // 5
     4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, // 6
     7, 7, 7, 7, 7, 7, 4, 7, 4, 4, 4, 4, 4, 4, 7, 4, // 7
     4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, // 8
     4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, // 9
     4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, // A
     4, 4, 4, 4, 4, 4, 7, 4, 4, 4, 4, 4, 4, 4, 7, 4, // B
    11,10,10,10,17,11, 7,11,11,10,10, 8,17,17, 7,11, // C
    11,10,10,11,17,11, 7,11,11, 4,10,11,17, 8, 7,11, // D
    11,10,10,19,17,11, 7,11,11, 4,10, 4,17, 8, 7,11, // E
    11,10,10, 4,17,11, 7,11,11, 6,10, 4,17, 8, 7,11, // F
];

pub struct AyCpu {
    szpc: [u8; 0x200],
    mem: *mut u8,
    base: CpuTime,
    time: CpuTime,
    pub r: Registers,
}

// SAFETY: AyCpu only accesses memory through the pointer set by the owning
// emulator, which points at memory owned by that emulator.
unsafe impl Send for AyCpu {}

impl Default for AyCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl AyCpu {
    pub fn new() -> Self {
        let mut szpc = [0u8; 0x200];
        // Matches C++ Ay_Cpu ctor table generation.
        for i in (0..0x100usize).rev() {
            let mut even: i32 = 1;
            let mut p = i as i32;
            while p != 0 {
                even ^= p;
                p >>= 1;
            }
            let n = (i as i32 & (S80 | F20 | F08)) | ((even & 1) * P04);
            szpc[i] = n as u8;
            szpc[i + 0x100] = (n | C01) as u8;
        }
        szpc[0x000] |= Z40 as u8;
        szpc[0x100] |= Z40 as u8;

        Self {
            szpc,
            mem: std::ptr::null_mut(),
            base: 0,
            time: 0,
            r: Registers::default(),
        }
    }

    /// Clear all registers and keep pointer to 64K memory passed in.
    /// Memory must have CPU_PADDING bytes past the 64K boundary.
    pub fn reset(&mut self, mem_64k: *mut u8) {
        self.mem = mem_64k;
        self.time = 0;
        self.base = 0;
        self.r = Registers::default();
    }

    /// Update the memory pointer without resetting (used to refresh after the
    /// owning emulator struct may have moved).
    pub fn set_mem(&mut self, mem_64k: *mut u8) {
        self.mem = mem_64k;
    }

    /// Time of beginning of next instruction.
    pub fn time(&self) -> CpuTime {
        self.time + self.base
    }

    /// Alter current time. Not supported during run() call.
    pub fn set_time(&mut self, t: CpuTime) {
        self.time = t - self.base;
    }

    pub fn adjust_time(&mut self, delta: CpuTime) {
        self.time += delta;
    }

    /// Run until specified time is reached. Returns true if suspicious or
    /// unsupported instruction was encountered at any point during run.
    pub fn run(
        &mut self,
        end_time: CpuTime,
        cpu_out: &mut impl FnMut(CpuTime, u32, u8),
        cpu_in: &mut impl FnMut(u32) -> u8,
    ) -> bool {
        // set_end_time( end_time )
        let delta = self.base - end_time;
        self.base = end_time;
        self.time += delta;

        let s_base = self.base;
        let mut s_time = self.time;
        let mut warning = false;

        let mem = self.mem;
        let szpc = &self.szpc;

        let mut pc = self.r.pc;
        let mut sp = self.r.sp;
        let mut ix = self.r.ix;
        let mut iy = self.r.iy;
        let mut a = self.r.a;
        let mut b = self.r.b;
        let mut c = self.r.c;
        let mut d = self.r.d;
        let mut e = self.r.e;
        let mut h = self.r.h;
        let mut l = self.r.l;
        let mut flags = self.r.flags as i32;
        // The C++ register union exposes the in-run copy of `flags` as 8-bit
        // register slot 6 (only reachable from a few ED-prefix opcodes); it is
        // overwritten with `flags` at exit, so it acts as a scratch register.
        let mut flags_slot: u8 = self.r.flags;

        // ---- memory access helpers (no hooks; flat 64K + padding) ----
        macro_rules! rd {
            ($addr:expr) => {
                unsafe { *mem.add(($addr) as usize) }
            };
        }
        macro_rules! wr {
            ($addr:expr, $data:expr) => {
                unsafe { *mem.add(($addr) as usize) = ($data) as u8 }
            };
        }
        // C++ GET_LE16( &mem [addr] ): no 64K wrap for the second byte.
        macro_rules! rd_word {
            ($addr:expr) => {{
                let addr = ($addr) as usize;
                unsafe { (*mem.add(addr) as u16) | ((*mem.add(addr + 1) as u16) << 8) }
            }};
        }
        macro_rules! wr_word {
            ($addr:expr, $data:expr) => {{
                let addr = ($addr) as usize;
                let data = ($data) as u16;
                unsafe {
                    *mem.add(addr) = data as u8;
                    *mem.add(addr + 1) = (data >> 8) as u8;
                }
            }};
        }
        macro_rules! out_port {
            ($addr:expr, $data:expr) => {
                cpu_out(s_time + s_base, ($addr) as u32, ($data) as u8)
            };
        }
        macro_rules! in_port {
            ($addr:expr) => {
                cpu_in(($addr) as u32)
            };
        }

        // ---- register helpers ----
        // 8-bit register slot, C++ R8(n, 0): 0=B 1=C 2=D 3=E 4=H 5=L 6=flags 7=A
        macro_rules! r8 {
            ($idx:expr) => {
                match ($idx) & 7 {
                    0 => b,
                    1 => c,
                    2 => d,
                    3 => e,
                    4 => h,
                    5 => l,
                    6 => flags_slot,
                    _ => a,
                }
            };
        }
        macro_rules! set_r8 {
            ($idx:expr, $val:expr) => {{
                let v = ($val) as u8;
                match ($idx) & 7 {
                    0 => b = v,
                    1 => c = v,
                    2 => d = v,
                    3 => e = v,
                    4 => h = v,
                    5 => l = v,
                    6 => flags_slot = v,
                    _ => a = v,
                }
            }};
        }
        // 16-bit register pair: 0=BC 1=DE 2=HL
        macro_rules! rp {
            ($idx:expr) => {
                match $idx {
                    0 => (b as u16) << 8 | c as u16,
                    1 => (d as u16) << 8 | e as u16,
                    _ => (h as u16) << 8 | l as u16,
                }
            };
        }
        macro_rules! set_rp {
            ($idx:expr, $val:expr) => {{
                let v = ($val) as u16;
                match $idx {
                    0 => {
                        b = (v >> 8) as u8;
                        c = v as u8;
                    }
                    1 => {
                        d = (v >> 8) as u8;
                        e = v as u8;
                    }
                    _ => {
                        h = (v >> 8) as u8;
                        l = v as u8;
                    }
                }
            }};
        }
        macro_rules! bc {
            () => {
                (b as u16) << 8 | c as u16
            };
        }
        macro_rules! de {
            () => {
                (d as u16) << 8 | e as u16
            };
        }
        macro_rules! hl {
            () => {
                (h as u16) << 8 | l as u16
            };
        }

        macro_rules! sz28p {
            ($n:expr) => {
                szpc[($n) as usize] as i32
            };
        }
        macro_rules! sz28pc {
            ($n:expr) => {
                szpc[($n) as usize] as i32
            };
        }
        macro_rules! sz28c {
            ($n:expr) => {
                (szpc[($n) as usize] as i32 & !P04)
            };
        }
        macro_rules! sz28 {
            ($n:expr) => {
                sz28c!($n)
            };
        }

        // ---- shared instruction tails (C++ goto targets) ----
        // adc_data: shared ADD/ADC/SUB/SBC core. $op is the C++ `opcode`
        // value at the time of the jump; bit 1 of (op >> 3) selects subtract.
        macro_rules! adc_data {
            ($op:expr, $data:expr) => {{
                let mut data16: u16 = ($data) as u16;
                let mut result: i32 = data16 as i32 + (flags & C01);
                data16 ^= a as u16;
                flags = ($op as i32) >> 3 & N02; // bit set in subtract opcodes
                if flags != 0 {
                    result = -result;
                }
                result += a as i32;
                data16 ^= result as u16;
                flags |= (data16 as i32 & H10)
                    | ((data16 as i32 + 0x80) >> 6 & V04)
                    | sz28c!(result & 0x1FF);
                a = result as u8;
            }};
        }

        macro_rules! cp_data {
            ($data:expr) => {{
                let mut data16: u16 = ($data) as u16;
                let result: i32 = a as i32 - data16 as i32;
                flags = N02 | (data16 as i32 & (F20 | F08)) | (result >> 8 & C01);
                data16 ^= a as u16;
                flags |= ((result ^ a as i32) & data16 as i32) >> 5 & V04;
                flags |= ((data16 as i32 & H10) ^ result) & (S80 | H10);
                if result as u8 == 0 {
                    flags |= Z40;
                }
            }};
        }

        macro_rules! and_data {
            ($data:expr) => {{
                a &= ($data) as u8;
                flags = sz28p!(a) | H10;
            }};
        }
        macro_rules! or_data {
            ($data:expr) => {{
                a |= ($data) as u8;
                flags = sz28p!(a);
            }};
        }
        macro_rules! xor_data {
            ($data:expr) => {{
                a ^= ($data) as u8;
                flags = sz28p!(a);
            }};
        }

        macro_rules! add_hl_data {
            ($data:expr) => {{
                let mut data16: u16 = ($data) as u16;
                let sum: u32 = hl!() as u32 + data16 as u32;
                data16 ^= hl!();
                h = (sum >> 8) as u8;
                l = sum as u8;
                flags = (flags & (S80 | Z40 | V04))
                    | (sum >> 16) as i32
                    | ((sum >> 8) as i32 & (F20 | F08))
                    | (((data16 as u32 ^ sum) >> 8) as i32 & H10);
            }};
        }

        // $data may exceed 8 bits for the (HL)/(IXY) variants, matching the
        // C++ uint16_t `data` temp.
        macro_rules! inc_set_flags {
            ($data:expr) => {{
                let data16: u16 = $data;
                flags = (flags & C01) | (((data16 as i32 & 0x0F) - 1) & H10) | sz28!(data16 as u8);
                if data16 == 0x80 {
                    flags |= V04;
                }
            }};
        }
        macro_rules! dec_set_flags {
            ($data:expr) => {{
                let data16: u16 = $data;
                flags = (flags & C01)
                    | N02
                    | (((data16 as i32 & 0x0F) + 1) & H10)
                    | sz28!(data16 as u8);
                if data16 == 0x7F {
                    flags |= V04;
                }
            }};
        }

        macro_rules! push16 {
            ($data:expr) => {{
                sp = sp.wrapping_sub(2);
                wr_word!(sp, $data);
            }};
        }
        macro_rules! ret_taken {
            () => {{
                pc = rd_word!(sp);
                sp = sp.wrapping_add(2);
            }};
        }

        macro_rules! jr {
            ($cond:expr, $data:expr) => {{
                let disp = ($data) as i8 as i32;
                pc = pc.wrapping_add(1);
                if $cond {
                    pc = (pc as i32 + disp) as u16;
                } else {
                    s_time -= 5;
                }
            }};
        }
        macro_rules! jp {
            ($cond:expr) => {{
                if $cond {
                    pc = rd_word!(pc);
                } else {
                    pc = pc.wrapping_add(2);
                }
            }};
        }
        macro_rules! ret {
            ($cond:expr) => {{
                if $cond {
                    ret_taken!();
                } else {
                    s_time -= 6;
                }
            }};
        }
        macro_rules! call {
            ($cond:expr) => {{
                if $cond {
                    let ret_addr = pc.wrapping_add(2);
                    pc = rd_word!(pc);
                    push16!(ret_addr);
                } else {
                    s_time -= 7;
                    pc = pc.wrapping_add(2);
                }
            }};
        }

        // ---- CB-prefix rotate/shift cores, addressed form (shared with DD CB) ----
        macro_rules! rlc_addr {
            ($addr:expr) => {{
                let addr = $addr;
                let v = rd!(addr);
                let result = v.rotate_left(1);
                flags = sz28p!(result) | (result as i32 & C01);
                wr!(addr, result);
            }};
        }
        macro_rules! rl_addr {
            ($addr:expr) => {{
                let addr = $addr;
                let result: u16 = ((rd!(addr) as u16) << 1) | (flags & C01) as u16;
                flags = sz28pc!(result);
                wr!(addr, result);
            }};
        }
        macro_rules! sla_addr {
            ($addr:expr, $add:expr) => {{
                let addr = $addr;
                let result: u16 = ((rd!(addr) as u16) << 1) | $add;
                flags = sz28pc!(result);
                wr!(addr, result);
            }};
        }
        macro_rules! rrc_addr {
            ($addr:expr) => {{
                let addr = $addr;
                let v = rd!(addr);
                flags = v as i32 & C01;
                let result = v.rotate_right(1);
                flags |= sz28p!(result);
                wr!(addr, result);
            }};
        }
        macro_rules! rr_addr {
            ($addr:expr) => {{
                let addr = $addr;
                let v = rd!(addr);
                let temp = v as i32 & C01;
                let result = ((flags << 7) as u8) | (v >> 1);
                flags = sz28p!(result) | temp;
                wr!(addr, result);
            }};
        }
        macro_rules! sra_addr {
            ($addr:expr) => {{
                let addr = $addr;
                let v = rd!(addr);
                flags = v as i32 & C01;
                let result = (v & 0x80) | (v >> 1);
                flags |= sz28p!(result);
                wr!(addr, result);
            }};
        }
        macro_rules! srl_addr {
            ($addr:expr) => {{
                let addr = $addr;
                let v = rd!(addr);
                flags = v as i32 & C01;
                let result = v >> 1;
                flags |= sz28p!(result);
                wr!(addr, result);
            }};
        }

        enum Exit {
            OutOfTime,
            Halt,
        }

        let exit = 'run: loop {
            debug_assert!((sp as u32) < 0x10000);
            debug_assert!((flags as u32) < 0x100);

            let opcode = rd!(pc);
            pc = pc.wrapping_add(1);

            let t = BASE_TIMING[opcode as usize] as CpuTime;
            s_time += t;
            if s_time >= 0 {
                // possibly_out_of_time
                if s_time >= t {
                    s_time -= t;
                    break 'run Exit::OutOfTime;
                }
                // else: almost out of time; execute this instruction
            }

            let data = rd!(pc);

            match opcode {
                // Common
                0x00 | 0x40 | 0x49 | 0x52 | 0x5B | 0x64 | 0x6D | 0x7F => {} // NOP, LD r,r

                0x08 => {
                    // EX AF,AF'
                    std::mem::swap(&mut self.r.alt_a, &mut a);
                    let temp = self.r.alt_flags as i32;
                    self.r.alt_flags = flags as u8;
                    flags = temp;
                }

                0xD3 => {
                    // OUT (imm),A
                    pc = pc.wrapping_add(1);
                    out_port!(data as u32 + a as u32 * 0x100, a);
                }

                0x2E => {
                    // LD L,imm
                    pc = pc.wrapping_add(1);
                    l = data;
                }

                0x3E => {
                    // LD A,imm
                    pc = pc.wrapping_add(1);
                    a = data;
                }

                0x3A => {
                    // LD A,(addr)
                    let addr = rd_word!(pc);
                    pc = pc.wrapping_add(2);
                    a = rd!(addr);
                }

                // JR
                0x20 => jr!(flags & Z40 == 0, data),
                0x28 => jr!(flags & Z40 != 0, data),
                0x30 => jr!(flags & C01 == 0, data),
                0x38 => jr!(flags & C01 != 0, data),
                0x18 => jr!(true, data),

                0x10 => {
                    // DJNZ disp
                    let temp = b as i32 - 1;
                    b = temp as u8;
                    jr!(temp != 0, data);
                }

                // JP
                0xC2 => jp!(flags & Z40 == 0),
                0xCA => jp!(flags & Z40 != 0),
                0xD2 => jp!(flags & C01 == 0),
                0xDA => jp!(flags & C01 != 0),
                0xE2 => jp!(flags & P04 == 0),
                0xEA => jp!(flags & P04 != 0),
                0xF2 => jp!(flags & S80 == 0),
                0xFA => jp!(flags & S80 != 0),

                0xC3 => pc = rd_word!(pc), // JP addr
                0xE9 => pc = hl!(),        // JP HL

                // RET
                0xC0 => ret!(flags & Z40 == 0),
                0xC8 => ret!(flags & Z40 != 0),
                0xD0 => ret!(flags & C01 == 0),
                0xD8 => ret!(flags & C01 != 0),
                0xE0 => ret!(flags & P04 == 0),
                0xE8 => ret!(flags & P04 != 0),
                0xF0 => ret!(flags & S80 == 0),
                0xF8 => ret!(flags & S80 != 0),
                0xC9 => ret_taken!(), // RET

                // CALL
                0xC4 => call!(flags & Z40 == 0),
                0xCC => call!(flags & Z40 != 0),
                0xD4 => call!(flags & C01 == 0),
                0xDC => call!(flags & C01 != 0),
                0xE4 => call!(flags & P04 == 0),
                0xEC => call!(flags & P04 != 0),
                0xF4 => call!(flags & S80 == 0),
                0xFC => call!(flags & S80 != 0),
                0xCD => call!(true), // CALL addr

                // RST (the C++ `(pc - 1) > 0xFFFF` idle check is always false
                // because pc is uint16_t, so 0xFF behaves like the others)
                0xFF | 0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 => {
                    let data16 = pc;
                    pc = (opcode & 0x38) as u16;
                    push16!(data16);
                }

                // PUSH/POP
                0xF5 => push16!(a as u16 * 0x100 + flags as u16), // PUSH AF
                0xC5 | 0xD5 | 0xE5 => {
                    // PUSH rp
                    let data16 = rp!(((opcode - 0xC5) >> 4) as usize);
                    push16!(data16);
                }

                0xF1 => {
                    // POP AF
                    flags = rd!(sp) as i32;
                    a = rd!(sp as u32 + 1);
                    sp = sp.wrapping_add(2);
                }
                0xC1 | 0xD1 | 0xE1 => {
                    // POP rp
                    let v = rd_word!(sp);
                    set_rp!(((opcode - 0xC1) >> 4) as usize, v);
                    sp = sp.wrapping_add(2);
                }

                // ADC/ADD/SBC/SUB
                0x96 | 0x86 => {
                    // SUB/ADD (HL)
                    flags &= !C01;
                    let v = rd!(hl!());
                    adc_data!(opcode, v);
                }
                0x9E | 0x8E => {
                    // SBC/ADC (HL)
                    let v = rd!(hl!());
                    adc_data!(opcode, v);
                }
                0xD6 | 0xC6 => {
                    // SUB/ADD imm
                    flags &= !C01;
                    pc = pc.wrapping_add(1);
                    adc_data!(opcode, data);
                }
                0xDE | 0xCE => {
                    // SBC/ADC imm
                    pc = pc.wrapping_add(1);
                    adc_data!(opcode, data);
                }
                0x90..=0x95 | 0x97 | 0x80..=0x85 | 0x87 => {
                    // SUB/ADD r
                    flags &= !C01;
                    let v = r8!(opcode & 7);
                    adc_data!(opcode, v);
                }
                0x98..=0x9D | 0x9F | 0x88..=0x8D | 0x8F => {
                    // SBC/ADC r
                    let v = r8!(opcode & 7);
                    adc_data!(opcode, v);
                }

                // CP
                0xBE => {
                    let v = rd!(hl!());
                    cp_data!(v);
                }
                0xFE => {
                    pc = pc.wrapping_add(1);
                    cp_data!(data);
                }
                0xB8..=0xBD | 0xBF => {
                    let v = r8!(opcode & 7);
                    cp_data!(v);
                }

                // ADD HL,rp
                0x39 => add_hl_data!(sp),
                0x09 | 0x19 | 0x29 => {
                    let v = rp!(((opcode - 0x09) >> 4) as usize);
                    add_hl_data!(v);
                }

                0x27 => {
                    // DAA
                    let mut aa = a as i32;
                    if aa > 0x99 {
                        flags |= C01;
                    }
                    let mut adjust = 0x60 & -(flags & C01);
                    if flags & H10 != 0 || (aa & 0x0F) > 9 {
                        adjust |= 0x06;
                    }
                    if flags & N02 != 0 {
                        adjust = -adjust;
                    }
                    aa += adjust;
                    flags = (flags & (C01 | N02)) | ((a as i32 ^ aa) & H10) | sz28p!(aa as u8);
                    a = aa as u8;
                }

                // INC/DEC
                0x34 => {
                    // INC (HL)
                    let addr = hl!();
                    let data16 = rd!(addr) as u16 + 1;
                    wr!(addr, data16);
                    inc_set_flags!(data16);
                }
                0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => {
                    // INC r
                    let idx = (opcode >> 3) & 7;
                    let v = r8!(idx).wrapping_add(1);
                    set_r8!(idx, v);
                    inc_set_flags!(v as u16);
                }
                0x35 => {
                    // DEC (HL)
                    let addr = hl!();
                    let data16 = (rd!(addr) as u16).wrapping_sub(1);
                    wr!(addr, data16);
                    dec_set_flags!(data16);
                }
                0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => {
                    // DEC r
                    let idx = (opcode >> 3) & 7;
                    let v = r8!(idx).wrapping_sub(1);
                    set_r8!(idx, v);
                    dec_set_flags!(v as u16);
                }

                0x03 | 0x13 | 0x23 => {
                    // INC rp
                    let idx = ((opcode - 0x03) >> 4) as usize;
                    let v = rp!(idx).wrapping_add(1);
                    set_rp!(idx, v);
                }
                0x33 => sp = sp.wrapping_add(1), // INC SP
                0x0B | 0x1B | 0x2B => {
                    // DEC rp
                    let idx = ((opcode - 0x0B) >> 4) as usize;
                    let v = rp!(idx).wrapping_sub(1);
                    set_rp!(idx, v);
                }
                0x3B => sp = sp.wrapping_sub(1), // DEC SP

                // AND
                0xA6 => {
                    let v = rd!(hl!());
                    and_data!(v);
                }
                0xE6 => {
                    pc = pc.wrapping_add(1);
                    and_data!(data);
                }
                0xA0..=0xA5 | 0xA7 => {
                    let v = r8!(opcode & 7);
                    and_data!(v);
                }

                // OR
                0xB6 => {
                    let v = rd!(hl!());
                    or_data!(v);
                }
                0xF6 => {
                    pc = pc.wrapping_add(1);
                    or_data!(data);
                }
                0xB0..=0xB5 | 0xB7 => {
                    let v = r8!(opcode & 7);
                    or_data!(v);
                }

                // XOR
                0xAE => {
                    let v = rd!(hl!());
                    xor_data!(v);
                }
                0xEE => {
                    pc = pc.wrapping_add(1);
                    xor_data!(data);
                }
                0xA8..=0xAD | 0xAF => {
                    let v = r8!(opcode & 7);
                    xor_data!(v);
                }

                // LD
                0x70..=0x75 | 0x77 => {
                    // LD (HL),r
                    let v = r8!(opcode & 7);
                    wr!(hl!(), v);
                }
                0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                    // LD r,(HL)
                    let v = rd!(hl!());
                    set_r8!((opcode >> 3) & 7, v);
                }
                0x41..=0x45
                | 0x47
                | 0x48
                | 0x4A..=0x4D
                | 0x4F
                | 0x50
                | 0x51
                | 0x53..=0x55
                | 0x57
                | 0x58..=0x5A
                | 0x5C
                | 0x5D
                | 0x5F
                | 0x60..=0x63
                | 0x65
                | 0x67
                | 0x68..=0x6B
                | 0x6C
                | 0x6F
                | 0x78..=0x7D => {
                    // LD r,r
                    let v = r8!(opcode & 7);
                    set_r8!((opcode >> 3) & 7, v);
                }

                0x06 | 0x0E | 0x16 | 0x1E | 0x26 => {
                    // LD r,imm
                    set_r8!((opcode >> 3) & 7, data);
                    pc = pc.wrapping_add(1);
                }

                0x36 => {
                    // LD (HL),imm
                    pc = pc.wrapping_add(1);
                    wr!(hl!(), data);
                }

                0x01 | 0x11 | 0x21 => {
                    // LD rp,imm
                    let v = rd_word!(pc);
                    set_rp!(((opcode - 0x01) >> 4) as usize, v);
                    pc = pc.wrapping_add(2);
                }
                0x31 => {
                    // LD SP,imm
                    sp = rd_word!(pc);
                    pc = pc.wrapping_add(2);
                }

                0x2A => {
                    // LD HL,(addr)
                    let addr = rd_word!(pc);
                    pc = pc.wrapping_add(2);
                    let v = rd_word!(addr);
                    h = (v >> 8) as u8;
                    l = v as u8;
                }

                0x32 => {
                    // LD (addr),A
                    let addr = rd_word!(pc);
                    pc = pc.wrapping_add(2);
                    wr!(addr, a);
                }

                0x22 => {
                    // LD (addr),HL
                    let addr = rd_word!(pc);
                    pc = pc.wrapping_add(2);
                    wr_word!(addr, hl!());
                }

                0x02 => wr!(bc!(), a),  // LD (BC),A
                0x12 => wr!(de!(), a),  // LD (DE),A
                0x0A => a = rd!(bc!()), // LD A,(BC)
                0x1A => a = rd!(de!()), // LD A,(DE)

                0xF9 => sp = hl!(), // LD SP,HL

                // Rotate
                0x07 => {
                    // RLCA
                    let temp = a as u32;
                    let temp = (temp << 1) | (temp >> 7);
                    flags = (flags & (S80 | Z40 | P04)) | (temp as i32 & (F20 | F08 | C01));
                    a = temp as u8;
                }
                0x0F => {
                    // RRCA
                    let temp = a as u32;
                    flags = (flags & (S80 | Z40 | P04)) | (temp as i32 & C01);
                    let temp = (temp << 7) | (temp >> 1);
                    flags |= temp as i32 & (F20 | F08);
                    a = temp as u8;
                }
                0x17 => {
                    // RLA
                    let temp: u32 = ((a as u32) << 1) | (flags & C01) as u32;
                    flags = (flags & (S80 | Z40 | P04))
                        | (temp as i32 & (F20 | F08))
                        | (temp >> 8) as i32;
                    a = temp as u8;
                }
                0x1F => {
                    // RRA
                    let temp: u32 = ((flags as u32) << 7) | ((a as u32) >> 1);
                    flags = (flags & (S80 | Z40 | P04))
                        | (temp as i32 & (F20 | F08))
                        | (a as i32 & C01);
                    a = temp as u8;
                }

                // Misc
                0x2F => {
                    // CPL
                    let temp = !a;
                    flags = (flags & (S80 | Z40 | P04 | C01))
                        | (temp as i32 & (F20 | F08))
                        | (H10 | N02);
                    a = temp;
                }
                0x3F => {
                    // CCF
                    flags = ((flags & (S80 | Z40 | P04 | C01)) ^ C01)
                        | (flags << 4 & H10)
                        | (a as i32 & (F20 | F08));
                }
                0x37 => {
                    // SCF
                    flags = (flags & (S80 | Z40 | P04)) | C01 | (a as i32 & (F20 | F08));
                }

                0xDB => {
                    // IN A,(imm)
                    pc = pc.wrapping_add(1);
                    a = in_port!(data as u32 + a as u32 * 0x100);
                }

                0xE3 => {
                    // EX (SP),HL
                    let temp = rd_word!(sp);
                    wr_word!(sp, hl!());
                    h = (temp >> 8) as u8;
                    l = temp as u8;
                }

                0xEB => {
                    // EX DE,HL
                    std::mem::swap(&mut d, &mut h);
                    std::mem::swap(&mut e, &mut l);
                }

                0xD9 => {
                    // EXX
                    std::mem::swap(&mut self.r.alt_b, &mut b);
                    std::mem::swap(&mut self.r.alt_c, &mut c);
                    std::mem::swap(&mut self.r.alt_d, &mut d);
                    std::mem::swap(&mut self.r.alt_e, &mut e);
                    std::mem::swap(&mut self.r.alt_h, &mut h);
                    std::mem::swap(&mut self.r.alt_l, &mut l);
                }

                0xF3 => {
                    // DI
                    self.r.iff1 = 0;
                    self.r.iff2 = 0;
                }

                0xFB => {
                    // EI
                    self.r.iff1 = 1;
                    self.r.iff2 = 1;
                }

                0x76 => {
                    // HALT
                    break 'run Exit::Halt;
                }

                //////////////////////////////////////// CB prefix
                0xCB => {
                    pc = pc.wrapping_add(1);
                    match data {
                        // Rotate left
                        0x06 => {
                            // RLC (HL)
                            s_time += 7;
                            rlc_addr!(hl!());
                        }
                        0x00..=0x05 | 0x07 => {
                            // RLC r
                            let idx = data & 7;
                            let result = r8!(idx).rotate_left(1);
                            flags = sz28p!(result) | (result as i32 & C01);
                            set_r8!(idx, result);
                        }

                        0x16 => {
                            // RL (HL)
                            s_time += 7;
                            rl_addr!(hl!());
                        }
                        0x10..=0x15 | 0x17 => {
                            // RL r
                            let idx = data & 7;
                            let result: u16 = ((r8!(idx) as u16) << 1) | (flags & C01) as u16;
                            flags = sz28pc!(result);
                            set_r8!(idx, result as u8);
                        }

                        0x26 => {
                            // SLA (HL)
                            s_time += 7;
                            sla_addr!(hl!(), 0u16);
                        }
                        0x20..=0x25 | 0x27 => {
                            // SLA r
                            let idx = data & 7;
                            let result: u16 = (r8!(idx) as u16) << 1;
                            flags = sz28pc!(result);
                            set_r8!(idx, result as u8);
                        }

                        0x36 => {
                            // SLL (HL)
                            s_time += 7;
                            sla_addr!(hl!(), 1u16);
                        }
                        0x30..=0x35 | 0x37 => {
                            // SLL r
                            let idx = data & 7;
                            let result: u16 = ((r8!(idx) as u16) << 1) | 1;
                            flags = sz28pc!(result);
                            set_r8!(idx, result as u8);
                        }

                        // Rotate right
                        0x0E => {
                            // RRC (HL)
                            s_time += 7;
                            rrc_addr!(hl!());
                        }
                        0x08..=0x0D | 0x0F => {
                            // RRC r
                            let idx = data & 7;
                            let v = r8!(idx);
                            flags = v as i32 & C01;
                            let result = v.rotate_right(1);
                            flags |= sz28p!(result);
                            set_r8!(idx, result);
                        }

                        0x1E => {
                            // RR (HL)
                            s_time += 7;
                            rr_addr!(hl!());
                        }
                        0x18..=0x1D | 0x1F => {
                            // RR r
                            let idx = data & 7;
                            let v = r8!(idx);
                            let temp = v as i32 & C01;
                            let result = ((flags << 7) as u8) | (v >> 1);
                            flags = sz28p!(result) | temp;
                            set_r8!(idx, result);
                        }

                        0x2E => {
                            // SRA (HL)
                            s_time += 7;
                            sra_addr!(hl!());
                        }
                        0x28..=0x2D | 0x2F => {
                            // SRA r
                            let idx = data & 7;
                            let v = r8!(idx);
                            flags = v as i32 & C01;
                            let result = (v & 0x80) | (v >> 1);
                            flags |= sz28p!(result);
                            set_r8!(idx, result);
                        }

                        0x3E => {
                            // SRL (HL)
                            s_time += 7;
                            srl_addr!(hl!());
                        }
                        0x38..=0x3D | 0x3F => {
                            // SRL r
                            let idx = data & 7;
                            let v = r8!(idx);
                            flags = v as i32 & C01;
                            let result = v >> 1;
                            flags |= sz28p!(result);
                            set_r8!(idx, result);
                        }

                        // BIT
                        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x76 | 0x7E => {
                            // BIT b,(HL)
                            s_time += 4;
                            let temp = rd!(hl!()) as i32;
                            flags &= C01;
                            let masked = temp & (1 << ((data >> 3) & 7));
                            flags |= (masked & S80) | H10 | ((masked - 1) >> 8 & (Z40 | P04));
                        }
                        0x40..=0x7F => {
                            // BIT b,r
                            let temp = r8!(data & 7) as i32;
                            flags = (flags & C01) | (temp & (F20 | F08));
                            let masked = temp & (1 << ((data >> 3) & 7));
                            flags |= (masked & S80) | H10 | ((masked - 1) >> 8 & (Z40 | P04));
                        }

                        // SET/RES (HL)
                        0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE | 0xC6 | 0xCE
                        | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                            s_time += 7;
                            let addr = hl!();
                            let mut temp = rd!(addr) as i32;
                            let bit = 1 << ((data >> 3) & 7);
                            temp |= bit; // SET
                            if data & 0x40 == 0 {
                                temp ^= bit; // RES
                            }
                            wr!(addr, temp);
                        }

                        // SET b,r
                        0xC0..=0xFF => {
                            let idx = data & 7;
                            let v = r8!(idx) | (1 << ((data >> 3) & 7));
                            set_r8!(idx, v);
                        }
                        // RES b,r
                        0x80..=0xBF => {
                            let idx = data & 7;
                            let v = r8!(idx) & !(1 << ((data >> 3) & 7));
                            set_r8!(idx, v);
                        }
                    }
                }

                //////////////////////////////////////// ED prefix
                0xED => {
                    pc = pc.wrapping_add(1);
                    s_time += (ED_DD_TIMING[data as usize] >> 4) as CpuTime;
                    match data {
                        0x72 | 0x7A | 0x42 | 0x52 | 0x62 | 0x4A | 0x5A | 0x6A => {
                            // SBC/ADC HL,rp
                            let mut temp: u32 = if data == 0x72 || data == 0x7A {
                                sp as u32
                            } else {
                                rp!(((data >> 3 & 6) >> 1) as usize) as u32
                            };
                            let mut sum: u32 = temp.wrapping_add((flags & C01) as u32);
                            flags = !(data as i32) >> 2 & N02;
                            if flags != 0 {
                                sum = sum.wrapping_neg();
                            }
                            sum = sum.wrapping_add(hl!() as u32);
                            temp ^= hl!() as u32;
                            temp ^= sum;
                            flags |= ((sum >> 16) & C01 as u32) as i32
                                | ((temp >> 8) as i32 & H10)
                                | ((sum >> 8) as i32 & (S80 | F20 | F08))
                                | ((temp.wrapping_add(0x8000) >> 14) as i32 & V04);
                            h = (sum >> 8) as u8;
                            l = sum as u8;
                            if sum as u16 == 0 {
                                flags |= Z40;
                            }
                        }

                        0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                            // IN r,(C)
                            let temp = in_port!(bc!());
                            set_r8!(data >> 3, temp);
                            flags = (flags & C01) | sz28p!(temp);
                        }

                        0x71 => {
                            // OUT (C),0
                            flags_slot = 0;
                            let v = r8!(data >> 3);
                            out_port!(bc!(), v);
                        }
                        0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x79 => {
                            // OUT (C),r
                            let v = r8!(data >> 3);
                            out_port!(bc!(), v);
                        }

                        0x73 | 0x43 | 0x53 => {
                            // LD (ADDR),SP / LD (ADDR),BC / LD (ADDR),DE
                            let temp: u16 = match data {
                                0x73 => sp,
                                0x43 => bc!(),
                                _ => de!(),
                            };
                            let addr = rd_word!(pc);
                            pc = pc.wrapping_add(2);
                            wr_word!(addr, temp);
                        }

                        0x4B | 0x5B => {
                            // LD BC,(ADDR) / LD DE,(ADDR)
                            let addr = rd_word!(pc);
                            pc = pc.wrapping_add(2);
                            let v = rd_word!(addr);
                            set_rp!(((data - 0x4B) >> 4) as usize, v);
                        }

                        0x7B => {
                            // LD SP,(ADDR)
                            let addr = rd_word!(pc);
                            pc = pc.wrapping_add(2);
                            sp = rd_word!(addr);
                        }

                        0x67 => {
                            // RRD
                            let addr = hl!();
                            let temp = rd!(addr);
                            wr!(addr, (a << 4) | (temp >> 4));
                            let temp = (a & 0xF0) | (temp & 0x0F);
                            flags = (flags & C01) | sz28p!(temp);
                            a = temp;
                        }

                        0x6F => {
                            // RLD
                            let addr = hl!();
                            let temp = rd!(addr);
                            wr!(addr, (temp << 4) | (a & 0x0F));
                            let temp = (a & 0xF0) | (temp >> 4);
                            flags = (flags & C01) | sz28p!(temp);
                            a = temp;
                        }

                        0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => {
                            // NEG (C++ sets opcode = 0x10 as "subtract" flag)
                            flags &= !C01;
                            let v = a;
                            a = 0;
                            adc_data!(0x10, v);
                        }

                        0xA9 | 0xB9 | 0xA1 | 0xB1 => {
                            // CPD/CPDR/CPI/CPIR
                            let inc: i32 = if data & 0x08 != 0 { -1 } else { 1 };
                            let addr = hl!();
                            let v = (addr as i32 + inc) as u16;
                            h = (v >> 8) as u8;
                            l = v as u8;
                            let temp = rd!(addr) as i32;

                            let mut result = a as i32 - temp;
                            flags = (flags & C01)
                                | N02
                                | ((((temp ^ a as i32) & H10) ^ result) & (S80 | H10));

                            if result as u8 == 0 {
                                flags |= Z40;
                            }
                            result -= (flags & H10) >> 4;
                            flags |= result & F08;
                            flags |= result << 4 & F20;

                            let new_bc = bc!().wrapping_sub(1);
                            b = (new_bc >> 8) as u8;
                            c = new_bc as u8;
                            if new_bc != 0 {
                                flags |= V04;
                                if flags & Z40 == 0 && data >= 0xB0 {
                                    pc = pc.wrapping_sub(2);
                                    s_time += 5;
                                }
                            }
                        }

                        0xA8 | 0xB8 | 0xA0 | 0xB0 => {
                            // LDD/LDDR/LDI/LDIR
                            let inc: i32 = if data & 0x08 != 0 { -1 } else { 1 };
                            let addr = hl!();
                            let v = (addr as i32 + inc) as u16;
                            h = (v >> 8) as u8;
                            l = v as u8;
                            let mut temp = rd!(addr) as i32;

                            let daddr = de!();
                            let v = (daddr as i32 + inc) as u16;
                            d = (v >> 8) as u8;
                            e = v as u8;
                            wr!(daddr, temp);

                            temp += a as i32;
                            flags = (flags & (S80 | Z40 | C01)) | (temp & F08) | (temp << 4 & F20);
                            let new_bc = bc!().wrapping_sub(1);
                            b = (new_bc >> 8) as u8;
                            c = new_bc as u8;
                            if new_bc != 0 {
                                flags |= V04;
                                if data >= 0xB0 {
                                    pc = pc.wrapping_sub(2);
                                    s_time += 5;
                                }
                            }
                        }

                        0xAB | 0xBB | 0xA3 | 0xB3 => {
                            // OUTD/OTDR/OUTI/OTIR
                            let inc: i32 = if data & 0x08 != 0 { -1 } else { 1 };
                            let addr = hl!();
                            let v = (addr as i32 + inc) as u16;
                            h = (v >> 8) as u8;
                            l = v as u8;
                            let temp = rd!(addr);

                            b = b.wrapping_sub(1);
                            flags = ((temp as i32) >> 6 & N02) | sz28!(b);
                            if b != 0 && data >= 0xB0 {
                                pc = pc.wrapping_sub(2);
                                s_time += 5;
                            }
                            out_port!(bc!(), temp);
                        }

                        0xAA | 0xBA | 0xA2 | 0xB2 => {
                            // IND/INDR/INI/INIR
                            let inc: i32 = if data & 0x08 != 0 { -1 } else { 1 };
                            let addr = hl!();
                            let v = (addr as i32 + inc) as u16;
                            h = (v >> 8) as u8;
                            l = v as u8;

                            let temp = in_port!(bc!());

                            b = b.wrapping_sub(1);
                            flags = ((temp as i32) >> 6 & N02) | sz28!(b);
                            if b != 0 && data >= 0xB0 {
                                pc = pc.wrapping_sub(2);
                                s_time += 5;
                            }
                            wr!(addr, temp);
                        }

                        0x47 => self.r.i = a, // LD I,A

                        0x4F => {
                            // LD R,A (not supported)
                            self.r.r = a;
                            warning = true;
                        }

                        0x57 => {
                            // LD A,I
                            a = self.r.i;
                            flags = (flags & C01) | sz28!(a) | ((self.r.iff2 as i32) << 2 & V04);
                        }

                        0x5F => {
                            // LD A,R (not supported)
                            a = self.r.r;
                            warning = true;
                            flags = (flags & C01) | sz28!(a) | ((self.r.iff2 as i32) << 2 & V04);
                        }

                        0x45 | 0x4D | 0x55 | 0x5D | 0x65 | 0x6D | 0x75 | 0x7D => {
                            // RETI/RETN
                            self.r.iff1 = self.r.iff2;
                            ret_taken!();
                        }

                        0x46 | 0x4E | 0x66 | 0x6E => self.r.im = 0, // IM 0
                        0x56 | 0x76 => self.r.im = 1,               // IM 1
                        0x5E | 0x7E => self.r.im = 2,               // IM 2

                        _ => {
                            // Opcode $ED $xx not supported
                            warning = true;
                        }
                    }
                }

                //////////////////////////////////////// DD/FD prefix
                0xDD | 0xFD => {
                    let mut ixy: u16 = if opcode == 0xDD { ix } else { iy };
                    pc = pc.wrapping_add(1);
                    let data2 = rd!(pc);
                    s_time += (ED_DD_TIMING[data as usize] & 0x0F) as CpuTime;

                    // set_ixy at the end of arms that modify ixy
                    macro_rules! set_ixy {
                        () => {
                            if opcode == 0xDD {
                                ix = ixy;
                            } else {
                                iy = ixy;
                            }
                        };
                    }
                    macro_rules! ixy_disp {
                        () => {
                            (ixy as i32 + data2 as i8 as i32) as u16
                        };
                    }

                    match data {
                        // ADD/ADC/SUB/SBC
                        0x96 | 0x86 => {
                            // SUB/ADD (IXY+disp)
                            flags &= !C01;
                            pc = pc.wrapping_add(1);
                            let v = rd!(ixy_disp!());
                            adc_data!(data, v);
                        }
                        0x9E | 0x8E => {
                            // SBC/ADC (IXY+disp)
                            pc = pc.wrapping_add(1);
                            let v = rd!(ixy_disp!());
                            adc_data!(data, v);
                        }

                        0x94 | 0x84 => {
                            // SUB/ADD HXY
                            flags &= !C01;
                            let v = (ixy >> 8) as u8;
                            adc_data!(data, v);
                        }
                        0x9C | 0x8C => {
                            // SBC/ADC HXY
                            let v = (ixy >> 8) as u8;
                            adc_data!(data, v);
                        }

                        0x95 | 0x85 => {
                            // SUB/ADD LXY
                            flags &= !C01;
                            let v = ixy as u8;
                            adc_data!(data, v);
                        }
                        0x9D | 0x8D => {
                            // SBC/ADC LXY
                            let v = ixy as u8;
                            adc_data!(data, v);
                        }

                        0x39 | 0x29 | 0x09 | 0x19 => {
                            // ADD IXY,rp
                            let mut temp: u32 = match data {
                                0x39 => sp as u32,
                                0x29 => ixy as u32,
                                _ => rp!(((data - 0x09) >> 4) as usize) as u32,
                            };
                            let sum: u32 = ixy as u32 + temp;
                            temp ^= ixy as u32;
                            ixy = sum as u16;
                            flags = (flags & (S80 | Z40 | V04))
                                | (sum >> 16) as i32
                                | ((sum >> 8) as i32 & (F20 | F08))
                                | (((temp ^ sum) >> 8) as i32 & H10);
                            set_ixy!();
                        }

                        // AND
                        0xA6 => {
                            pc = pc.wrapping_add(1);
                            let v = rd!(ixy_disp!());
                            and_data!(v);
                        }
                        0xA4 => and_data!(ixy >> 8),
                        0xA5 => and_data!(ixy as u8),

                        // OR
                        0xB6 => {
                            pc = pc.wrapping_add(1);
                            let v = rd!(ixy_disp!());
                            or_data!(v);
                        }
                        0xB4 => or_data!(ixy >> 8),
                        0xB5 => or_data!(ixy as u8),

                        // XOR
                        0xAE => {
                            pc = pc.wrapping_add(1);
                            let v = rd!(ixy_disp!());
                            xor_data!(v);
                        }
                        0xAC => xor_data!(ixy >> 8),
                        0xAD => xor_data!(ixy as u8),

                        // CP
                        0xBE => {
                            pc = pc.wrapping_add(1);
                            let v = rd!(ixy_disp!());
                            cp_data!(v);
                        }
                        0xBC => cp_data!(ixy >> 8),
                        0xBD => cp_data!(ixy as u8),

                        // LD
                        0x70..=0x75 | 0x77 => {
                            // LD (IXY+disp),r
                            let v = r8!(data & 7);
                            pc = pc.wrapping_add(1);
                            wr!(ixy_disp!(), v);
                        }
                        0x36 => {
                            // LD (IXY+disp),imm
                            pc = pc.wrapping_add(1);
                            let v = rd!(pc);
                            pc = pc.wrapping_add(1);
                            wr!(ixy_disp!(), v);
                        }

                        0x44 | 0x4C | 0x54 | 0x5C | 0x7C => {
                            // LD r,HXY
                            set_r8!(data >> 3, ixy >> 8);
                        }

                        0x64 | 0x6D => {} // LD HXY,HXY / LD LXY,LXY

                        0x45 | 0x4D | 0x55 | 0x5D | 0x7D => {
                            // LD r,LXY
                            set_r8!(data >> 3, ixy);
                        }

                        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                            // LD r,(IXY+disp)
                            pc = pc.wrapping_add(1);
                            let v = rd!(ixy_disp!());
                            set_r8!(data >> 3, v);
                        }

                        0x26 => {
                            // LD HXY,imm
                            pc = pc.wrapping_add(1);
                            ixy = (ixy & 0x00FF) | ((data2 as u16) << 8);
                            set_ixy!();
                        }
                        0x65 => {
                            // LD HXY,LXY
                            let v = ixy as u8;
                            ixy = (ixy & 0x00FF) | ((v as u16) << 8);
                            set_ixy!();
                        }
                        0x60..=0x63 | 0x67 => {
                            // LD HXY,r
                            let v = r8!(data & 7);
                            ixy = (ixy & 0x00FF) | ((v as u16) << 8);
                            set_ixy!();
                        }

                        0x2E => {
                            // LD LXY,imm
                            pc = pc.wrapping_add(1);
                            ixy = (ixy & 0xFF00) | data2 as u16;
                            set_ixy!();
                        }
                        0x6C => {
                            // LD LXY,HXY
                            let v = (ixy >> 8) as u8;
                            ixy = (ixy & 0xFF00) | v as u16;
                            set_ixy!();
                        }
                        0x68..=0x6B | 0x6F => {
                            // LD LXY,r
                            let v = r8!(data & 7);
                            ixy = (ixy & 0xFF00) | v as u16;
                            set_ixy!();
                        }

                        0xF9 => sp = ixy, // LD SP,IXY

                        0x22 => {
                            // LD (ADDR),IXY
                            let addr = rd_word!(pc);
                            pc = pc.wrapping_add(2);
                            wr_word!(addr, ixy);
                        }

                        0x21 => {
                            // LD IXY,imm
                            ixy = rd_word!(pc);
                            pc = pc.wrapping_add(2);
                            set_ixy!();
                        }

                        0x2A => {
                            // LD IXY,(addr)
                            let addr = rd_word!(pc);
                            ixy = rd_word!(addr);
                            pc = pc.wrapping_add(2);
                            set_ixy!();
                        }

                        // DD/FD CB prefix
                        0xCB => {
                            let addr = ixy_disp!();
                            pc = pc.wrapping_add(1);
                            let data2 = rd!(pc);
                            pc = pc.wrapping_add(1);
                            match data2 {
                                0x06 => rlc_addr!(addr),       // RLC (IXY)
                                0x16 => rl_addr!(addr),        // RL (IXY)
                                0x26 => sla_addr!(addr, 0u16), // SLA (IXY)
                                0x36 => sla_addr!(addr, 1u16), // SLL (IXY)
                                0x0E => rrc_addr!(addr),       // RRC (IXY)
                                0x1E => rr_addr!(addr),        // RR (IXY)
                                0x2E => sra_addr!(addr),       // SRA (IXY)
                                0x3E => srl_addr!(addr),       // SRL (IXY)

                                0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x76 | 0x7E => {
                                    // BIT b,(IXY+disp)
                                    let temp = rd!(addr) as i32;
                                    let masked = temp & (1 << ((data2 >> 3) & 7));
                                    flags = (flags & C01)
                                        | H10
                                        | (masked & S80)
                                        | ((masked - 1) >> 8 & (Z40 | P04));
                                }

                                0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE | 0xC6
                                | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                                    // RES/SET b,(IXY+disp)
                                    let mut temp = rd!(addr) as i32;
                                    let bit = 1 << ((data2 >> 3) & 7);
                                    temp |= bit; // SET
                                    if data2 & 0x40 == 0 {
                                        temp ^= bit; // RES
                                    }
                                    wr!(addr, temp);
                                }

                                _ => {
                                    // Opcode $DD/$FD $CB $xx not supported
                                    warning = true;
                                }
                            }
                        }

                        // INC/DEC
                        0x23 => {
                            ixy = ixy.wrapping_add(1);
                            set_ixy!();
                        }
                        0x2B => {
                            ixy = ixy.wrapping_sub(1);
                            set_ixy!();
                        }

                        0x34 => {
                            // INC (IXY+disp)
                            let addr = ixy_disp!();
                            pc = pc.wrapping_add(1);
                            let data16 = rd!(addr) as u16 + 1;
                            wr!(addr, data16);
                            inc_set_flags!(data16);
                        }

                        0x35 => {
                            // DEC (IXY+disp)
                            let addr = ixy_disp!();
                            pc = pc.wrapping_add(1);
                            let data16 = (rd!(addr) as u16).wrapping_sub(1);
                            wr!(addr, data16);
                            dec_set_flags!(data16);
                        }

                        0x24 => {
                            // INC HXY
                            ixy = ixy.wrapping_add(0x100);
                            let data16 = ixy >> 8;
                            set_ixy!();
                            inc_set_flags!(data16);
                        }
                        0x2C => {
                            // INC LXY
                            let data16 = (ixy as u8).wrapping_add(1) as u16;
                            ixy = (ixy & 0xFF00) | data16;
                            set_ixy!();
                            inc_set_flags!(data16);
                        }

                        0x25 => {
                            // DEC HXY
                            ixy = ixy.wrapping_sub(0x100);
                            let data16 = ixy >> 8;
                            set_ixy!();
                            dec_set_flags!(data16);
                        }
                        0x2D => {
                            // DEC LXY
                            let data16 = (ixy as u8).wrapping_sub(1) as u16;
                            ixy = (ixy & 0xFF00) | data16;
                            set_ixy!();
                            dec_set_flags!(data16);
                        }

                        // PUSH/POP
                        0xE5 => push16!(ixy), // PUSH IXY

                        0xE1 => {
                            // POP IXY
                            ixy = rd_word!(sp);
                            sp = sp.wrapping_add(2);
                            set_ixy!();
                        }

                        // Misc
                        0xE9 => pc = ixy, // JP (IXY)

                        0xE3 => {
                            // EX (SP),IXY
                            let temp = rd_word!(sp);
                            wr_word!(sp, ixy);
                            ixy = temp;
                            set_ixy!();
                        }

                        _ => {
                            // Unnecessary DD/FD prefix
                            warning = true;
                            pc = pc.wrapping_sub(1);
                        }
                    }
                }
            }
        };

        // out_of_time / halt exit
        if let Exit::Halt = exit {
            s_time &= 3; // increment by multiple of 4
        }
        pc = pc.wrapping_sub(1);

        self.time = s_time;
        self.r.pc = pc;
        self.r.sp = sp;
        self.r.ix = ix;
        self.r.iy = iy;
        self.r.a = a;
        self.r.b = b;
        self.r.c = c;
        self.r.d = d;
        self.r.e = e;
        self.r.h = h;
        self.r.l = l;
        self.r.flags = flags as u8;

        warning
    }
}
