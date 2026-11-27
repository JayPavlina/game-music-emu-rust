//! NES 6502 CPU emulator.
//! Replaces C++ Nes_Cpu.

pub type NesTime = i32;

const PAGE_SHIFT: usize = 11;
#[allow(dead_code)]
const PAGE_SIZE: usize = 1 << PAGE_SHIFT; // 2048
const PAGE_COUNT: usize = 0x10000 >> PAGE_SHIFT; // 32

/// Per-opcode base cycle counts. Ported from C++ Nes_Cpu.cpp.
/// Branch opcodes (0x10,0x30,0x50,0x70,0x90,0xB0,0xD0,0xF0) show 3 here.
/// The branch macro subtracts 1 for untaken branches (net=2) and adds 1 for
/// taken (net=4) + page crossing (net=5), matching C++ behavior.
/// BRK (0x00) and bad_opcode (0xF2) are handled specially.
const CLOCK_TABLE: [u8; 256] = [
    //   0  1  2  3  4  5  6  7  8  9  A  B  C  D  E  F
    0, 6, 2, 8, 3, 3, 5, 5, 3, 2, 2, 2, 4, 4, 6, 6, // 0  (0x00=BRK, 0x02=JAM)
    3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // 1  (0x12=2 cycles)
    6, 6, 2, 8, 3, 3, 5, 5, 4, 2, 2, 2, 4, 4, 6, 6, // 2  (0x22=2 cycles)
    3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // 3  (0x32=2 cycles)
    6, 6, 2, 8, 3, 3, 5, 5, 3, 2, 2, 2, 3, 4, 6, 6, // 4  (0x42=2 cycles)
    3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // 5  (0x52=2 cycles)
    6, 6, 2, 8, 3, 3, 5, 5, 4, 2, 2, 2, 5, 4, 6, 6, // 6  (0x62=2 cycles)
    3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // 7  (0x72=2 cycles)
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, // 8
    3, 6, 2, 6, 4, 4, 4, 4, 2, 5, 2, 5, 5, 5, 5, 5, // 9  (0x90=BCC)
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, // A
    3, 5, 2, 5, 4, 4, 4, 4, 2, 4, 2, 4, 4, 4, 4, 4, // B  (0xB0=BCS)
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, // C
    3, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // D  (0xD2=2 cycles)
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, // E
    3, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7, // F  (0xF0=BEQ, 0xF2=bad_opcode=0)
];

/// CPU registers.
#[derive(Clone, Copy, Debug)]
pub struct Registers {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub status: u8,
    pub sp: u8,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            pc: 0,
            a: 0,
            x: 0,
            y: 0,
            status: 0x24, // I flag + reserved
            sp: 0xFD,
        }
    }
}

/// Status flag constants
const FLAG_C: u8 = 0x01;
const FLAG_Z: u8 = 0x02;
const FLAG_I: u8 = 0x04;
const FLAG_D: u8 = 0x08;
const FLAG_B: u8 = 0x10;
const FLAG_V: u8 = 0x40;
const FLAG_N: u8 = 0x80;

/// CPU state for timing.
#[derive(Clone, Default)]
pub struct CpuState {
    pub base: NesTime,
    pub time: NesTime,
}

/// NES CPU.
pub struct NesCpu {
    pub r: Registers,
    pub state: CpuState,
    pub low_mem: [u8; 0x800],
    pub code_map: [usize; PAGE_COUNT + 1], // offsets into rom data
    pub irq_time: NesTime,
    pub end_time: NesTime,
    pub error_count: u64,
    /// ROM data reference (index into external storage)
    pub rom_data: Vec<u8>,
    /// SRAM (8KB)
    pub sram: [u8; 0x2000],
    /// Bad opcode address (used as sentinel for NSF play routine)
    pub badop_addr: u16,
    /// Whether CPU hit bad opcode
    pub hit_badop: bool,
}

impl Default for NesCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl NesCpu {
    pub const BAD_OPCODE: u8 = 0xF2;

    pub fn new() -> Self {
        Self {
            r: Registers::default(),
            state: CpuState::default(),
            low_mem: [0; 0x800],
            code_map: [usize::MAX; PAGE_COUNT + 1], // usize::MAX = unmapped (returns bad_opcode)
            irq_time: 0,
            end_time: 0,
            error_count: 0,
            rom_data: Vec::new(),
            sram: [0; 0x2000],
            badop_addr: 0x5FF8,
            hit_badop: false,
        }
    }

    pub fn reset(&mut self) {
        self.r = Registers::default();
        self.state = CpuState::default();
        self.irq_time = 0;
        self.hit_badop = false;
    }

    pub fn time(&self) -> NesTime {
        self.state.base + self.state.time
    }

    pub fn set_time(&mut self, t: NesTime) {
        self.state.time = t - self.state.base;
    }

    /// Map a region of address space to data.
    /// data_offset is the index into rom_data such that rom_data[data_offset + addr] gives
    /// the correct byte for CPU address `addr`.
    pub fn map_code(&mut self, addr: u16, size: u16, rom_data_index: usize) {
        let begin = addr as usize >> PAGE_SHIFT;
        let end = (addr as usize + size as usize) >> PAGE_SHIFT;
        // Store the pre-computed offset: rom_data_index - addr
        // So that code_map[page] + addr = rom_data_index
        let offset = rom_data_index.wrapping_sub(addr as usize);
        self.code_map[begin..end.min(PAGE_COUNT)].fill(offset);
    }

