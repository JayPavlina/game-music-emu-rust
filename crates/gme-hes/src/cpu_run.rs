// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2003-2006 Shay Green
// Rust adaptation of Hes_Cpu.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! HuC6280 CPU interpreter — port of C++ Hes_Cpu::run (Hes_Cpu.cpp) with the
//! memory-access glue from hes_cpu_io.h inlined as `HesEmu` methods/macros.
//!
//! IMPORTANT width note: the reference build runs on Darwin where
//! `uint_fast16_t` == `uint16_t` and `uint_fast8_t` == `uint8_t`, so all of
//! pc/data/addr/c/nz arithmetic wraps at 16 bits and a/x/y/status at 8 bits.
//! This port replicates exactly those widths.

use crate::cpu::{
    CLOCK_TABLE, HesTime, IDLE_ADDR, PAGE_SHIFT, PageRef, ST_B, ST_C, ST_D, ST_I, ST_N, ST_V, ST_Z,
    WritePage,
};
use crate::hes_emu::HesEmu;

impl HesEmu {
    /// Read a byte from a captured code page (C++ `instr [i]` / READ_PROG).
    /// `idx` may exceed the page size by up to CPU_PAD bytes.
    #[inline]
    pub(crate) fn page_byte(&self, page: PageRef, idx: usize) -> u8 {
        match page {
            PageRef::Rom(o) => self.rom.data[o + idx],
            PageRef::Ram => self.cpu.ram[idx],
            PageRef::Sgx(o) => self.sgx[o + idx],
        }
    }

