// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Sap_Cpu.cpp (2026-06-09), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
/// Atari 6502 CPU emulator for SAP format.
/// Matches C++ Sap_Cpu.h / Sap_Cpu.cpp.
pub type SapTime = i32;
pub type SapAddr = u32;

pub const FUTURE_SAP_TIME: SapTime = i32::MAX / 2 + 1;
pub const IDLE_ADDR: SapAddr = 0xFEFF;

/// Result of CPU execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuResult {
    /// CPU ran out of time (normal).
    OutOfTime,
    /// CPU hit idle_addr (play routine returned).
    HitIdle,
    /// CPU encountered an illegal instruction.
    Illegal,
}

// Status flags
const ST_N: u8 = 0x80;
const ST_V: u8 = 0x40;
const ST_R: u8 = 0x20;
const ST_B: u8 = 0x10;
const ST_D: u8 = 0x08;
const ST_I: u8 = 0x04;
const ST_Z: u8 = 0x02;
const ST_C: u8 = 0x01;
const IRQ_INHIBIT: u8 = ST_I;

/// CPU registers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Registers {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub status: u8,
    pub sp: u8,
}

struct State {
    base: SapTime,
    time: SapTime,
}

/// 6502 CPU with cycle-accurate timing.
pub struct SapCpu {
    pub r: Registers,
    state: State,
    irq_time: SapTime,
    end_time: SapTime,
    /// 64KB memory, accessed via raw pointer for speed.
    mem: *mut u8,
}

// SAFETY: SapCpu only accesses memory through the mem pointer which is owned
// by SapEmu. The CPU does not share state across threads.
unsafe impl Send for SapCpu {}

impl Default for SapCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl SapCpu {
    pub fn new() -> Self {
        Self {
            r: Registers::default(),
            state: State { base: 0, time: 0 },
            irq_time: FUTURE_SAP_TIME,
            end_time: FUTURE_SAP_TIME,
            mem: std::ptr::null_mut(),
        }
    }

    /// Reset CPU with pointer to 64KB memory.
    pub fn reset(&mut self, mem: *mut u8) {
        self.mem = mem;
        self.r.status = ST_I;
        self.r.sp = 0xFF;
        self.r.pc = 0;
        self.r.a = 0;
        self.r.x = 0;
        self.r.y = 0;
        self.state.time = 0;
        self.state.base = 0;
        self.irq_time = FUTURE_SAP_TIME;
        self.end_time = FUTURE_SAP_TIME;
    }

    #[inline]
    pub fn time(&self) -> SapTime {
        self.state.time + self.state.base
    }

    #[inline]
    pub fn set_time(&mut self, t: SapTime) {
        self.state.time = t - self.state.base;
    }

    pub fn irq_time(&self) -> SapTime {
        self.irq_time
    }

    pub fn set_irq_time(&mut self, t: SapTime) {
        self.irq_time = t;
        let delta = self.update_end_time(self.end_time, t);
        self.state.time += delta;
    }

    pub fn end_time_val(&self) -> SapTime {
        self.end_time
    }

    pub fn set_end_time(&mut self, t: SapTime) {
        self.end_time = t;
        let delta = self.update_end_time(t, self.irq_time);
        self.state.time += delta;
    }

    fn update_end_time(&mut self, t: SapTime, irq: SapTime) -> SapTime {
        let mut t = t;
        if irq < t && (self.r.status & IRQ_INHIBIT) == 0 {
            t = irq;
        }
        let delta = self.state.base - t;
        self.state.base = t;
        delta
    }