    /// Read a byte from the CPU address space.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.low_mem[addr as usize & 0x7FF],
            0x8000..=0xFFFF => {
                let page = addr as usize >> PAGE_SHIFT;
                let idx = self.code_map[page].wrapping_add(addr as usize);
                if idx < self.rom_data.len() {
                    self.rom_data[idx]
                } else {
                    (addr >> 8) as u8 // open bus
                }
            }
            0x6000..=0x7FFF => self.sram[(addr - 0x6000) as usize],
            _ => (addr >> 8) as u8, // open bus
        }
    }

    /// Write a byte to the CPU address space.
    pub fn write(&mut self, addr: u16, data: u8) {
        let addr = addr as usize;
        match addr {
            0x6000..=0x7FFF => self.sram[addr - 0x6000] = data,
            0x0000..=0x1FFF => self.low_mem[addr & 0x7FF] = data,
            _ => {} // I/O handled externally
        }
    }

    /// Push a byte onto the stack (public for NSF play routine setup).
    pub fn push(&mut self, data: u8) {
        let sp = self.r.sp as u16 | 0x0100;
        self.write(sp, data);
        self.r.sp = self.r.sp.wrapping_sub(1);
    }

    /// Pull a byte from the stack.
    #[allow(dead_code)]
    fn pull(&mut self) -> u8 {
        self.r.sp = self.r.sp.wrapping_add(1);
        let sp = self.r.sp as u16 | 0x0100;
        self.read(sp)
    }

    /// Reset the CPU for starting a track.
    pub fn start(&mut self, init_addr: u16, track: u8, pal: bool) {
        self.r = Registers::default();
        self.state.time = 0;
        self.irq_time = 0;
        self.hit_badop = false;

        // Mark all pages as unmapped (usize::MAX = returns bad_opcode)
        self.code_map[..PAGE_COUNT].fill(usize::MAX);

        // Write return address directly to top of stack page (matching C++ start_track_).
        // C++ writes to low_mem[0x1FF]/low_mem[0x1FE] and sets sp=0xFD.
        // The init routine's RTS will read from 0x1FE/0x1FF and leave sp=0xFF.
        self.low_mem[0x1FF] = ((self.badop_addr - 1) >> 8) as u8;
        self.low_mem[0x1FE] = ((self.badop_addr - 1) & 0xFF) as u8;
        self.r.sp = 0xFD;

        // Set up init call
        self.r.pc = init_addr;
        self.r.a = track;
        self.r.x = if pal { 1 } else { 0 };
    }

    /// Run CPU until end_time or bad opcode.
    /// Returns true if hit bad opcode.
    /// end_time must be set before calling this (via self.end_time = ...).
    #[allow(unused_assignments)]
    pub fn run<R, W>(&mut self, cpu_read: &R, cpu_write: &W) -> bool
    where
        R: Fn(u16, NesTime) -> u8,
        W: Fn(u16, u8, NesTime),
    {
        self.hit_badop = false;

        let mut pc = self.r.pc;
        let mut a = self.r.a;
        let mut x = self.r.x;
        let mut y = self.r.y;
        let mut sp = self.r.sp as u16 | 0x0100;
        let mut status = self.r.status;
        // Use nz and c encoding for flags
        let mut nz: u16 = 0;
        let mut c: u16 = 0;

        // Initialize flag state from status register
        if status & FLAG_C != 0 {
            c = 0x100;
        }
        if status & FLAG_Z == 0 {
            nz = 1;
        }
        if status & FLAG_N != 0 {
            nz |= 0x8080;
        }

        let mut time = self.state.time;

        macro_rules! read {
            ($addr:expr) => {
                cpu_read($addr, self.state.base.wrapping_add(time))
            };
        }

        macro_rules! write {
            ($addr:expr, $data:expr) => {
                cpu_write($addr, $data, self.state.base.wrapping_add(time));
            };
        }

        macro_rules! set_nz {
            ($val:expr) => {
                nz = $val as u16;
            };
        }

        macro_rules! branch {
            ($cond:expr) => {
                if $cond {
                    let offset = read!(pc) as i8;
                    pc = pc.wrapping_add(1);
                    let new_pc = pc.wrapping_add(offset as u16);
                    // Clock table gives 3 for branches. Untaken costs 2 (time-=1 below).
                    // Taken costs 3 (no extra). Page crossing adds 1 more (total 4).
                    if (pc ^ new_pc) & 0xFF00 != 0 {
                        time += 1;
                    }
                    pc = new_pc;
                } else {
                    pc = pc.wrapping_add(1);
                    time -= 1; // Table gives 3, untaken branch costs 2
                }
            };
        }

        macro_rules! push {
            ($val:expr) => {
                write!(sp, $val);
                sp = sp.wrapping_sub(1) | 0x0100;
            };
        }

        macro_rules! pull {
            () => {{
                sp = sp.wrapping_add(1) | 0x0100;
                read!(sp)
            }};
        }

        // Load effective address for zero-page modes
        macro_rules! zp_addr {
            () => {{
                let addr = read!(pc) as u16;
                pc = pc.wrapping_add(1);
                addr
            }};
        }

        macro_rules! zp_x_addr {
            () => {{
                let addr = (read!(pc).wrapping_add(x)) as u16;
                pc = pc.wrapping_add(1);
                addr & 0xFF
            }};
        }

        macro_rules! abs_addr {
            () => {{
                let lo = read!(pc) as u16;
                pc = pc.wrapping_add(1);
                let hi = read!(pc) as u16;
                pc = pc.wrapping_add(1);
                lo | (hi << 8)
            }};
        }

        // Main execution loop
        loop {
            if time >= self.end_time {
                break;
            }

            let opcode = read!(pc);
            pc = pc.wrapping_add(1);

            // Differential-trace hook (NSF_CPU_TRACE_RS=<path>), mirrors C++
            #[cfg(feature = "trace")]
            {
                // Compose P the way C++ CALC_STATUS does
                let mut p = status & (FLAG_V | FLAG_D | FLAG_I);
                p |= (((nz >> 8) | nz) as u8) & 0x80;
                if c & 0x100 != 0 {
                    p |= FLAG_C;
                }
                if nz & 0xFF == 0 {
                    p |= 0x02;
                }
                cpu_trace(
                    pc.wrapping_sub(1),
                    opcode,
                    a,
                    x,
                    y,
                    p,
                    (sp & 0xFF) as u8,
                    time,
                );
            }

            // Advance time using per-opcode cycle table
            time += CLOCK_TABLE[opcode as usize] as i32;

            match opcode {
                // LDA immediate
                0xA9 => {
                    a = read!(pc);
                    pc = pc.wrapping_add(1);
                    set_nz!(a);
                }
                // LDA zero page
                0xA5 => {
                    let addr = zp_addr!();
                    a = read!(addr);
                    set_nz!(a);
                }
                // LDA zero page,X
                0xB5 => {
                    let addr = zp_x_addr!();
                    a = read!(addr);
                    set_nz!(a);
                }
                // LDA absolute
                0xAD => {
                    let addr = abs_addr!();
                    a = read!(addr);
                    set_nz!(a);
                }
                // LDA absolute,X
                0xBD => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    a = read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // LDA absolute,Y
                0xB9 => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    a = read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // LDA (indirect,X)
                0xA1 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    a = read!(lo | (hi << 8));
                    set_nz!(a);
                }
                // LDA (indirect),Y
                0xB1 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    a = read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // LDX immediate
                0xA2 => {
                    x = read!(pc);
                    pc = pc.wrapping_add(1);
                    set_nz!(x);
                }
                // LDX zero page
                0xA6 => {
                    let addr = zp_addr!();
                    x = read!(addr);
                    set_nz!(x);
                }
                // LDX absolute
                0xAE => {
                    let addr = abs_addr!();
                    x = read!(addr);
                    set_nz!(x);
                }

                // LDY immediate
                0xA0 => {
                    y = read!(pc);
                    pc = pc.wrapping_add(1);
                    set_nz!(y);
                }
                // LDY zero page
                0xA4 => {
                    let addr = zp_addr!();
                    y = read!(addr);
                    set_nz!(y);
                }
                // LDY absolute
                0xAC => {
                    let addr = abs_addr!();
                    y = read!(addr);
                    set_nz!(y);
                }

                // STA zero page
                0x85 => {
                    let addr = zp_addr!();
                    write!(addr, a);
                }
                // STA absolute
                0x8D => {
                    let addr = abs_addr!();
                    write!(addr, a);
                }
                // STA zero page,X
                0x95 => {
                    let addr = zp_x_addr!();
                    write!(addr, a);
                }
                // STA absolute,X
                0x9D => {
                    let addr = abs_addr!().wrapping_add(x as u16);
                    write!(addr, a);
                }
                // STA absolute,Y
                0x99 => {
                    let addr = abs_addr!().wrapping_add(y as u16);
                    write!(addr, a);
                }
                // STA (indirect,X)
                0x81 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    write!(lo | (hi << 8), a);
                }
                // STA (indirect),Y
                0x91 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    write!((lo | (hi << 8)).wrapping_add(y as u16), a);
                }

                // STX zero page
                0x86 => {
                    let addr = zp_addr!();
                    write!(addr, x);
                }
                // STX absolute
                0x8E => {
                    let addr = abs_addr!();
                    write!(addr, x);
                }

                // STY zero page
                0x84 => {
                    let addr = zp_addr!();
                    write!(addr, y);
                }
                // STY absolute
                0x8C => {
                    let addr = abs_addr!();
                    write!(addr, y);
                }

                // TAX
                0xAA => {
                    x = a;
                    set_nz!(x);
                }
                // TAY
                0xA8 => {
                    y = a;
                    set_nz!(y);
                }
                // TXA
                0x8A => {
                    a = x;
                    set_nz!(a);
                }
                // TYA
                0x98 => {
                    a = y;
                    set_nz!(a);
                }
                // TSX
                0xBA => {
                    x = (sp & 0xFF) as u8;
                    set_nz!(x);
                }
                // TXS
                0x9A => {
                    sp = x as u16 | 0x0100;
                }

                // ADC immediate
                0x69 => {
                    let val = read!(pc);
                    pc = pc.wrapping_add(1);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    let result = sum as u8;
                    // Overflow detection
                    if ((a ^ result) & (val ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }

                // SBC immediate
                0xE9 => {
                    let val = read!(pc);
                    pc = pc.wrapping_add(1);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry as i16);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }

                // CMP immediate
                0xC9 => {
                    let val = read!(pc);
                    pc = pc.wrapping_add(1);
                    let diff = a as i16 - val as i16;
                    c = if diff >= 0 { 0x100 } else { 0 };
                    nz = diff as u8 as u16;
                    if a >= val {
                        nz |= 0x100;
                    } // not really needed for nz
                    // Actually: set nz so that N flag works
                    nz = diff as u16 & 0xFF;
                    if diff >= 0 {
                        c = 0x100;
                    } else {
                        c = 0;
                    }
                }

                // CPX immediate
                0xE0 => {
                    let val = read!(pc);
                    pc = pc.wrapping_add(1);
                    let diff = x.wrapping_sub(val);
                    c = if x >= val { 0x100 } else { 0 };
                    set_nz!(diff);
                }

                // CPY immediate
                0xC0 => {
                    let val = read!(pc);
                    pc = pc.wrapping_add(1);
                    let diff = y.wrapping_sub(val);
                    c = if y >= val { 0x100 } else { 0 };
                    set_nz!(diff);
                }

                // AND immediate
                0x29 => {
                    a &= read!(pc);
                    pc = pc.wrapping_add(1);
                    set_nz!(a);
                }

                // ORA immediate
                0x09 => {
                    a |= read!(pc);
                    pc = pc.wrapping_add(1);
                    set_nz!(a);
                }

                // EOR immediate
                0x49 => {
                    a ^= read!(pc);
                    pc = pc.wrapping_add(1);
                    set_nz!(a);
                }

                // ASL accumulator
                0x0A => {
                    c = (a as u16) << 1;
                    a = (c & 0xFF) as u8;
                    set_nz!(a);
                }

                // LSR accumulator
                0x4A => {
                    c = if a & 1 != 0 { 0x100 } else { 0 };
                    a >>= 1;
                    set_nz!(a);
                }

                // ROL accumulator
                0x2A => {
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (a as u16) << 1 | old_c;
                    a = (c & 0xFF) as u8;
                    set_nz!(a);
                }

                // ROR accumulator
                0x6A => {
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if a & 1 != 0 { 0x100 } else { 0 };
                    a = (a >> 1) | old_c;
                    set_nz!(a);
                }

                // INC zero page
                0xE6 => {
                    let addr = zp_addr!();
                    let val = read!(addr).wrapping_add(1);
                    write!(addr, val);
                    set_nz!(val);
                }

                // DEC zero page
                0xC6 => {
                    let addr = zp_addr!();
                    let val = read!(addr).wrapping_sub(1);
                    write!(addr, val);
                    set_nz!(val);
                }

                // INX
                0xE8 => {
                    x = x.wrapping_add(1);
                    set_nz!(x);
                }
                // DEX
                0xCA => {
                    x = x.wrapping_sub(1);
                    set_nz!(x);
                }
                // INY
                0xC8 => {
                    y = y.wrapping_add(1);
                    set_nz!(y);
                }
                // DEY
                0x88 => {
                    y = y.wrapping_sub(1);
                    set_nz!(y);
                }

                // JMP absolute
                0x4C => {
                    pc = abs_addr!();
                }
                // JMP indirect
                0x6C => {
                    let addr = abs_addr!();
                    let lo = read!(addr) as u16;
                    // 6502 bug: wraps within page
                    let hi_addr = (addr & 0xFF00) | ((addr + 1) & 0x00FF);
                    let hi = read!(hi_addr) as u16;
                    pc = lo | (hi << 8);
                }

                // JSR absolute
                0x20 => {
                    let addr = abs_addr!();
                    let ret = pc.wrapping_sub(1);
                    push!((ret >> 8) as u8);
                    push!((ret & 0xFF) as u8);
                    pc = addr;
                }

                // RTS
                0x60 => {
                    let lo = pull!() as u16;
                    let hi = pull!() as u16;
                    pc = (hi << 8 | lo).wrapping_add(1);
                }

                // RTI
                0x40 => {
                    status = pull!();
                    let lo = pull!() as u16;
                    let hi = pull!() as u16;
                    pc = hi << 8 | lo;
                    // Rebuild nz and c from status
                    nz = 0;
                    c = 0;
                    if status & FLAG_C != 0 {
                        c = 0x100;
                    }
                    if status & FLAG_Z == 0 {
                        nz = 1;
                    }
                    if status & FLAG_N != 0 {
                        nz |= 0x8080;
                    }
                }

                // PHA
                0x48 => {
                    push!(a);
                }
                // PHP (C++: CALC_STATUS then push with B|R set)
                0x08 => {
                    let mut flags = status & (FLAG_V | FLAG_D | FLAG_I);
                    flags |= (((nz >> 8) | nz) as u8) & FLAG_N;
                    if c & 0x100 != 0 {
                        flags |= FLAG_C;
                    }
                    if nz & 0xFF == 0 {
                        flags |= FLAG_Z;
                    }
                    push!(flags | FLAG_B | 0x20);
                }
                // PLA
                0x68 => {
                    a = pull!();
                    set_nz!(a);
                }
                // PLP
                0x28 => {
                    status = pull!() & !(FLAG_B | 0x20);
                    nz = 0;
                    c = 0;
                    if status & FLAG_C != 0 {
                        c = 0x100;
                    }
                    if status & FLAG_Z == 0 {
                        nz = 1;
                    }
                    if status & FLAG_N != 0 {
                        nz |= 0x8080;
                    }
                }

                // BCC
                0x90 => {
                    branch!(c & 0x100 == 0);
                }
                // BCS
                0xB0 => {
                    branch!(c & 0x100 != 0);
                }
                // BEQ
                0xF0 => {
                    branch!(nz & 0xFF == 0);
                }
                // BNE
                0xD0 => {
                    branch!(nz & 0xFF != 0);
                }
                // BMI
                0x30 => {
                    branch!(nz & 0x8080 != 0);
                }
                // BPL
                0x10 => {
                    branch!(nz & 0x8080 == 0);
                }
                // BVC
                0x50 => {
                    branch!(status & FLAG_V == 0);
                }
                // BVS
                0x70 => {
                    branch!(status & FLAG_V != 0);
                }

                // CLC
                0x18 => {
                    c = 0;
                    status &= !FLAG_C;
                }
                // SEC
                0x38 => {
                    c = 0x100;
                    status |= FLAG_C;
                }
                // CLI
                0x58 => {
                    status &= !FLAG_I;
                }
                // SEI
                0x78 => {
                    status |= FLAG_I;
                }
                // CLV
                0xB8 => {
                    status &= !FLAG_V;
                }
                // CLD
                0xD8 => {
                    status &= !FLAG_D;
                }
                // SED
                0xF8 => {
                    status |= FLAG_D;
                }

                // NOP
                0xEA => {}

                // BRK (C++ handle_brk: CALC_STATUS, push with R|B set)
                0x00 => {
                    pc = pc.wrapping_add(1);
                    push!((pc >> 8) as u8);
                    push!((pc & 0xFF) as u8);
                    let mut flags = status & (FLAG_V | FLAG_D | FLAG_I);
                    flags |= (((nz >> 8) | nz) as u8) & FLAG_N;
                    if c & 0x100 != 0 {
                        flags |= FLAG_C;
                    }
                    if nz & 0xFF == 0 {
                        flags |= FLAG_Z;
                    }
                    push!(flags | FLAG_B | 0x20);
                    status |= FLAG_I;
                    let lo = read!(0xFFFE) as u16;
                    let hi = read!(0xFFFF) as u16;
                    pc = lo | (hi << 8);
                    time += 7; // BRK takes 7 cycles total (table has 0)
                }

                // BIT zero page (C++: V from operand; nz holds the operand,
                // shifted high when (A & val) == 0 so Z reads as set while N
                // still comes from the operand's bit 7)
                0x24 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    status = (status & !FLAG_V) | (val & FLAG_V);
                    nz = val as u16;
                    if a & val == 0 {
                        nz <<= 8; // result must be zero, even if N bit is set
                    }
                }
                // BIT absolute
                0x2C => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    status = (status & !FLAG_V) | (val & FLAG_V);
                    nz = val as u16;
                    if a & val == 0 {
                        nz <<= 8; // result must be zero, even if N bit is set
                    }
                }

                // ===== ALU operations with all addressing modes =====

                // AND zero page
                0x25 => {
                    let addr = zp_addr!();
                    a &= read!(addr);
                    set_nz!(a);
                }
                // AND zero page,X
                0x35 => {
                    let addr = zp_x_addr!();
                    a &= read!(addr);
                    set_nz!(a);
                }
                // AND absolute
                0x2D => {
                    let addr = abs_addr!();
                    a &= read!(addr);
                    set_nz!(a);
                }
                // AND absolute,X
                0x3D => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    a &= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // AND absolute,Y
                0x39 => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    a &= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // AND (indirect,X)
                0x21 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    a &= read!(lo | (hi << 8));
                    set_nz!(a);
                }
                // AND (indirect),Y
                0x31 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    a &= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // ORA zero page
                0x05 => {
                    let addr = zp_addr!();
                    a |= read!(addr);
                    set_nz!(a);
                }
                // ORA zero page,X
                0x15 => {
                    let addr = zp_x_addr!();
                    a |= read!(addr);
                    set_nz!(a);
                }
                // ORA absolute
                0x0D => {
                    let addr = abs_addr!();
                    a |= read!(addr);
                    set_nz!(a);
                }
                // ORA absolute,X
                0x1D => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    a |= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // ORA absolute,Y
                0x19 => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    a |= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // ORA (indirect,X)
                0x01 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    a |= read!(lo | (hi << 8));
                    set_nz!(a);
                }
                // ORA (indirect),Y
                0x11 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    a |= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // EOR zero page
                0x45 => {
                    let addr = zp_addr!();
                    a ^= read!(addr);
                    set_nz!(a);
                }
                // EOR zero page,X
                0x55 => {
                    let addr = zp_x_addr!();
                    a ^= read!(addr);
                    set_nz!(a);
                }
                // EOR absolute
                0x4D => {
                    let addr = abs_addr!();
                    a ^= read!(addr);
                    set_nz!(a);
                }
                // EOR absolute,X
                0x5D => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    a ^= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // EOR absolute,Y
                0x59 => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    a ^= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // EOR (indirect,X)
                0x41 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    a ^= read!(lo | (hi << 8));
                    set_nz!(a);
                }
                // EOR (indirect),Y
                0x51 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    a ^= read!(eff);
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // ADC zero page
                0x65 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (val ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                }
                // ADC zero page,X
                0x75 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (val ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                }
                // ADC absolute
                0x6D => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (val ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                }
                // ADC absolute,X
                0x7D => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    let val = read!(eff);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (val ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // ADC absolute,Y
                0x79 => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    let val = read!(eff);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (val ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // ADC (indirect,X)
                0x61 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let val = read!(lo | (hi << 8));
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (val ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                }
                // ADC (indirect),Y
                0x71 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    let val = read!(eff);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + val as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (val ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // SBC zero page
                0xE5 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }
                // SBC zero page,X
                0xF5 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }
                // SBC absolute
                0xED => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }
                // SBC absolute,X
                0xFD => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    let val = read!(eff);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // SBC absolute,Y
                0xF9 => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    let val = read!(eff);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // SBC (indirect,X)
                0xE1 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let val = read!(lo | (hi << 8));
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }
                // SBC (indirect),Y
                0xF1 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    let val = read!(eff);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // CMP zero page
                0xC5 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                }
                // CMP zero page,X
                0xD5 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                }
                // CMP absolute
                0xCD => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                }
                // CMP absolute,X
                0xDD => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    let val = read!(eff);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // CMP absolute,Y
                0xD9 => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    let val = read!(eff);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // CMP (indirect,X)
                0xC1 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let val = read!(lo | (hi << 8));
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                }
                // CMP (indirect),Y
                0xD1 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    let val = read!(eff);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // CPX zero page
                0xE4 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    c = if x >= val { 0x100 } else { 0 };
                    set_nz!(x.wrapping_sub(val));
                }
                // CPX absolute
                0xEC => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    c = if x >= val { 0x100 } else { 0 };
                    set_nz!(x.wrapping_sub(val));
                }

                // CPY zero page
                0xC4 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    c = if y >= val { 0x100 } else { 0 };
                    set_nz!(y.wrapping_sub(val));
                }
                // CPY absolute
                0xCC => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    c = if y >= val { 0x100 } else { 0 };
                    set_nz!(y.wrapping_sub(val));
                }

                // ===== LDX additional modes =====
                // LDX zero page,Y
                0xB6 => {
                    let addr = (read!(pc).wrapping_add(y)) as u16 & 0xFF;
                    pc = pc.wrapping_add(1);
                    x = read!(addr);
                    set_nz!(x);
                }
                // LDX absolute,Y
                0xBE => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    x = read!(eff);
                    set_nz!(x);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // ===== LDY additional modes =====
                // LDY zero page,X
                0xB4 => {
                    let addr = zp_x_addr!();
                    y = read!(addr);
                    set_nz!(y);
                }
                // LDY absolute,X
                0xBC => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(x as u16);
                    y = read!(eff);
                    set_nz!(y);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // ===== STX additional modes =====
                // STX zero page,Y
                0x96 => {
                    let addr = (read!(pc).wrapping_add(y)) as u16 & 0xFF;
                    pc = pc.wrapping_add(1);
                    write!(addr, x);
                }

                // ===== STY additional modes =====
                // STY zero page,X
                0x94 => {
                    let addr = zp_x_addr!();
                    write!(addr, y);
                }

                // ===== INC additional modes =====
                // INC zero page,X
                0xF6 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr).wrapping_add(1);
                    write!(addr, val);
                    set_nz!(val);
                }
                // INC absolute
                0xEE => {
                    let addr = abs_addr!();
                    let val = read!(addr).wrapping_add(1);
                    write!(addr, val);
                    set_nz!(val);
                }
                // INC absolute,X
                0xFE => {
                    let addr = abs_addr!().wrapping_add(x as u16);
                    let val = read!(addr).wrapping_add(1);
                    write!(addr, val);
                    set_nz!(val);
                }

                // ===== DEC additional modes =====
                // DEC zero page,X
                0xD6 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr).wrapping_sub(1);
                    write!(addr, val);
                    set_nz!(val);
                }
                // DEC absolute
                0xCE => {
                    let addr = abs_addr!();
                    let val = read!(addr).wrapping_sub(1);
                    write!(addr, val);
                    set_nz!(val);
                }
                // DEC absolute,X
                0xDE => {
                    let addr = abs_addr!().wrapping_add(x as u16);
                    let val = read!(addr).wrapping_sub(1);
                    write!(addr, val);
                    set_nz!(val);
                }

                // ===== ASL additional modes =====
                // ASL zero page
                0x06 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    c = (val as u16) << 1;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }
                // ASL zero page,X
                0x16 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr);
                    c = (val as u16) << 1;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }
                // ASL absolute
                0x0E => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    c = (val as u16) << 1;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }
                // ASL absolute,X
                0x1E => {
                    let addr = abs_addr!().wrapping_add(x as u16);
                    let val = read!(addr);
                    c = (val as u16) << 1;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }

                // ===== LSR additional modes =====
                // LSR zero page
                0x46 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    write!(addr, val >> 1);
                    set_nz!(val >> 1);
                }
                // LSR zero page,X
                0x56 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr);
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    write!(addr, val >> 1);
                    set_nz!(val >> 1);
                }
                // LSR absolute
                0x4E => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    write!(addr, val >> 1);
                    set_nz!(val >> 1);
                }
                // LSR absolute,X
                0x5E => {
                    let addr = abs_addr!().wrapping_add(x as u16);
                    let val = read!(addr);
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    write!(addr, val >> 1);
                    set_nz!(val >> 1);
                }

                // ===== ROL additional modes =====
                // ROL zero page
                0x26 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (val as u16) << 1 | old_c;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }
                // ROL zero page,X
                0x36 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (val as u16) << 1 | old_c;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }
                // ROL absolute
                0x2E => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (val as u16) << 1 | old_c;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }
                // ROL absolute,X
                0x3E => {
                    let addr = abs_addr!().wrapping_add(x as u16);
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (val as u16) << 1 | old_c;
                    write!(addr, (c & 0xFF) as u8);
                    set_nz!((c & 0xFF) as u8);
                }

                // ===== ROR additional modes =====
                // ROR zero page
                0x66 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = (val >> 1) | old_c;
                    write!(addr, result);
                    set_nz!(result);
                }
                // ROR zero page,X
                0x76 => {
                    let addr = zp_x_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = (val >> 1) | old_c;
                    write!(addr, result);
                    set_nz!(result);
                }
                // ROR absolute
                0x6E => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = (val >> 1) | old_c;
                    write!(addr, result);
                    set_nz!(result);
                }
                // ROR absolute,X
                0x7E => {
                    let addr = abs_addr!().wrapping_add(x as u16);
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = (val >> 1) | old_c;
                    write!(addr, result);
                    set_nz!(result);
                }

                // ===== Unofficial but commonly used opcodes =====
                // LAX (indirect,X) - loads A and X
                0xA3 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    a = read!(lo | (hi << 8));
                    x = a;
                    set_nz!(a);
                }
                // LAX zero page
                0xA7 => {
                    let addr = zp_addr!();
                    a = read!(addr);
                    x = a;
                    set_nz!(a);
                }
                // LAX absolute
                0xAF => {
                    let addr = abs_addr!();
                    a = read!(addr);
                    x = a;
                    set_nz!(a);
                }
                // LAX (indirect),Y
                0xB3 => {
                    let zp = zp_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let eff = addr.wrapping_add(y as u16);
                    a = read!(eff);
                    x = a;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }
                // LAX zero page,Y
                0xB7 => {
                    let addr = (read!(pc).wrapping_add(y)) as u16 & 0xFF;
                    pc = pc.wrapping_add(1);
                    a = read!(addr);
                    x = a;
                    set_nz!(a);
                }
                // LAX absolute,Y
                0xBF => {
                    let addr = abs_addr!();
                    let eff = addr.wrapping_add(y as u16);
                    a = read!(eff);
                    x = a;
                    set_nz!(a);
                    if (addr ^ eff) & 0xFF00 != 0 {
                        time += 1;
                    }
                }

                // SAX (indirect,X)
                0x83 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    write!(lo | (hi << 8), a & x);
                }
                // SAX zero page
                0x87 => {
                    let addr = zp_addr!();
                    write!(addr, a & x);
                }
                // SAX absolute
                0x8F => {
                    let addr = abs_addr!();
                    write!(addr, a & x);
                }
                // SAX zero page,Y
                0x97 => {
                    let addr = (read!(pc).wrapping_add(y)) as u16 & 0xFF;
                    pc = pc.wrapping_add(1);
                    write!(addr, a & x);
                }

                // DCP (indirect,X)
                0xC3 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let val = read!(addr).wrapping_sub(1);
                    write!(addr, val);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                }
                // DCP zero page
                0xC7 => {
                    let addr = zp_addr!();
                    let val = read!(addr).wrapping_sub(1);
                    write!(addr, val);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                }
                // DCP absolute
                0xCF => {
                    let addr = abs_addr!();
                    let val = read!(addr).wrapping_sub(1);
                    write!(addr, val);
                    c = if a >= val { 0x100 } else { 0 };
                    set_nz!(a.wrapping_sub(val));
                }

                // ISC (indirect,X)
                0xE3 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let val = read!(addr).wrapping_add(1);
                    write!(addr, val);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }
                // ISC zero page
                0xE7 => {
                    let addr = zp_addr!();
                    let val = read!(addr).wrapping_add(1);
                    write!(addr, val);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }
                // ISC absolute
                0xEF => {
                    let addr = abs_addr!();
                    let val = read!(addr).wrapping_add(1);
                    write!(addr, val);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let diff = a as i16 - val as i16 - (1 - carry);
                    c = if diff >= 0 { 0x100 } else { 0 };
                    let result = diff as u8;
                    if ((a ^ val) & (a ^ result) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = result;
                    set_nz!(a);
                }

                // SLO (indirect,X)
                0x03 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let val = read!(addr);
                    c = (val as u16) << 1;
                    let result = (c & 0xFF) as u8;
                    write!(addr, result);
                    a |= result;
                    set_nz!(a);
                }
                // SLO zero page
                0x07 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    c = (val as u16) << 1;
                    let result = (c & 0xFF) as u8;
                    write!(addr, result);
                    a |= result;
                    set_nz!(a);
                }
                // SLO absolute
                0x0F => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    c = (val as u16) << 1;
                    let result = (c & 0xFF) as u8;
                    write!(addr, result);
                    a |= result;
                    set_nz!(a);
                }

                // RLA (indirect,X)
                0x23 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (val as u16) << 1 | old_c;
                    let result = (c & 0xFF) as u8;
                    write!(addr, result);
                    a &= result;
                    set_nz!(a);
                }
                // RLA zero page
                0x27 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (val as u16) << 1 | old_c;
                    let result = (c & 0xFF) as u8;
                    write!(addr, result);
                    a &= result;
                    set_nz!(a);
                }
                // RLA absolute
                0x2F => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 1 } else { 0 };
                    c = (val as u16) << 1 | old_c;
                    let result = (c & 0xFF) as u8;
                    write!(addr, result);
                    a &= result;
                    set_nz!(a);
                }

                // SRE (indirect,X)
                0x43 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let val = read!(addr);
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = val >> 1;
                    write!(addr, result);
                    a ^= result;
                    set_nz!(a);
                }
                // SRE zero page
                0x47 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = val >> 1;
                    write!(addr, result);
                    a ^= result;
                    set_nz!(a);
                }
                // SRE absolute
                0x4F => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = val >> 1;
                    write!(addr, result);
                    a ^= result;
                    set_nz!(a);
                }

                // RRA (indirect,X)
                0x63 => {
                    let zp = zp_x_addr!();
                    let lo = read!(zp) as u16;
                    let hi = read!(zp.wrapping_add(1) & 0xFF) as u16;
                    let addr = lo | (hi << 8);
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = (val >> 1) | old_c;
                    write!(addr, result);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + result as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (result ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                }
                // RRA zero page
                0x67 => {
                    let addr = zp_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = (val >> 1) | old_c;
                    write!(addr, result);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + result as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (result ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                }
                // RRA absolute
                0x6F => {
                    let addr = abs_addr!();
                    let val = read!(addr);
                    let old_c = if c & 0x100 != 0 { 0x80 } else { 0 };
                    c = if val & 1 != 0 { 0x100 } else { 0 };
                    let result = (val >> 1) | old_c;
                    write!(addr, result);
                    let carry = if c & 0x100 != 0 { 1 } else { 0 };
                    let sum = a as u16 + result as u16 + carry;
                    c = sum;
                    if ((a ^ sum as u8) & (result ^ sum as u8) & 0x80) != 0 {
                        status |= FLAG_V;
                    } else {
                        status &= !FLAG_V;
                    }
                    a = sum as u8;
                    set_nz!(a);
                }

                // NOP (various)
                0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {}
                // NOP with operand (various)
                0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {
                    pc = pc.wrapping_add(1);
                }
                0x04 | 0x14 | 0x34 | 0x44 | 0x54 | 0x64 | 0x74 | 0xD4 | 0xF4 => {
                    pc = pc.wrapping_add(1);
                }
                0x0C => {
                    pc = pc.wrapping_add(2);
                }
                0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                    pc = pc.wrapping_add(2);
                    time += 1;
                }

                // Illegal opcode 0xF2 - used as bad_opcode sentinel for NSF
                0xF2 => {
                    pc = pc.wrapping_sub(1); // Back up to bad opcode address (matching C++ pc--)
                    // Flush state back
                    self.r.pc = pc;
                    self.r.a = a;
                    self.r.x = x;
                    self.r.y = y;
                    self.r.sp = (sp & 0xFF) as u8;
                    // Rebuild status from nz, c
                    status = (status & !(FLAG_N | FLAG_Z | FLAG_C))
                        | if c & 0x100 != 0 { FLAG_C } else { 0 }
                        | if nz & 0xFF == 0 { FLAG_Z } else { 0 }
                        | if nz & 0x8080 != 0 { FLAG_N } else { 0 };
                    self.r.status = status;
                    self.state.time = time;
                    self.hit_badop = true;
                    return true;
                }

                _ => {
                    // Unknown opcode - treat as NOP
                    self.error_count += 1;
                }
            }
        }

        // Time expired - flush state
        self.r.pc = pc;
        self.r.a = a;
        self.r.x = x;
        self.r.y = y;
        self.r.sp = (sp & 0xFF) as u8;
        status = (status & !(FLAG_N | FLAG_Z | FLAG_C))
            | if c & 0x100 != 0 { FLAG_C } else { 0 }
            | if nz & 0xFF == 0 { FLAG_Z } else { 0 }
            | if nz & 0x8080 != 0 { FLAG_N } else { 0 };
        self.r.status = status;
        self.state.time = time;
        false
    }
}