    /// C++ Hes_Cpu::run. Returns true if an illegal instruction was hit.
    #[allow(unused_assignments)]
    pub(crate) fn cpu_run(&mut self, end_time: HesTime) -> bool {
        let mut illegal_encountered = false;
        self.cpu_set_end_time(end_time);

        // Local cache of state->time (C++ s_time); state->base lives in
        // self.cpu.state_base, kept live so I/O callbacks see it.
        let mut s_time: i32 = self.cpu.state_time;

        // Registers (C++ uint_fast16_t == u16, uint_fast8_t == u8 on Darwin)
        let mut pc: u16 = self.cpu.r.pc;
        let mut a: u8 = self.cpu.r.a;
        let mut x: u8 = self.cpu.r.x;
        let mut y: u8 = self.cpu.r.y;
        let mut sp: u16 = (self.cpu.r.sp as u16 + 1) | 0x100; // SET_SP

        let mut status: u8;
        let mut c: u16; // carry set if (c & 0x100) != 0
        let mut nz: u16; // Z set if (nz & 0xFF) == 0, N set if (nz & 0x8080) != 0

        macro_rules! flush_time {
            () => {
                self.cpu.state_time = s_time
            };
        }
        macro_rules! cache_time {
            () => {
                s_time = self.cpu.state_time
            };
        }
        macro_rules! read_low {
            ($addr:expr) => {
                self.cpu.ram[($addr) as usize]
            };
        }
        macro_rules! write_low {
            ($addr:expr, $v:expr) => {
                self.cpu.ram[($addr) as usize] = ($v) as u8
            };
        }
        macro_rules! push {
            ($v:expr) => {
                sp = sp.wrapping_sub(1) | 0x100;
                write_low!(sp, $v);
            };
        }
        macro_rules! pop {
            () => {{
                let v = read_low!(sp);
                sp = sp.wrapping_sub(0xFF) | 0x100;
                v
            }};
        }
        macro_rules! set_status {
            ($in_:expr) => {{
                let in_: u8 = $in_;
                status = in_ & (ST_V | ST_D | ST_I);
                nz = (in_ as u16) << 8;
                c = nz;
                nz |= (!in_ & ST_Z) as u16;
            }};
        }
        macro_rules! calc_status {
            () => {{
                let mut out = status & (ST_V | ST_D | ST_I);
                out |= (((nz >> 8) | nz) as u8) & ST_N;
                out |= ((c >> 8) as u8) & ST_C;
                if nz & 0xFF == 0 {
                    out |= ST_Z;
                }
                out
            }};
        }
        // CPU_READ_FAST (hes_cpu_io.h)
        macro_rules! read_fast {
            ($addr:expr) => {{
                let addr_: u16 = $addr;
                let page = (addr_ >> PAGE_SHIFT) as usize;
                let mut out =
                    self.page_byte(self.cpu.code_map[page], (addr_ & 0x1FFF) as usize) as u16;
                if self.cpu.mmr[page] == 0xFF {
                    flush_time!();
                    out = self.cpu_read_io(addr_) as u16;
                    cache_time!();
                }
                out
            }};
        }
        // CPU_WRITE_FAST (hes_cpu_io.h)
        macro_rules! write_fast {
            ($addr:expr, $data:expr) => {{
                let addr_: u16 = $addr;
                let data_ = $data;
                let page = (addr_ >> PAGE_SHIFT) as usize;
                let masked = (addr_ & 0x1FFF) as usize;
                match self.write_pages[page] {
                    WritePage::Ram => self.cpu.ram[masked] = data_ as u8,
                    WritePage::Sgx(o) => self.sgx[o + masked] = data_ as u8,
                    WritePage::None => {
                        if self.cpu.mmr[page] == 0xFF {
                            flush_time!();
                            self.cpu_write_io(masked as u16, data_ as i32);
                            cache_time!();
                        }
                    }
                }
            }};
        }
        // Full READ/WRITE (Hes_Emu::cpu_read / cpu_write); call sites add
        // flush/cache exactly where C++ does.
        macro_rules! read_full {
            ($addr:expr) => {
                self.cpu_read_full($addr)
            };
        }
        macro_rules! write_full {
            ($addr:expr, $data:expr) => {
                self.cpu_write_full($addr, $data as i32)
            };
        }
        macro_rules! handle_cli {
            () => {{
                self.cpu.r.status = status; // update externally-visible I flag
                let delta = self.cpu.state_base - self.cpu.irq_time;
                if delta > 0 {
                    self.cpu.state_base = self.cpu.irq_time;
                    s_time += delta;
                    if s_time >= 0 {
                        // C++: delta >= s_time + 1
                        if delta > s_time {
                            // delayed irq until after next instruction
                            self.cpu.state_base += s_time + 1;
                            s_time = -1;
                            self.cpu.irq_time = self.cpu.state_base;
                        }
                        // else: delayed CLI not supported
                    }
                }
                // delta <= 0: either TIME < irq_time (nothing to do) or
                // delayed CLI (not supported)
            }};
        }
        macro_rules! handle_sei {
            () => {{
                self.cpu.r.status = status; // update externally-visible I flag
                let delta = self.cpu.state_base - self.cpu.end_time;
                self.cpu.state_base = self.cpu.end_time;
                s_time += delta;
                // s_time >= 0: delayed SEI not supported
            }};
        }

        {
            let temp = self.cpu.r.status;
            set_status!(temp);
        }

        'main: loop {
            // ---- loop: fetch ----
            let fpage = self.cpu.code_map[(pc >> PAGE_SHIFT) as usize];
            let foff = (pc & 0x1FFF) as usize;
            let opcode = self.page_byte(fpage, foff);
            pc = pc.wrapping_add(1);

            macro_rules! instr {
                ($i:expr) => {
                    self.page_byte(fpage, foff + 1 + $i)
                };
            }
            macro_rules! get_msb {
                () => {
                    instr!(1)
                };
            }
            macro_rules! get_addr {
                () => {
                    (instr!(0) as u16) | ((instr!(1) as u16) << 8)
                };
            }

            let clocks = CLOCK_TABLE[opcode as usize] as i32;
            s_time += clocks;

            let mut go_out_of_time = false;
            let mut irq_vector: i32 = 0;

            if s_time >= clocks {
                // out of time before executing this instruction
                s_time -= clocks;
                go_out_of_time = true;
            } else {
                let mut data: u16 = instr!(0) as u16;

                macro_rules! branch {
                    ($cond:expr) => {{
                        let offset = (data as u8 as i8) as i16;
                        pc = pc.wrapping_add(1);
                        if $cond {
                            pc = pc.wrapping_add(offset as u16);
                        } else {
                            s_time -= 2; // branch_not_taken
                        }
                    }};
                }
                macro_rules! bbr_bbs {
                    () => {{
                        let t = (0x101u16.wrapping_mul(read_low!(data as u8) as u16)) ^ 0xFF;
                        pc = pc.wrapping_add(1);
                        data = get_msb!() as u16;
                        branch!(t & (1u16 << (opcode >> 4)) != 0);
                    }};
                }
                // ADD_PAGE(out): pc++, out = data + 0x100 * MSB (16-bit wrap)
                macro_rules! add_page {
                    ($out:ident) => {{
                        pc = pc.wrapping_add(1);
                        $out = data.wrapping_add((get_msb!() as u16) << 8);
                    }};
                }
                // ARITH_ADDR_MODES: resolves the operand into `data`.
                macro_rules! arith_operand {
                    ($base:expr) => {{
                        let delta = opcode.wrapping_sub($base);
                        match delta {
                            0xFC | 0x0D => {
                                // (ind,X) / (ind)
                                if delta == 0xFC {
                                    data = (data as u8).wrapping_add(x) as u16;
                                }
                                data = ((read_low!((data as u8).wrapping_add(1)) as u16) << 8)
                                    .wrapping_add(read_low!(data as u8) as u16);
                                flush_time!();
                                data = read_full!(data) as u16;
                                cache_time!();
                            }
                            0x0C => {
                                // (ind),Y
                                let temp = (read_low!(data as u8) as u16).wrapping_add(y as u16);
                                data = temp.wrapping_add(
                                    (read_low!((data as u8).wrapping_add(1)) as u16) << 8,
                                );
                                flush_time!();
                                data = read_full!(data) as u16;
                                cache_time!();
                            }
                            0x10 | 0x00 => {
                                // zp,X / zp
                                if delta == 0x10 {
                                    data = (data as u8).wrapping_add(x) as u16;
                                }
                                data = read_low!(data as u8) as u16;
                            }
                            0x14 | 0x18 | 0x08 => {
                                // abs,Y / abs,X / abs
                                if delta == 0x14 {
                                    data = data.wrapping_add(y as u16);
                                }
                                if delta == 0x18 {
                                    data = data.wrapping_add(x as u16);
                                }
                                let addr;
                                add_page!(addr);
                                data = addr;
                                flush_time!();
                                data = read_full!(data) as u16;
                                cache_time!();
                            }
                            _ => {} // 0x04: imm — operand already in data
                        }
                    }};
                }
                macro_rules! adc_body {
                    () => {{
                        // (decimal mode not supported)
                        let carry = (c >> 8) & 1;
                        let ov: i16 = (a ^ 0x80) as i16 + carry as i16 + (data as u8 as i8) as i16;
                        status &= !ST_V;
                        status |= ((ov >> 2) & 0x40) as u8;
                        c = (a as u16).wrapping_add(data).wrapping_add(carry);
                        nz = c;
                        pc = pc.wrapping_add(1);
                        a = nz as u8;
                    }};
                }

                match opcode {
                    // ---- Branch ----
                    0xF0 => branch!(nz as u8 == 0),      // BEQ
                    0xD0 => branch!(nz as u8 != 0),      // BNE
                    0x10 => branch!(nz & 0x8080 == 0),   // BPL
                    0x90 => branch!(c & 0x100 == 0),     // BCC
                    0x30 => branch!(nz & 0x8080 != 0),   // BMI
                    0x50 => branch!(status & ST_V == 0), // BVC
                    0x70 => branch!(status & ST_V != 0), // BVS
                    0xB0 => branch!(c & 0x100 != 0),     // BCS
                    0x80 => branch!(true),               // BRA

                    0xFF => {
                        if pc == IDLE_ADDR + 1 {
                            // idle_done
                            s_time = 0;
                            go_out_of_time = true;
                        } else {
                            bbr_bbs!();
                        }
                    }
                    // BBRn / BBSn
                    0x0F | 0x1F | 0x2F | 0x3F | 0x4F | 0x5F | 0x6F | 0x7F | 0x8F | 0x9F | 0xAF
                    | 0xBF | 0xCF | 0xDF | 0xEF => bbr_bbs!(),

                    0x4C => pc = get_addr!(), // JMP abs

                    0x7C | 0x6C => {
                        // JMP (ind+X) / JMP (ind)
                        if opcode == 0x7C {
                            data = data.wrapping_add(x as u16);
                        }
                        data = data.wrapping_add((get_msb!() as u16) << 8);
                        let page = self.cpu.code_map[(data >> PAGE_SHIFT) as usize];
                        let off = (data & 0x1FFF) as usize;
                        pc = self.page_byte(page, off) as u16
                            | ((self.page_byte(page, off + 1) as u16) << 8);
                    }

                    // ---- Subroutine ----
                    0x44 => {
                        // BSR
                        write_low!(0x100 | sp.wrapping_sub(1), (pc >> 8) as u8);
                        sp = sp.wrapping_sub(2) | 0x100;
                        write_low!(sp, pc as u8);
                        branch!(true);
                    }
                    0x20 => {
                        // JSR
                        let temp = pc.wrapping_add(1);
                        pc = get_addr!();
                        write_low!(0x100 | sp.wrapping_sub(1), (temp >> 8) as u8);
                        sp = sp.wrapping_sub(2) | 0x100;
                        write_low!(sp, temp as u8);
                    }
                    0x60 => {
                        // RTS
                        pc = (read_low!(0x100 | sp.wrapping_sub(0xFF)) as u16) << 8;
                        pc = pc.wrapping_add(1).wrapping_add(read_low!(sp) as u16);
                        sp = sp.wrapping_sub(0xFE) | 0x100;
                    }
                    0x00 => {
                        // BRK
                        pc = pc.wrapping_add(1);
                        irq_vector = 6;
                    }

                    // ---- Common ----
                    0xBD => {
                        // LDA abs,X
                        let addr = get_addr!().wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        nz = read_fast!(addr);
                        a = nz as u8;
                    }
                    0x9D => {
                        // STA abs,X
                        let addr = get_addr!().wrapping_add(x as u16);
                        pc = pc.wrapping_add(2);
                        write_fast!(addr, a);
                    }
                    0x95 | 0x85 => {
                        // STA zp,x / STA zp
                        if opcode == 0x95 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        pc = pc.wrapping_add(1);
                        write_low!(data, a);
                    }
                    0xAE => {
                        // LDX abs
                        let addr = get_addr!();
                        pc = pc.wrapping_add(2);
                        nz = read_fast!(addr);
                        x = nz as u8;
                    }
                    0xA5 => {
                        // LDA zp
                        nz = read_low!(data) as u16;
                        a = nz as u8;
                        pc = pc.wrapping_add(1);
                    }

                    // ---- Load/store ----
                    0x91 => {
                        // STA (ind),Y
                        let mut addr = (read_low!((data as u8).wrapping_add(1)) as u16) << 8;
                        addr = addr
                            .wrapping_add(read_low!(data as u8) as u16)
                            .wrapping_add(y as u16);
                        pc = pc.wrapping_add(1);
                        write_fast!(addr, a);
                    }
                    0x81 | 0x92 => {
                        // STA (ind,X) / STA (ind)
                        if opcode == 0x81 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        let mut addr = (read_low!((data as u8).wrapping_add(1)) as u16) << 8;
                        addr = addr.wrapping_add(read_low!(data as u8) as u16);
                        pc = pc.wrapping_add(1);
                        write_fast!(addr, a);
                    }
                    0x99 | 0x8D => {
                        // STA abs,Y / STA abs
                        if opcode == 0x99 {
                            data = data.wrapping_add(y as u16);
                        }
                        let addr = data.wrapping_add((get_msb!() as u16) << 8);
                        pc = pc.wrapping_add(2);
                        write_fast!(addr, a);
                    }
                    0xA1 | 0xB2 => {
                        // LDA (ind,X) / LDA (ind)
                        if opcode == 0xA1 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        let mut addr = (read_low!((data as u8).wrapping_add(1)) as u16) << 8;
                        addr = addr.wrapping_add(read_low!(data as u8) as u16);
                        pc = pc.wrapping_add(1);
                        nz = read_fast!(addr);
                        a = nz as u8;
                    }
                    0xB1 => {
                        // LDA (ind),Y
                        let mut addr = (read_low!(data as u8) as u16).wrapping_add(y as u16);
                        addr = addr
                            .wrapping_add((read_low!((data as u8).wrapping_add(1)) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        nz = read_fast!(addr);
                        a = nz as u8;
                    }
                    0xB9 | 0xAD => {
                        // LDA abs,Y / LDA abs
                        if opcode == 0xB9 {
                            data = data.wrapping_add(y as u16);
                        }
                        let addr = data.wrapping_add((get_msb!() as u16) << 8);
                        pc = pc.wrapping_add(2);
                        nz = read_fast!(addr);
                        a = nz as u8;
                    }
                    0xBE => {
                        // LDX abs,Y
                        let addr = get_addr!().wrapping_add(y as u16);
                        pc = pc.wrapping_add(2);
                        flush_time!();
                        nz = read_full!(addr) as u16;
                        cache_time!();
                        x = nz as u8;
                    }
                    0xB5 => {
                        // LDA zp,x
                        nz = read_low!((data as u8).wrapping_add(x)) as u16;
                        a = nz as u8;
                        pc = pc.wrapping_add(1);
                    }
                    0xA9 => {
                        // LDA #imm
                        pc = pc.wrapping_add(1);
                        a = data as u8;
                        nz = data;
                    }

                    // ---- Bit operations ----
                    0x3C | 0x2C => {
                        // BIT abs,x / BIT abs
                        if opcode == 0x3C {
                            data = data.wrapping_add(x as u16);
                        }
                        let addr;
                        add_page!(addr);
                        flush_time!();
                        nz = read_full!(addr) as u16;
                        cache_time!();
                        // bit_common
                        pc = pc.wrapping_add(1);
                        status &= !ST_V;
                        status |= (nz as u8) & ST_V;
                        if nz & a as u16 == 0 {
                            nz <<= 8; // set Z flag without affecting N flag
                        }
                    }
                    0x34 | 0x24 | 0x89 => {
                        // BIT zp,x / BIT zp / BIT imm
                        if opcode == 0x34 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        if opcode != 0x89 {
                            data = read_low!(data as u8) as u16;
                        }
                        nz = data;
                        // bit_common
                        pc = pc.wrapping_add(1);
                        status &= !ST_V;
                        status |= (nz as u8) & ST_V;
                        if nz & a as u16 == 0 {
                            nz <<= 8;
                        }
                    }
                    0xB3 | 0x93 => {
                        // TST abs,x / TST abs
                        let mut addr = get_msb!() as u16;
                        if opcode == 0xB3 {
                            addr = addr.wrapping_add(x as u16);
                        }
                        addr = addr.wrapping_add((instr!(2) as u16) << 8);
                        pc = pc.wrapping_add(1);
                        flush_time!();
                        nz = read_full!(addr) as u16;
                        cache_time!();
                        // tst_common
                        pc = pc.wrapping_add(2);
                        status &= !ST_V;
                        status |= (nz as u8) & ST_V;
                        if nz & data == 0 {
                            nz <<= 8;
                        }
                    }
                    0xA3 | 0x83 => {
                        // TST zp,x / TST zp
                        nz = if opcode == 0xA3 {
                            read_low!(get_msb!().wrapping_add(x)) as u16
                        } else {
                            read_low!(get_msb!()) as u16
                        };
                        // tst_common
                        pc = pc.wrapping_add(2);
                        status &= !ST_V;
                        status |= (nz as u8) & ST_V;
                        if nz & data == 0 {
                            nz <<= 8;
                        }
                    }
                    0x0C | 0x1C | 0x04 | 0x14 => {
                        // TSB abs / TRB abs / TSB zp / TRB zp
                        let addr: u16 = if opcode & 0x08 != 0 {
                            let addr = get_addr!();
                            pc = pc.wrapping_add(1);
                            addr
                        } else {
                            data.wrapping_add(0x2000) // ram_addr
                        };
                        flush_time!();
                        nz = (a as u16) | read_full!(addr) as u16;
                        if opcode & 0x10 != 0 {
                            nz ^= a as u16; // TRB clears bits from a
                        }
                        status &= !ST_V;
                        status |= (nz as u8) & ST_V;
                        pc = pc.wrapping_add(1);
                        write_full!(addr, nz);
                        cache_time!();
                    }
                    0x07 | 0x17 | 0x27 | 0x37 | 0x47 | 0x57 | 0x67 | 0x77 => {
                        // RMBn
                        pc = pc.wrapping_add(1);
                        let v = read_low!(data) & !(1u8 << (opcode >> 4));
                        write_low!(data, v);
                    }
                    0x87 | 0x97 | 0xA7 | 0xB7 | 0xC7 | 0xD7 | 0xE7 | 0xF7 => {
                        // SMBn
                        pc = pc.wrapping_add(1);
                        let v = read_low!(data) | (1u8 << ((opcode >> 4) - 8));
                        write_low!(data, v);
                    }

                    // ---- Load/store ----
                    0x9E | 0x9C => {
                        // STZ abs,x / STZ abs
                        if opcode == 0x9E {
                            data = data.wrapping_add(x as u16);
                        }
                        let addr;
                        add_page!(addr);
                        data = addr;
                        pc = pc.wrapping_add(1);
                        flush_time!();
                        write_full!(data, 0);
                        cache_time!();
                    }
                    0x74 | 0x64 => {
                        // STZ zp,x / STZ zp
                        if opcode == 0x74 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        pc = pc.wrapping_add(1);
                        write_low!(data, 0);
                    }
                    0x94 | 0x84 => {
                        // STY zp,x / STY zp
                        if opcode == 0x94 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        pc = pc.wrapping_add(1);
                        write_low!(data, y);
                    }
                    0x96 | 0x86 => {
                        // STX zp,y / STX zp
                        if opcode == 0x96 {
                            data = (data as u8).wrapping_add(y) as u16;
                        }
                        pc = pc.wrapping_add(1);
                        write_low!(data, x);
                    }
                    0xB6 | 0xA6 | 0xA2 => {
                        // LDX zp,y / LDX zp / LDX #imm
                        if opcode == 0xB6 {
                            data = (data as u8).wrapping_add(y) as u16;
                        }
                        if opcode != 0xA2 {
                            data = read_low!(data as u8) as u16;
                        }
                        pc = pc.wrapping_add(1);
                        x = data as u8;
                        nz = data;
                    }
                    0xB4 | 0xA4 | 0xA0 => {
                        // LDY zp,x / LDY zp / LDY #imm
                        if opcode == 0xB4 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        if opcode != 0xA0 {
                            data = read_low!(data as u8) as u16;
                        }
                        pc = pc.wrapping_add(1);
                        y = data as u8;
                        nz = data;
                    }
                    0xBC | 0xAC => {
                        // LDY abs,X / LDY abs
                        if opcode == 0xBC {
                            data = data.wrapping_add(x as u16);
                        }
                        let addr = data.wrapping_add((get_msb!() as u16) << 8);
                        pc = pc.wrapping_add(2);
                        flush_time!();
                        nz = read_full!(addr) as u16;
                        cache_time!();
                        y = nz as u8;
                    }
                    0x8C | 0x8E => {
                        // STY abs / STX abs
                        let temp = if opcode == 0x8C { y } else { x };
                        let addr = get_addr!();
                        pc = pc.wrapping_add(2);
                        flush_time!();
                        write_full!(addr, temp);
                        cache_time!();
                    }

                    // ---- Compare ----
                    0xEC | 0xE4 | 0xE0 => {
                        // CPX abs / zp / imm
                        if opcode == 0xEC {
                            let addr = get_addr!();
                            pc = pc.wrapping_add(1);
                            flush_time!();
                            data = read_full!(addr) as u16;
                            cache_time!();
                        } else if opcode == 0xE4 {
                            data = read_low!(data as u8) as u16;
                        }
                        nz = (x as u16).wrapping_sub(data);
                        pc = pc.wrapping_add(1);
                        c = !nz;
                        nz &= 0xFF;
                    }
                    0xCC | 0xC4 | 0xC0 => {
                        // CPY abs / zp / imm
                        if opcode == 0xCC {
                            let addr = get_addr!();
                            pc = pc.wrapping_add(1);
                            flush_time!();
                            data = read_full!(addr) as u16;
                            cache_time!();
                        } else if opcode == 0xC4 {
                            data = read_low!(data as u8) as u16;
                        }
                        nz = (y as u16).wrapping_sub(data);
                        pc = pc.wrapping_add(1);
                        c = !nz;
                        nz &= 0xFF;
                    }

                    // ---- Logical ----
                    0xC1 | 0xD2 | 0xD1 | 0xD5 | 0xC5 | 0xD9 | 0xDD | 0xCD | 0xC9 => {
                        // CMP
                        arith_operand!(0xC5);
                        nz = (a as u16).wrapping_sub(data);
                        pc = pc.wrapping_add(1);
                        c = !nz;
                        nz &= 0xFF;
                    }
                    0x21 | 0x32 | 0x31 | 0x35 | 0x25 | 0x39 | 0x3D | 0x2D | 0x29 => {
                        // AND
                        arith_operand!(0x25);
                        a &= data as u8;
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x41 | 0x52 | 0x51 | 0x55 | 0x45 | 0x59 | 0x5D | 0x4D | 0x49 => {
                        // EOR
                        arith_operand!(0x45);
                        a ^= data as u8;
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }
                    0x01 | 0x12 | 0x11 | 0x15 | 0x05 | 0x19 | 0x1D | 0x0D | 0x09 => {
                        // ORA
                        arith_operand!(0x05);
                        a |= data as u8;
                        nz = a as u16;
                        pc = pc.wrapping_add(1);
                    }

                    // ---- Add/subtract ----
                    0xE1 | 0xF2 | 0xF1 | 0xF5 | 0xE5 | 0xF9 | 0xFD | 0xED | 0xE9 => {
                        // SBC
                        arith_operand!(0xE5);
                        data ^= 0xFF;
                        adc_body!();
                    }
                    0x61 | 0x72 | 0x71 | 0x75 | 0x65 | 0x79 | 0x7D | 0x6D | 0x69 => {
                        // ADC
                        arith_operand!(0x65);
                        adc_body!();
                    }

                    // ---- Shift/rotate ----
                    0x4A | 0x6A => {
                        // LSR A / ROR A
                        if opcode == 0x4A {
                            c = 0;
                        }
                        nz = (c >> 1) & 0x80;
                        c = (a as u16) << 8;
                        nz |= (a >> 1) as u16;
                        a = nz as u8;
                    }
                    0x0A => {
                        // ASL A
                        nz = (a as u16) << 1;
                        c = nz;
                        a = nz as u8;
                    }
                    0x2A => {
                        // ROL A
                        nz = (a as u16) << 1;
                        let temp = (c >> 8) & 1;
                        c = nz;
                        nz |= temp;
                        a = nz as u8;
                    }
                    0x5E | 0x4E | 0x6E | 0x7E => {
                        // LSR abs,X / LSR abs / ROR abs / ROR abs,X
                        if opcode == 0x5E || opcode == 0x7E {
                            data = data.wrapping_add(x as u16);
                        }
                        if opcode == 0x5E || opcode == 0x4E {
                            c = 0;
                        }
                        // ror_abs
                        let addr;
                        add_page!(addr);
                        data = addr;
                        flush_time!();
                        let temp = read_full!(data);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        // rotate_common
                        pc = pc.wrapping_add(1);
                        write_full!(data, nz as u8);
                        cache_time!();
                    }
                    0x3E | 0x1E | 0x0E | 0x2E => {
                        // ROL abs,X / ASL abs,X / ASL abs / ROL abs
                        if opcode == 0x3E || opcode == 0x1E {
                            data = data.wrapping_add(x as u16);
                        }
                        if opcode == 0x1E || opcode == 0x0E {
                            c = 0;
                        }
                        // rol_abs
                        let addr;
                        add_page!(addr);
                        data = addr;
                        nz = (c >> 8) & 1;
                        flush_time!();
                        c = (read_full!(data) as u16) << 1;
                        nz |= c;
                        // rotate_common
                        pc = pc.wrapping_add(1);
                        write_full!(data, nz as u8);
                        cache_time!();
                    }
                    0x76 | 0x56 | 0x46 | 0x66 => {
                        // ROR zp,x / LSR zp,x / LSR zp / ROR zp
                        if opcode == 0x76 || opcode == 0x56 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        if opcode == 0x56 || opcode == 0x46 {
                            c = 0;
                        }
                        // ror_zp
                        let temp = read_low!(data);
                        nz = ((c >> 1) & 0x80) | ((temp >> 1) as u16);
                        c = (temp as u16) << 8;
                        // write_nz_zp
                        pc = pc.wrapping_add(1);
                        write_low!(data, nz as u8);
                    }
                    0x36 | 0x16 | 0x06 | 0x26 => {
                        // ROL zp,x / ASL zp,x / ASL zp / ROL zp
                        if opcode == 0x36 || opcode == 0x16 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        if opcode == 0x16 || opcode == 0x06 {
                            c = 0;
                        }
                        // rol_zp
                        nz = (c >> 8) & 1;
                        c = (read_low!(data) as u16) << 1;
                        nz |= c;
                        // write_nz_zp
                        pc = pc.wrapping_add(1);
                        write_low!(data, nz as u8);
                    }

                    // ---- Increment/decrement ----
                    0x1A => {
                        nz = (a as u16).wrapping_add(1);
                        a = nz as u8;
                    } // INA
                    0xE8 => {
                        nz = (x as u16).wrapping_add(1);
                        x = nz as u8;
                    } // INX
                    0xC8 => {
                        nz = (y as u16).wrapping_add(1);
                        y = nz as u8;
                    } // INY
                    0x3A => {
                        nz = (a as u16).wrapping_add(0xFFFF);
                        a = nz as u8;
                    } // DEA
                    0xCA => {
                        nz = (x as u16).wrapping_add(0xFFFF);
                        x = nz as u8;
                    } // DEX
                    0x88 => {
                        nz = (y as u16).wrapping_add(0xFFFF);
                        y = nz as u8;
                    } // DEY

                    0xF6 | 0xE6 | 0xD6 | 0xC6 => {
                        // INC zp,x / INC zp / DEC zp,x / DEC zp
                        if opcode == 0xF6 || opcode == 0xD6 {
                            data = (data as u8).wrapping_add(x) as u16;
                        }
                        nz = if opcode & 0x20 != 0 { 1 } else { 0xFFFF };
                        // add_nz_zp
                        nz = nz.wrapping_add(read_low!(data) as u16);
                        // write_nz_zp
                        pc = pc.wrapping_add(1);
                        write_low!(data, nz as u8);
                    }
                    0xFE | 0xEE | 0xDE | 0xCE => {
                        // INC abs,x / INC abs / DEC abs,x / DEC abs
                        data = get_addr!();
                        if opcode == 0xFE || opcode == 0xDE {
                            data = data.wrapping_add(x as u16);
                        }
                        nz = if opcode & 0x20 != 0 { 1 } else { 0xFFFF };
                        // inc_common
                        flush_time!();
                        nz = nz.wrapping_add(read_full!(data) as u16);
                        pc = pc.wrapping_add(2);
                        write_full!(data, nz as u8);
                        cache_time!();
                    }

                    // ---- Transfer ----
                    0xA8 => {
                        y = a;
                        nz = a as u16;
                    } // TAY
                    0x98 => {
                        a = y;
                        nz = y as u16;
                    } // TYA
                    0xAA => {
                        x = a;
                        nz = a as u16;
                    } // TAX
                    0x8A => {
                        a = x;
                        nz = x as u16;
                    } // TXA
                    0x9A => {
                        sp = (x as u16 + 1) | 0x100; // TXS (no flag change)
                    }
                    0xBA => {
                        // TSX
                        x = (sp.wrapping_sub(1) & 0xFF) as u8;
                        nz = x as u16;
                    }
                    0x02 => std::mem::swap(&mut x, &mut y), // SXY
                    0x22 => std::mem::swap(&mut a, &mut x), // SAX
                    0x42 => std::mem::swap(&mut a, &mut y), // SAY
                    0x62 => a = 0,                          // CLA
                    0x82 => x = 0,                          // CLX
                    0xC2 => y = 0,                          // CLY

                    // ---- Stack ----
                    0x48 => {
                        push!(a);
                    } // PHA
                    0xDA => {
                        push!(x);
                    } // PHX
                    0x5A => {
                        push!(y);
                    } // PHY

                    0x40 => {
                        // RTI
                        let temp = read_low!(sp);
                        pc = read_low!(0x100 | sp.wrapping_sub(0xFF)) as u16;
                        pc |= (read_low!(0x100 | sp.wrapping_sub(0xFE)) as u16) << 8;
                        sp = sp.wrapping_sub(0xFD) | 0x100;
                        let old_status = status;
                        set_status!(temp);
                        self.cpu.r.status = status; // update externally-visible I flag
                        if (old_status ^ status) & ST_I != 0 {
                            let mut new_time = self.cpu.end_time;
                            if status & ST_I == 0 && new_time > self.cpu.irq_time {
                                new_time = self.cpu.irq_time;
                            }
                            let delta = self.cpu.state_base - new_time;
                            self.cpu.state_base = new_time;
                            s_time += delta;
                        }
                    }
                    0x68 => {
                        // PLA
                        a = pop!();
                        nz = a as u16;
                    }
                    0xFA => {
                        // PLX
                        x = pop!();
                        nz = x as u16;
                    }
                    0x7A => {
                        // PLY
                        y = pop!();
                        nz = y as u16;
                    }
                    0x28 => {
                        // PLP
                        let temp = pop!();
                        let changed = status ^ temp;
                        set_status!(temp);
                        if changed & ST_I != 0 {
                            if status & ST_I != 0 {
                                handle_sei!();
                            } else {
                                handle_cli!();
                            }
                        }
                    }
                    0x08 => {
                        // PHP
                        let temp = calc_status!();
                        push!(temp | ST_B);
                    }

                    // ---- Flags ----
                    0x38 => c = 0xFFFF,      // SEC
                    0x18 => c = 0,           // CLC
                    0xB8 => status &= !ST_V, // CLV
                    0xD8 => status &= !ST_D, // CLD
                    0xF8 => status |= ST_D,  // SED

                    0x58 => {
                        // CLI
                        if status & ST_I != 0 {
                            status &= !ST_I;
                            handle_cli!();
                        }
                    }
                    0x78 => {
                        // SEI
                        if status & ST_I == 0 {
                            status |= ST_I;
                            handle_sei!();
                        }
                    }

                    // ---- Special ----
                    0x53 => {
                        // TAM
                        let bits = data as u8;
                        pc = pc.wrapping_add(1);
                        for i in 0..8 {
                            if bits & (1 << i) != 0 {
                                self.set_mmr(i, a as i32);
                            }
                        }
                    }
                    0x43 => {
                        // TMA
                        pc = pc.wrapping_add(1);
                        let mut d = data;
                        let mut i = 0usize;
                        loop {
                            if d & 1 != 0 {
                                a = self.cpu.mmr[i];
                            }
                            i += 1;
                            d >>= 1;
                            if d == 0 {
                                break;
                            }
                        }
                    }
                    0x03 | 0x13 | 0x23 => {
                        // ST0 / ST1 / ST2
                        let mut addr = (opcode >> 4) as i32;
                        if addr != 0 {
                            addr += 1;
                        }
                        pc = pc.wrapping_add(1);
                        flush_time!();
                        self.cpu_write_vdp(addr, data as i32);
                        cache_time!();
                    }
                    0xEA => {} // NOP
                    0x54 => {
                        // CSL (not supported)
                        illegal_encountered = true;
                    }
                    0xD4 => {} // CSH
                    0xF4 => {
                        // SET (not handled)
                        illegal_encountered = true;
                    }

                    // ---- Block transfer ----
                    0xE3 | 0xF3 | 0xD3 | 0xC3 | 0x73 => {
                        // TIA / TAI / TIN / TDD / TII
                        let (in_alt, mut in_inc, out_alt, mut out_inc): (bool, i32, bool, i32) =
                            match opcode {
                                0xE3 => (false, 1, true, 0),    // TIA
                                0xF3 => (true, 0, false, 1),    // TAI
                                0xD3 => (false, 1, false, 0),   // TIN
                                0xC3 => (false, -1, false, -1), // TDD
                                _ => (false, 1, false, 1),      // TII
                            };
                        let mut src = (instr!(0) as u16) | ((instr!(1) as u16) << 8);
                        let mut dst = (instr!(2) as u16) | ((instr!(3) as u16) << 8);
                        let mut count = ((instr!(4) as u16) | ((instr!(5) as u16) << 8)) as i32;
                        if count == 0 {
                            count = 0x10000;
                        }
                        pc = pc.wrapping_add(6);
                        write_low!(0x100 | sp.wrapping_sub(1), y);
                        write_low!(0x100 | sp.wrapping_sub(2), a);
                        write_low!(0x100 | sp.wrapping_sub(3), x);
                        flush_time!();
                        loop {
                            let t = read_full!(src);
                            src = (src as i32 + in_inc) as u16;
                            self.cpu.state_time += 6;
                            if in_alt {
                                in_inc = -in_inc;
                            }
                            write_full!(dst, t);
                            dst = (dst as i32 + out_inc) as u16;
                            if out_alt {
                                out_inc = -out_inc;
                            }
                            count -= 1;
                            if count == 0 {
                                break;
                            }
                        }
                        cache_time!();
                    }

                    // ---- Illegal ----
                    _ => {
                        illegal_encountered = true;
                    }
                }
            }

            if go_out_of_time {
                // out_of_time
                pc = pc.wrapping_sub(1);
                flush_time!();
                let result = self.cpu_done();
                cache_time!();
                if result > 0 {
                    irq_vector = result;
                } else if s_time < 0 {
                    continue 'main;
                } else {
                    break 'main;
                }
            }

            if irq_vector != 0 {
                // interrupt
                s_time += 7;

                write_low!(0x100 | sp.wrapping_sub(1), (pc >> 8) as u8);
                write_low!(0x100 | sp.wrapping_sub(2), pc as u8);
                let vpage = self.cpu.code_map[(0xFFF0u16 >> PAGE_SHIFT) as usize];
                let voff = 0x1FF0 + irq_vector as usize;
                pc = self.page_byte(vpage, voff) as u16
                    | ((self.page_byte(vpage, voff + 1) as u16) << 8);

                sp = sp.wrapping_sub(3) | 0x100;
                let mut temp = calc_status!();
                if irq_vector == 6 {
                    temp |= ST_B;
                }
                write_low!(sp, temp);

                status &= !ST_D;
                status |= ST_I;
                self.cpu.r.status = status; // update externally-visible I flag

                let delta = self.cpu.state_base - self.cpu.end_time;
                self.cpu.state_base = self.cpu.end_time;
                s_time += delta;
            }
        }

        // store registers back (C++ epilogue)
        self.cpu.state_time = s_time;
        self.cpu.r.pc = pc;
        self.cpu.r.sp = (sp.wrapping_sub(1) & 0xFF) as u8;
        self.cpu.r.a = a;
        self.cpu.r.x = x;
        self.cpu.r.y = y;
        self.cpu.r.status = calc_status!();

        illegal_encountered
    }
}