    /// Run CPU until end_time. Returns the reason for stopping.
    /// The `cpu_write` callback is called for writes to the $D2xx page.
    #[allow(unused_assignments)]
    pub fn run<F>(&mut self, end_time: SapTime, cpu_write: &mut F) -> CpuResult
    where
        F: FnMut(u32, u8),
    {
        self.set_end_time(end_time);

        // Cache state locally for speed (matches C++ pattern)
        let mut s_base = self.state.base;
        let mut s_time = self.state.time;
        let mem = self.mem;
        let mut result = CpuResult::OutOfTime;

        // Registers
        let mut pc = self.r.pc;
        let mut a = self.r.a;
        let mut x = self.r.x;
        let mut y = self.r.y;
        let mut sp: u16 = (self.r.sp as u16 + 1) | 0x100;

        // Status flags - use nz/c representation for fast flag computation
        let mut status: u8;
        let mut c: u16; // carry set if (c & 0x100) != 0
        let mut nz: u16; // Z set if (nz & 0xFF) == 0, N set if (nz & 0x8080) != 0
        {
            let temp = self.r.status;
            // SET_STATUS
            status = temp & (ST_V | ST_D | ST_I);
            nz = (temp as u16) << 8;
            c = nz;
            nz |= (!(temp as u16)) & (ST_Z as u16);
        }

        macro_rules! flush_time {
            () => {
                self.state.time = s_time;
            };
        }

        macro_rules! cache_time {
            () => {
                s_time = self.state.time;
            };
        }

        macro_rules! read {
            ($addr:expr) => {{
                flush_time!();
                let addr = $addr as usize;
                let val = if addr < 0x10000 { *mem.add(addr) } else { 0 };
                cache_time!();
                val
            }};
        }

        macro_rules! write {
            ($addr:expr, $data:expr) => {{
                let addr = $addr as usize;
                let data = $data;
                if addr < 0x10000 {
                    *mem.add(addr) = data;
                    if (addr >> 8) == 0xD2 {
                        flush_time!();
                        cpu_write(addr as u32, data);
                        cache_time!();
                    }
                }
            }};
        }

        macro_rules! read_low {
            ($addr:expr) => {
                *mem.add($addr as usize)
            };
        }

        macro_rules! write_low {
            ($addr:expr, $data:expr) => {
                *mem.add($addr as usize) = $data
            };
        }

        macro_rules! read_prog {
            ($addr:expr) => {
                read_low!($addr)
            };
        }

        macro_rules! get_sp {
            () => {
                (sp.wrapping_sub(1)) & 0xFF
            };
        }

        macro_rules! push {
            ($v:expr) => {{
                sp = (sp.wrapping_sub(1)) | 0x100;
                write_low!(sp, $v);
            }};
        }

        macro_rules! get_le16 {
            ($ptr:expr) => {{
                let p = $ptr;
                (*p as u16) | ((*p.add(1) as u16) << 8)
            }};
        }

        macro_rules! calc_status {
            ($out:ident) => {
                let mut $out = status & (ST_V | ST_D | ST_I);
                $out |= ((nz >> 8) | nz) as u8 & ST_N;
                $out |= (c >> 8) as u8 & ST_C;
                if (nz & 0xFF) == 0 {
                    $out |= ST_Z;
                }
            };
        }

        macro_rules! set_status {
            ($in:expr) => {
                status = $in & (ST_V | ST_D | ST_I);
                nz = ($in as u16) << 8;
                c = nz;
                nz |= !($in as u16) & (ST_Z as u16);
            };
        }

        let clock_table: [u8; 256] = [
            0, 6, 2, 8, 3, 3, 5, 5, 3, 2, 2, 2, 4, 4, 6, 6, 3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4,
            4, 7, 7, 6, 6, 2, 8, 3, 3, 5, 5, 4, 2, 2, 2, 4, 4, 6, 6, 3, 5, 2, 8, 4, 4, 6, 6, 2, 4,
            2, 7, 4, 4, 7, 7, 6, 6, 2, 8, 3, 3, 5, 5, 3, 2, 2, 2, 3, 4, 6, 6, 3, 5, 2, 8, 4, 4, 6,
            6, 2, 4, 2, 7, 4, 4, 7, 7, 6, 6, 2, 8, 3, 3, 5, 5, 4, 2, 2, 2, 5, 4, 6, 6, 3, 5, 2, 8,
            4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, 2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, 3,
            6, 2, 6, 4, 4, 4, 4, 2, 5, 2, 5, 5, 5, 5, 5, 2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4,
            4, 4, 3, 5, 2, 5, 4, 4, 4, 4, 2, 4, 2, 4, 4, 4, 4, 4, 2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2,
            2, 4, 4, 6, 6, 3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, 2, 6, 2, 8, 3, 3, 5, 5,
            2, 2, 2, 2, 4, 4, 6, 6, 3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
        ];

        // Main emulation loop
        // SAFETY: mem is a valid pointer to 64KB memory owned by SapEmu.
        // The CPU is the sole accessor of this memory during execution.
        let mut data: i32;
        unsafe {
            'main_loop: loop {
                let opcode = *mem.add(pc as usize);
                pc = pc.wrapping_add(1);
                let instr = mem.add(pc as usize);

                let cycles = clock_table[opcode as usize] as i32;
                s_time += cycles;

                // Check if time is up AFTER adding cycles
                // Matches C++ exactly: add cycles first, then check
                if s_time >= 0 {
                    // Possibly out of time - check if we should actually stop
                    if s_time < cycles {
                        // Not enough time has accumulated to skip this instruction
                        // Execute it (fall through to almost_out_of_time)
                    } else {
                        // Out of time - undo cycle addition and break
                        s_time -= cycles;
                        pc = pc.wrapping_sub(1);
                        break 'main_loop;
                    }
                }

                // Fetch operand and execute instruction
                data = *instr as i32;

                match opcode {
                    // === BRK ===
                    0x00 => {
                        if (pc.wrapping_sub(1)) as u32 >= IDLE_ADDR {
                            // idle_done - CPU hit idle address
                            // Matches C++: pc--; goto stop;
                            // In C++, pc has already been incremented, so pc-1 = idle_addr
                            pc = pc.wrapping_sub(1);
                            result = CpuResult::HitIdle;
                            break 'main_loop;
                        }
                        // Normal BRK: push PC+1 and status, jump to IRQ vector
                        // Note: we push pc (which is already pc+1 from the fetch)
                        s_time += 7;
                        write_low!(0x100u16 | (sp.wrapping_sub(1)), (pc >> 8) as u8);
                        write_low!(0x100u16 | (sp.wrapping_sub(2)), pc as u8);
                        let vec_addr = 0xFFFEu16;
                        pc = read_prog!(vec_addr) as u16
                            | ((read_prog!(vec_addr.wrapping_add(1)) as u16) << 8);
                        sp = (sp.wrapping_sub(3)) | 0x100;
                        calc_status!(temp);
                        let temp = temp | ST_R | ST_B;
                        write_low!(sp, temp);
                        status &= !ST_D;
                        status |= ST_I;
                        self.r.status = status;
                        let delta = s_base - self.end_time;
                        s_base = self.end_time;
                        s_time += delta;
                    }

                    // === LDA zp,x ===
                    0xB5 => {
                        a = read_low!((data as u8).wrapping_add(x) as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }

                    // === LDA zp ===
                    0xA5 => {
                        a = read_low!(data as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }

                    // === BNE ===
                    0xD0 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (nz & 0xFF) != 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1; // dec_clock_loop: not-taken = 2 cycles
                        }
                    }

                    // === JSR ===
                    0x20 => {
                        let temp = pc.wrapping_add(1);
                        pc = get_le16!(instr);
                        write_low!(0x100u16 | (sp.wrapping_sub(1)), (temp >> 8) as u8);
                        sp = (sp.wrapping_sub(2)) | 0x100;
                        write_low!(sp, temp as u8);
                    }

                    // === JMP abs ===
                    0x4C => {
                        pc = get_le16!(instr);
                    }

                    // === INX ===
                    0xE8 => {
                        x = x.wrapping_add(1);
                        nz = x as u16;
                    }

                    // === BPL ===
                    0x10 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (nz & 0x8080) == 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1;
                        }
                    }

                    // === CMP (all addressing modes) ===
                    0xC5 => {
                        // CMP zp
                        data = read_low!(data as u16) as i32;
                        pc = pc.wrapping_add(1);
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }
                    0xD5 => {
                        // CMP zp,x
                        data = read_low!((data as u8).wrapping_add(x) as u16) as i32;
                        pc = pc.wrapping_add(1);
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }
                    0xC9 => {
                        // CMP imm
                        pc = pc.wrapping_add(1);
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }
                    0xCD => {
                        // CMP abs
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        data = read!(addr) as i32;
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }
                    0xDD => {
                        // CMP abs,x
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32;
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }
                    0xD9 => {
                        // CMP abs,y
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32;
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }
                    0xC1 => {
                        // CMP (ind,x)
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        data = read!(addr) as i32;
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }
                    0xD1 => {
                        // CMP (ind),y
                        let temp = read_low!(data as u16) as u16
                            | ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        let addr = temp.wrapping_add(y as u16);
                        pc = pc.wrapping_add(1);
                        if (temp & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32;
                        nz = (a as i32 - data) as u16 & 0xFF;
                        c = if (a as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }

                    // === BMI ===
                    0x30 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (nz & 0x8080) != 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1;
                        }
                    }

                    // === BEQ ===
                    0xF0 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (nz & 0xFF) == 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1;
                        }
                    }

                    // === STA zp,x ===
                    0x95 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        write_low!(addr, a);
                    }

                    // === STA zp ===
                    0x85 => {
                        pc = pc.wrapping_add(1);
                        write_low!(data as u16, a);
                    }

                    // === INY ===
                    0xC8 => {
                        y = y.wrapping_add(1);
                        nz = y as u16;
                    }

                    // === TAY ===
                    0xA8 => {
                        y = a;
                        nz = a as u16;
                    }

                    // === TYA ===
                    0x98 => {
                        a = y;
                        nz = y as u16;
                    }

                    // === LDA abs ===
                    0xAD => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        a = read!(addr);
                        nz = a as u16;
                    }

                    // === RTS ===
                    0x60 => {
                        pc = read_low!(sp) as u16;
                        pc = pc.wrapping_add(
                            (read_low!(0x100u16 | (sp.wrapping_sub(0xFF))) as u16) << 8,
                        );
                        pc = pc.wrapping_add(1);
                        sp = (sp.wrapping_sub(0xFE)) | 0x100;
                    }

                    // === STA abs,Y ===
                    0x99 => {
                        let addr = y as u16 + get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        write!(addr, a);
                    }

                    // === STA abs ===
                    0x8D => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        write!(addr, a);
                    }

                    // === STA abs,X ===
                    0x9D => {
                        let addr = x as u16 + get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        write!(addr, a);
                    }

                    // === STA (ind),Y ===
                    0x91 => {
                        let temp = read_low!(data as u16) as u16 + y as u16;
                        let addr =
                            temp + ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        write!(addr, a);
                    }

                    // === STA (ind,X) ===
                    0x81 => {
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        write!(addr, a);
                    }

                    // === LDA #imm ===
                    0xA9 => {
                        pc = pc.wrapping_add(1);
                        a = data as u8;
                        nz = data as u16;
                    }

                    // === LDA (ind,X) ===
                    0xA1 => {
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        a = read!(addr);
                        nz = a as u16;
                    }

                    // === LDA (ind),Y ===
                    0xB1 => {
                        let temp = read_low!(data as u16) as u16 + y as u16;
                        let addr =
                            temp + ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        if (temp & 0xFF00) != 0 {
                            s_time += 1;
                        }
                        a = read!(addr);
                        nz = a as u16;
                    }

                    // === LDA abs,Y ===
                    0xB9 => {
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a = read!(addr);
                        nz = a as u16;
                    }

                    // === LDA abs,X ===
                    0xBD => {
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a = read!(addr);
                        nz = a as u16;
                    }

                    // === BVC ===
                    0x50 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (status & ST_V) == 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1;
                        }
                    }

                    // === BVS ===
                    0x70 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (status & ST_V) != 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1;
                        }
                    }

                    // === BCS ===
                    0xB0 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (c & 0x100) != 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1;
                        }
                    }

                    // === BCC ===
                    0x90 => {
                        let offset = data as i8 as i16;
                        pc = pc.wrapping_add(1);
                        if (c & 0x100) == 0 {
                            let extra_clock = ((pc & 0xFF) as i16 + offset) as u16;
                            pc = (pc as i16 + offset) as u16;
                            s_time += (extra_clock >> 8) as i32 & 1;
                        } else {
                            s_time -= 1;
                        }
                    }

                    // === STY zp,x ===
                    0x94 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        write_low!(addr, y);
                    }

                    // === STY zp ===
                    0x84 => {
                        pc = pc.wrapping_add(1);
                        write_low!(data as u16, y);
                    }

                    // === STX zp,y ===
                    0x96 => {
                        let addr = (data as u8).wrapping_add(y) as u16;
                        pc = pc.wrapping_add(1);
                        write_low!(addr, x);
                    }

                    // === STX zp ===
                    0x86 => {
                        pc = pc.wrapping_add(1);
                        write_low!(data as u16, x);
                    }

                    // === LDX zp,y ===
                    0xB6 => {
                        data = read_low!((data as u8).wrapping_add(y) as u16) as i32;
                        x = data as u8;
                        nz = data as u16;
                        pc = pc.wrapping_add(1);
                    }

                    // === LDX zp ===
                    0xA6 => {
                        data = read_low!(data as u16) as i32;
                        x = data as u8;
                        nz = data as u16;
                        pc = pc.wrapping_add(1);
                    }

                    // === LDX #imm ===
                    0xA2 => {
                        pc = pc.wrapping_add(1);
                        x = data as u8;
                        nz = data as u16;
                    }

                    // === LDY zp,x ===
                    0xB4 => {
                        data = read_low!((data as u8).wrapping_add(x) as u16) as i32;
                        y = data as u8;
                        nz = data as u16;
                        pc = pc.wrapping_add(1);
                    }

                    // === LDY zp ===
                    0xA4 => {
                        data = read_low!(data as u16) as i32;
                        y = data as u8;
                        nz = data as u16;
                        pc = pc.wrapping_add(1);
                    }

                    // === LDY #imm ===
                    0xA0 => {
                        pc = pc.wrapping_add(1);
                        y = data as u8;
                        nz = data as u16;
                    }

                    // === LDY abs,X ===
                    0xBC => {
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        y = read!(addr);
                        nz = y as u16;
                    }

                    // === LDY abs ===
                    0xAC => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        y = read!(addr);
                        nz = y as u16;
                    }

                    // === LDX abs,y ===
                    0xBE => {
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        x = read!(addr);
                        nz = x as u16;
                    }

                    // === LDX abs ===
                    0xAE => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        x = read!(addr);
                        nz = x as u16;
                    }

                    // === STY abs ===
                    0x8C => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        write!(addr, y);
                    }

                    // === STX abs ===
                    0x8E => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        write!(addr, x);
                    }

                    // === CPX abs ===
                    0xEC => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        data = read!(addr) as i32;
                        nz = (x as i32 - data) as u16 & 0xFF;
                        c = if (x as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }

                    // === CPX zp ===
                    0xE4 => {
                        data = read_low!(data as u16) as i32;
                        nz = (x as i32 - data) as u16 & 0xFF;
                        c = if (x as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                        pc = pc.wrapping_add(1);
                    }

                    // === CPX #imm ===
                    0xE0 => {
                        pc = pc.wrapping_add(1);
                        nz = (x as i32 - data) as u16 & 0xFF;
                        c = if (x as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }

                    // === CPY abs ===
                    0xCC => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        data = read!(addr) as i32;
                        nz = (y as i32 - data) as u16 & 0xFF;
                        c = if (y as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }

                    // === CPY zp ===
                    0xC4 => {
                        data = read_low!(data as u16) as i32;
                        nz = (y as i32 - data) as u16 & 0xFF;
                        c = if (y as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                        pc = pc.wrapping_add(1);
                    }

                    // === CPY #imm ===
                    0xC0 => {
                        pc = pc.wrapping_add(1);
                        nz = (y as i32 - data) as u16 & 0xFF;
                        c = if (y as i32) >= data { 0x100 } else { 0 };
                        nz |= c;
                    }

                    // === AND (all addressing modes) ===
                    0x29 => {
                        // AND imm
                        pc = pc.wrapping_add(1);
                        a &= data as u8;
                        nz = a as u16;
                    }
                    0x25 => {
                        // AND zp
                        a &= read_low!(data as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x35 => {
                        // AND zp,x
                        a &= read_low!((data as u8).wrapping_add(x) as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x2D => {
                        // AND abs
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        a &= read!(addr);
                        nz = a as u16;
                    }
                    0x3D => {
                        // AND abs,x
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a &= read!(addr);
                        nz = a as u16;
                    }
                    0x39 => {
                        // AND abs,y
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a &= read!(addr);
                        nz = a as u16;
                    }
                    0x21 => {
                        // AND (ind,x)
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        a &= read!(addr);
                        nz = a as u16;
                    }
                    0x31 => {
                        // AND (ind),y
                        let temp = read_low!(data as u16) as u16
                            | ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        let addr = temp.wrapping_add(y as u16);
                        pc = pc.wrapping_add(1);
                        if (temp & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a &= read!(addr);
                        nz = a as u16;
                    }

                    // === EOR (all addressing modes) ===
                    0x49 => {
                        // EOR imm
                        pc = pc.wrapping_add(1);
                        a ^= data as u8;
                        nz = a as u16;
                    }
                    0x45 => {
                        // EOR zp
                        a ^= read_low!(data as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x55 => {
                        // EOR zp,x
                        a ^= read_low!((data as u8).wrapping_add(x) as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x4D => {
                        // EOR abs
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        a ^= read!(addr);
                        nz = a as u16;
                    }
                    0x5D => {
                        // EOR abs,x
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a ^= read!(addr);
                        nz = a as u16;
                    }
                    0x59 => {
                        // EOR abs,y
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a ^= read!(addr);
                        nz = a as u16;
                    }
                    0x41 => {
                        // EOR (ind,x)
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        a ^= read!(addr);
                        nz = a as u16;
                    }
                    0x51 => {
                        // EOR (ind),y
                        let temp = read_low!(data as u16) as u16
                            | ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        let addr = temp.wrapping_add(y as u16);
                        pc = pc.wrapping_add(1);
                        if (temp & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a ^= read!(addr);
                        nz = a as u16;
                    }

                    // === ORA (all addressing modes) ===
                    0x09 => {
                        // ORA imm
                        pc = pc.wrapping_add(1);
                        a |= data as u8;
                        nz = a as u16;
                    }
                    0x05 => {
                        // ORA zp
                        a |= read_low!(data as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x15 => {
                        // ORA zp,x
                        a |= read_low!((data as u8).wrapping_add(x) as u16);
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x0D => {
                        // ORA abs
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        a |= read!(addr);
                        nz = a as u16;
                    }
                    0x1D => {
                        // ORA abs,x
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a |= read!(addr);
                        nz = a as u16;
                    }
                    0x19 => {
                        // ORA abs,y
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a |= read!(addr);
                        nz = a as u16;
                    }
                    0x01 => {
                        // ORA (ind,x)
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        a |= read!(addr);
                        nz = a as u16;
                    }
                    0x11 => {
                        // ORA (ind),y
                        let temp = read_low!(data as u16) as u16
                            | ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        let addr = temp.wrapping_add(y as u16);
                        pc = pc.wrapping_add(1);
                        if (temp & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        a |= read!(addr);
                        nz = a as u16;
                    }

                    // === BIT ===
                    0x2C => {
                        // BIT abs
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        status &= !ST_V;
                        data = read!(addr) as i32;
                        status |= (data as u8) & ST_V;
                        nz = (data as u16) & (a as u16);
                        if nz == 0 {
                            nz = (data as u16 & 0x80) << 8; // preserve N flag but set Z
                        } else {
                            nz = data as u16;
                        }
                    }
                    0x24 => {
                        // BIT zp
                        data = read_low!(data as u16) as i32;
                        pc = pc.wrapping_add(1);
                        status &= !ST_V;
                        status |= (data as u8) & ST_V;
                        nz = (data as u16) & (a as u16);
                        if nz == 0 {
                            nz = (data as u16 & 0x80) << 8;
                        } else {
                            nz = data as u16;
                        }
                    }

                    // === SBC ===
                    0xE9 => {
                        // SBC imm
                        pc = pc.wrapping_add(1);
                        data ^= 0xFF;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xEB => {
                        // SBC unofficial
                        pc = pc.wrapping_add(1);
                        data ^= 0xFF;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xE5 => {
                        // SBC zp
                        data = read_low!(data as u16) as i32 ^ 0xFF;
                        pc = pc.wrapping_add(1);
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xF5 => {
                        // SBC zp,x
                        data = read_low!((data as u8).wrapping_add(x) as u16) as i32 ^ 0xFF;
                        pc = pc.wrapping_add(1);
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xED => {
                        // SBC abs
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        data = read!(addr) as i32 ^ 0xFF;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xFD => {
                        // SBC abs,x
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32 ^ 0xFF;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xF9 => {
                        // SBC abs,y
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32 ^ 0xFF;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xE1 => {
                        // SBC (ind,x)
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        data = read!(addr) as i32 ^ 0xFF;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0xF1 => {
                        // SBC (ind),y
                        let temp = read_low!(data as u16) as u16
                            | ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        let addr = temp.wrapping_add(y as u16);
                        pc = pc.wrapping_add(1);
                        if (temp & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32 ^ 0xFF;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }

                    // === ADC ===
                    0x69 => {
                        // ADC imm
                        pc = pc.wrapping_add(1);
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0x65 => {
                        // ADC zp
                        data = read_low!(data as u16) as i32;
                        pc = pc.wrapping_add(1);
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0x75 => {
                        // ADC zp,x
                        data = read_low!((data as u8).wrapping_add(x) as u16) as i32;
                        pc = pc.wrapping_add(1);
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0x6D => {
                        // ADC abs
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        data = read!(addr) as i32;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0x7D => {
                        // ADC abs,x
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0x79 => {
                        // ADC abs,y
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0x61 => {
                        // ADC (ind,x)
                        let temp = (data as u8).wrapping_add(x) as u16;
                        let addr = read_low!(temp) as u16
                            | ((read_low!(temp.wrapping_add(1) & 0xFF) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        data = read!(addr) as i32;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }
                    0x71 => {
                        // ADC (ind),y
                        let temp = read_low!(data as u16) as u16
                            | ((read_low!((data as u8).wrapping_add(1) as u16) as u16) << 8);
                        let addr = temp.wrapping_add(y as u16);
                        pc = pc.wrapping_add(1);
                        if (temp & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        data = read!(addr) as i32;
                        goto_adc!(a, data as u8, status, c, nz, pc);
                    }

                    // === LSR A ===
                    0x4A => {
                        c = 0;
                        nz = (a >> 1) as u16;
                        c |= (a as u16) << 8;
                        a = nz as u8;
                    }

                    // === ROR A ===
                    0x6A => {
                        nz = (c >> 1) & 0x80;
                        c = (a as u16) << 8;
                        nz |= (a >> 1) as u16;
                        a = nz as u8;
                    }

                    // === ASL A ===
                    0x0A => {
                        nz = (a as u16) << 1;
                        c = nz;
                        a = nz as u8;
                    }

                    // === ROL A ===
                    0x2A => {
                        nz = (a as u16) << 1;
                        let temp = (c >> 8) & 1;
                        c = nz;
                        nz |= temp;
                        a = nz as u8;
                    }

                    // === LSR abs,X ===
                    0x5E => {
                        let addr = get_le16!(instr).wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        c = 0;
                        let temp = read!(addr);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write!(addr, nz as u8);
                    }

                    // === LSR abs ===
                    0x4E => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        c = 0;
                        let temp = read!(addr);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write!(addr, nz as u8);
                    }

                    // === ROR abs ===
                    0x6E => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        let temp = read!(addr);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write!(addr, nz as u8);
                    }

                    // === ROL abs,X ===
                    0x3E => {
                        let addr = get_le16!(instr).wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        nz = (c >> 8) & 1;
                        let temp = read!(addr) as u16;
                        nz |= temp << 1;
                        c = temp << 1;
                        write!(addr, nz as u8);
                    }

                    // === ASL abs,X ===
                    0x1E => {
                        let addr = get_le16!(instr).wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        c = 0;
                        nz = (read!(addr) as u16) << 1;
                        c = nz;
                        write!(addr, nz as u8);
                    }

                    // === ASL abs ===
                    0x0E => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        c = 0;
                        nz = (read!(addr) as u16) << 1;
                        c = nz;
                        write!(addr, nz as u8);
                    }

                    // === ROL abs ===
                    0x2E => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        nz = (c >> 8) & 1;
                        let temp = read!(addr) as u16;
                        nz |= temp << 1;
                        c = temp << 1;
                        write!(addr, nz as u8);
                    }

                    // === ROR abs,X ===
                    0x7E => {
                        let addr = get_le16!(instr).wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        let temp = read!(addr);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write!(addr, nz as u8);
                    }

                    // === ROR zp,x ===
                    0x76 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        let temp = read_low!(addr);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write_low!(addr, nz as u8);
                    }

                    // === LSR zp,x ===
                    0x56 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        c = 0;
                        let temp = read_low!(addr);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write_low!(addr, nz as u8);
                    }

                    // === LSR zp ===
                    0x46 => {
                        pc = pc.wrapping_add(1);
                        c = 0;
                        let temp = read_low!(data as u16);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write_low!(data as u16, nz as u8);
                    }

                    // === ROR zp ===
                    0x66 => {
                        pc = pc.wrapping_add(1);
                        let temp = read_low!(data as u16);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        write_low!(data as u16, nz as u8);
                    }

                    // === ROL zp,x ===
                    0x36 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        nz = (c >> 8) & 1;
                        let temp = read_low!(addr) as u16;
                        nz |= temp << 1;
                        c = temp << 1;
                        write_low!(addr, nz as u8);
                    }

                    // === ASL zp,x ===
                    0x16 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        c = 0;
                        nz = (read_low!(addr) as u16) << 1;
                        c = nz;
                        write_low!(addr, nz as u8);
                    }

                    // === ASL zp ===
                    0x06 => {
                        pc = pc.wrapping_add(1);
                        c = 0;
                        nz = (read_low!(data as u16) as u16) << 1;
                        c = nz;
                        write_low!(data as u16, nz as u8);
                    }

                    // === ROL zp ===
                    0x26 => {
                        pc = pc.wrapping_add(1);
                        nz = (c >> 8) & 1;
                        let temp = read_low!(data as u16) as u16;
                        nz |= temp << 1;
                        c = temp << 1;
                        write_low!(data as u16, nz as u8);
                    }

                    // === DEX ===
                    0xCA => {
                        x = x.wrapping_sub(1);
                        nz = x as u16;
                    }

                    // === DEY ===
                    0x88 => {
                        y = y.wrapping_sub(1);
                        nz = y as u16;
                    }

                    // === INC zp,x ===
                    0xF6 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        nz = read_low!(addr).wrapping_add(1) as u16;
                        write_low!(addr, nz as u8);
                    }

                    // === INC zp ===
                    0xE6 => {
                        pc = pc.wrapping_add(1);
                        nz = read_low!(data as u16).wrapping_add(1) as u16;
                        write_low!(data as u16, nz as u8);
                    }

                    // === DEC zp,x ===
                    0xD6 => {
                        let addr = (data as u8).wrapping_add(x) as u16;
                        pc = pc.wrapping_add(1);
                        nz = read_low!(addr).wrapping_sub(1) as u16 & 0xFF;
                        write_low!(addr, nz as u8);
                    }

                    // === DEC zp ===
                    0xC6 => {
                        pc = pc.wrapping_add(1);
                        nz = read_low!(data as u16).wrapping_sub(1) as u16 & 0xFF;
                        write_low!(data as u16, nz as u8);
                    }

                    // === INC abs,x ===
                    0xFE => {
                        let addr = get_le16!(instr).wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        nz = read!(addr).wrapping_add(1) as u16;
                        write!(addr, nz as u8);
                    }

                    // === INC abs ===
                    0xEE => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        nz = read!(addr).wrapping_add(1) as u16;
                        write!(addr, nz as u8);
                    }

                    // === DEC abs,x ===
                    0xDE => {
                        let addr = get_le16!(instr).wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        nz = read!(addr).wrapping_sub(1) as u16 & 0xFF;
                        write!(addr, nz as u8);
                    }

                    // === DEC abs ===
                    0xCE => {
                        let addr = get_le16!(instr);
                        pc = pc.wrapping_add(2);
                        nz = read!(addr).wrapping_sub(1) as u16 & 0xFF;
                        write!(addr, nz as u8);
                    }

                    // === TAX ===
                    0xAA => {
                        x = a;
                        nz = a as u16;
                    }

                    // === TXA ===
                    0x8A => {
                        a = x;
                        nz = x as u16;
                    }

                    // === TXS ===
                    0x9A => {
                        sp = (x as u16 + 1) | 0x100;
                    }

                    // === TSX ===
                    0xBA => {
                        x = get_sp!() as u8;
                        nz = x as u16;
                    }

                    // === PHA ===
                    0x48 => {
                        push!(a);
                    }

                    // === PLA ===
                    0x68 => {
                        a = read_low!(sp);
                        nz = a as u16;
                        sp = (sp.wrapping_sub(0xFF)) | 0x100;
                    }

                    // === RTI ===
                    0x40 => {
                        let temp = read_low!(sp);
                        pc = read_low!(0x100u16 | (sp.wrapping_sub(0xFF))) as u16;
                        pc |= (read_low!(0x100u16 | (sp.wrapping_sub(0xFE))) as u16) << 8;
                        sp = (sp.wrapping_sub(0xFD)) | 0x100;
                        let old_status = status;
                        set_status!(temp);
                        self.r.status = status;
                        if (old_status ^ status) & ST_I != 0 {
                            let mut new_time = self.end_time;
                            if (status & ST_I) == 0 && new_time > self.irq_time {
                                new_time = self.irq_time;
                            }
                            let delta = s_base - new_time;
                            s_base = new_time;
                            s_time += delta;
                        }
                    }

                    // === PLP ===
                    0x28 => {
                        let temp = read_low!(sp);
                        sp = (sp.wrapping_sub(0xFF)) | 0x100;
                        let changed = status ^ temp;
                        set_status!(temp);
                        if (changed & ST_I) != 0 {
                            if (status & ST_I) != 0 {
                                // SEI
                                self.r.status = status;
                                let delta = s_base - self.end_time;
                                s_base = self.end_time;
                                s_time += delta;
                            } else {
                                // CLI
                                self.r.status = status;
                                let delta = s_base - self.irq_time;
                                if delta > 0 {
                                    s_base = self.irq_time;
                                    s_time += delta;
                                }
                            }
                        }
                    }

                    // === PHP ===
                    0x08 => {
                        calc_status!(temp);
                        push!(temp | ST_B | ST_R);
                    }

                    // === JMP (ind) ===
                    0x6C => {
                        let addr = get_le16!(instr);
                        pc = read_prog!(addr) as u16;
                        let hi_addr = (addr & 0xFF00) | ((addr.wrapping_add(1)) & 0xFF);
                        pc |= (read_prog!(hi_addr) as u16) << 8;
                    }

                    // === SEC ===
                    0x38 => {
                        c = 0xFFFF; // carry set
                    }

                    // === CLC ===
                    0x18 => {
                        c = 0;
                    }

                    // === CLV ===
                    0xB8 => {
                        status &= !ST_V;
                    }

                    // === CLD ===
                    0xD8 => {
                        status &= !ST_D;
                    }

                    // === SED ===
                    0xF8 => {
                        status |= ST_D;
                    }

                    // === CLI ===
                    0x58 => {
                        if (status & ST_I) != 0 {
                            status &= !ST_I;
                            self.r.status = status;
                            let delta = s_base - self.irq_time;
                            if delta > 0 {
                                s_base = self.irq_time;
                                s_time += delta;
                                if s_time >= 0 {
                                    // Delayed CLI
                                }
                            }
                        }
                    }

                    // === SEI ===
                    0x78 => {
                        if (status & ST_I) == 0 {
                            status |= ST_I;
                            self.r.status = status;
                            let delta = s_base - self.end_time;
                            s_base = self.end_time;
                            s_time += delta;
                        }
                    }

                    // === Unofficial: SKW (skip word) ===
                    0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                        let base = get_le16!(instr);
                        let addr = base.wrapping_add(x as u16);
                        if (base & 0xFF00) != (addr & 0xFF00) {
                            s_time += 1;
                        }
                        pc = pc.wrapping_add(2);
                    }
                    0x0C => {
                        pc = pc.wrapping_add(2);
                    }

                    // === Unofficial: SKB (skip byte) ===
                    0x74 | 0x04 | 0x14 | 0x34 | 0x44 | 0x54 | 0x64 | 0x82 | 0x89 | 0xC2 | 0xD4
                    | 0xE2 | 0xF4 => {
                        pc = pc.wrapping_add(1);
                    }

                    // === NOP ===
                    0xEA | 0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {}

                    // === Unofficial NOPs with operand ===
                    0x80 => {
                        pc = pc.wrapping_add(1);
                    }

                    // === Unofficial opcodes - treat as NOPs ===
                    // These are "undocumented" 6502 opcodes that some programs use.
                    // Most are combinations of other instructions. We treat them as
                    // NOPs with appropriate operand skipping to match C++ behavior.
                    0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xB2 | 0xD2
                    | 0xF2 => {
                        // HLT/KIL - halt the CPU. Treat as illegal.
                        result = CpuResult::Illegal;
                        pc = pc.wrapping_sub(1);
                        break 'main_loop;
                    }

                    _ => {
                        // Unknown opcode - stop CPU (matches C++ behavior)
                        result = CpuResult::Illegal;
                        pc = pc.wrapping_sub(1);
                        break 'main_loop;
                    }
                }
            }
        } // end unsafe block

        // Write back state
        self.r.pc = pc;
        self.r.sp = get_sp!() as u8;
        self.r.a = a;
        self.r.x = x;
        self.r.y = y;
        calc_status!(final_status);
        self.r.status = final_status;
        self.state.base = s_base;
        self.state.time = s_time;

        result
    }
}

/// ADC/SBC helper macro - implements the add with carry logic.
macro_rules! goto_adc {
    ($a:expr, $data:expr, $status:expr, $c:expr, $nz:expr, $pc:expr) => {{
        let carry = ($c >> 8) & 1;
        let a_val = $a as i32;
        let d = $data as i32;
        let ov = (a_val ^ 0x80) + carry as i32 + (($data as i8) as i32); // sign-extend for overflow
        $status &= !ST_V;
        $status |= ((ov >> 2) & 0x40) as u8;
        let result = a_val + (d & 0xFF) + carry as i32;
        $c = (result as u16) & 0x1FF;
        $nz = (result as u16) & 0xFF;
        if $nz == 0 {
            // Z flag already set by nz & 0xFF == 0
        }
        $a = result as u8;
    }};
}

use goto_adc;