/// Differential-trace hook for CPU instructions, mirroring the C++
/// NSF_CPU_TRACE hook in Nes_Cpu.cpp. Enabled via NSF_CPU_TRACE_RS=<path>.
/// Compiled out unless the `trace` cargo feature is on (it costs an
/// indirect call per instruction); diagnostic runs use
/// `cargo test -p gme-nsf --features trace` with the env var set.
#[cfg(feature = "trace")]
#[allow(clippy::too_many_arguments, clippy::collapsible_if)]
fn cpu_trace(pc: u16, opcode: u8, a: u8, x: u8, y: u8, p: u8, sp: u8, time: i32) {
    use std::io::Write;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Mutex, OnceLock};
    static TRACE: OnceLock<Option<Mutex<std::io::BufWriter<std::fs::File>>>> = OnceLock::new();
    static COUNT: AtomicI64 = AtomicI64::new(0);
    let f = TRACE.get_or_init(|| {
        std::env::var("NSF_CPU_TRACE_RS")
            .ok()
            .and_then(|p| std::fs::File::create(p).ok())
            .map(|f| Mutex::new(std::io::BufWriter::new(f)))
    });
    if let Some(f) = f {
        if COUNT.fetch_add(1, Ordering::Relaxed) < 2_000_000 {
            let _ = writeln!(
                f.lock().unwrap(),
                "{:04X} {:02X} A={:02X} X={:02X} Y={:02X} P={:02X} S={:02X} t={}",
                pc,
                opcode,
                a,
                x,
                y,
                p,
                sp,
                time
            );
        }
    }
}
