/// Game Boy CPU emulator.
/// Treats every instruction as taking 4 cycles.
/// Matches C++ Gb_Cpu from Gb_Cpu.h/Gb_Cpu.cpp.
const CLOCKS_PER_INSTR: i32 = 4;
const PAGE_SHIFT: usize = 13;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT; // 0x2000 = 8192
const PAGE_COUNT: usize = 0x10000 >> PAGE_SHIFT; // 8

/// CPU address type (16-bit).
pub type GbAddr = u16;

/// Flag constants (Game Boy flags at bits 7-4).
const Z_FLAG: u32 = 0x80;
const N_FLAG: u32 = 0x40;
const H_FLAG: u32 = 0x20;
const C_FLAG: u32 = 0x10;

/// CPU registers matching C++ Gb_Cpu::registers_t layout.
/// Little-endian layout: c,b,e,d,l,h,a,flags for pairs bc,de,hl.
#[derive(Clone, Debug, Default)]
pub struct Registers {
    // Individual byte registers (little-endian pair layout)
    pub c: u8,
    pub b: u8,
    pub e: u8,
    pub d: u8,
    pub l: u8,
    pub h: u8,
    pub a: u8,
    pub flags: u8,
    // 16-bit registers
    pub pc: u32, // more than 16 bits for overflow detection
    pub sp: u16,
}

impl Registers {
    #[inline]
    fn bc(&self) -> u16 {
        (self.b as u16) << 8 | self.c as u16
    }
    #[inline]
    fn de(&self) -> u16 {
        (self.d as u16) << 8 | self.e as u16
    }
    #[inline]
    fn hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }
    #[inline]
    fn set_bc(&mut self, v: u16) {
        self.b = (v >> 8) as u8;
        self.c = v as u8;
    }
    #[inline]
    fn set_de(&mut self, v: u16) {
        self.d = (v >> 8) as u8;
        self.e = v as u8;
    }
    #[inline]
    fn set_hl(&mut self, v: u16) {
        self.h = (v >> 8) as u8;
        self.l = v as u8;
    }

    /// Get register by index (0=B,1=C,2=D,3=E,4=H,5=L,6=(HL) dummy,7=A).
    /// Index 6 should NOT be used directly — handle (HL) separately.
    #[inline]
    fn r8(&self, idx: usize) -> u8 {
        match idx {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            7 => self.a,
            _ => 0, // should not happen
        }
    }

    /// Set register by index.
    #[inline]
    fn set_r8(&mut self, idx: usize, val: u8) {
        match idx {
            0 => self.b = val,
            1 => self.c = val,
            2 => self.d = val,
            3 => self.e = val,
            4 => self.h = val,
            5 => self.l = val,
            7 => self.a = val,
            _ => {} // should not happen
        }
    }

    /// Get register pair by index (0=BC,1=DE,2=HL,3=SP).
    #[inline]
    fn r16(&self, idx: usize) -> u16 {
        match idx {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            3 => self.sp,
            _ => 0,
        }
    }

    /// Set register pair by index.
    #[inline]
    fn set_r16(&mut self, idx: usize, val: u16) {
        match idx {
            0 => self.set_bc(val),
            1 => self.set_de(val),
            2 => self.set_hl(val),
            3 => self.sp = val,
            _ => {}
        }
    }
}

/// Game Boy CPU.
pub struct GbCpu {
    pub r: Registers,
    /// Base address for RST vectors (normally 0, set to load_addr for GBS).
    pub rst_base: u16,
    /// Code map: pointers into ROM/RAM data for each page.
    pub code_map: [*const u8; PAGE_COUNT + 1],
    /// Remaining cycles from most recent run().
    remain: i32,
}

// SAFETY: code_map pointers point into data owned by GbsEmu which holds this struct.
// The pointers are set during load/start_track and remain valid for the emulation frame.
unsafe impl Send for GbCpu {}

impl Default for GbCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl GbCpu {
    /// Address where CPU treats opcode 0xFF as illegal instruction (halt).
    pub const IDLE_ADDR: u16 = 0xF00D;
    /// Padding bytes allowed past end of a page.
    pub const CPU_PADDING: usize = 8;

    pub fn new() -> Self {
        Self {
            r: Registers::default(),
            rst_base: 0,
            code_map: [std::ptr::null(); PAGE_COUNT + 1],
            remain: 0,
        }
    }

    /// Reset CPU state. Sets all code pages to unmapped.
    ///
    /// # Safety
    /// `unmapped` must be a valid pointer that can be used for pointer arithmetic
    /// and stored in the code map. The caller must ensure no aliased access to
    /// `self.code_map` pages while they are live.
    pub unsafe fn reset(&mut self, unmapped: *const u8) {
        self.remain = 0;
        // C++ set_code_page stores the page's base data pointer; offsets within
        // the page are added at access time. All pages map to the same
        // unmapped buffer.
        self.code_map.fill(unmapped);
        self.r = Registers::default();
    }

    /// Map code memory. Start and size must be multiples of page_size.
    /// Stores pointers directly - code_map[page] points to the base of that page's data.
    ///
    /// # Safety
    /// `data` must point to at least `start + size` bytes of valid memory.
    /// `start` and `size` must be multiples of `PAGE_SIZE`.
    /// The caller must ensure no aliased access to mapped pages while they are live.
    pub unsafe fn map_code(&mut self, start: u16, size: u16, data: *const u8) {
        unsafe {
            debug_assert!((start as usize).is_multiple_of(PAGE_SIZE));
            debug_assert!((size as usize).is_multiple_of(PAGE_SIZE));
            let first_page = start as usize / PAGE_SIZE;
            for i in 0..size as usize / PAGE_SIZE {
                self.code_map[first_page + i] = data.add(i * PAGE_SIZE);
            }
        }
    }

    /// Get code pointer for address.
    #[inline]
    pub fn get_code(&self, addr: u16) -> *const u8 {
        let page = (addr as usize) >> PAGE_SHIFT;
        unsafe { self.code_map[page].add(addr as usize & (PAGE_SIZE - 1)) }
    }

    /// Push a byte onto the stack.
    pub fn push_byte(&mut self, data: u8, write: &mut impl FnMut(u32, u8)) {
        self.r.sp = self.r.sp.wrapping_sub(1);
        write(self.r.sp as u32, data);
    }

    /// Number of clock cycles remaining from most recent run().
    pub fn remain(&self) -> i32 {
        self.remain * CLOCKS_PER_INSTR
    }

    /// Run CPU for at least `cycle_count` cycles.
    /// Returns true if an illegal instruction or the idle-address 0xFF opcode
    /// stopped execution early (C++ returns `s.remain > 0`).
    /// `read` reads a byte from the address space (address may be > 0xFFFF,
    /// matching C++ where e.g. READ(sp + 1) is not masked).
    /// `write` writes a byte to the address space.
    #[allow(unused_assignments)]
    pub fn run(
        &mut self,
        cycle_count: i32,
        read: &impl Fn(u32) -> u8,
        write: &mut impl FnMut(u32, u8),
    ) -> bool {
        let mut s_remain = (cycle_count + CLOCKS_PER_INSTR) as u32 / CLOCKS_PER_INSTR as u32;
        self.remain = s_remain as i32;

        // Copy registers to local variables for speed
        let mut r = self.r.clone();
        let mut pc = r.pc;
        let mut sp = r.sp as u32;
        let mut flags = r.flags as u32;

        'main_loop: loop {
            // NOTE: pc is intentionally NOT masked to 16 bits here; like the
            // C++ it may exceed 0xFFFF and fall into the extra unmapped page.
            debug_assert!(sp < 0x10000);
            debug_assert!((flags & !0xF0) == 0);

            // Fetch opcode. The operand bytes are read through the SAME page
            // pointer as the opcode (matching C++ `instr`), which may read a
            // couple of bytes past the page end (cpu_padding allows this).
            let page = ((pc as usize) >> PAGE_SHIFT).min(PAGE_COUNT);
            let instr_ptr = self.code_map[page];
            let off = pc as usize & (PAGE_SIZE - 1);
            let op: u8 = unsafe { *instr_ptr.add(off) };
            pc = pc.wrapping_add(1);

            // Decrement remaining instructions and stop if exhausted.
            // Matches C++ Gb_Cpu::run: opcode is read, then s.remain is
            // decremented; if it reaches 0, we stop WITHOUT reading the
            // data byte or executing the instruction. This guarantees
            // cycle_count cycles are consumed per call.
            s_remain = s_remain.wrapping_sub(1);
            // Update self.remain so remain() returns current time during execution
            self.remain = s_remain as i32;
            if s_remain == 0 {
                break 'main_loop;
            }

            // Read next byte (data/operand) from the opcode's page pointer
            let data: u8 = unsafe { *instr_ptr.add(off + 1) };

            macro_rules! get_addr {
                () => {{
                    // C++ GET_LE16(instr): both operand bytes come from the
                    // opcode's page pointer (may read past page end into the
                    // following memory/padding).
                    let lo = data as u16;
                    let hi: u8 = unsafe { *instr_ptr.add(off + 2) };
                    pc = pc.wrapping_add(2);
                    (hi as u16) << 8 | lo
                }};
            }

            macro_rules! read_mem {
                ($addr:expr) => {
                    read($addr as u32)
                };
            }

            macro_rules! write_mem {
                ($addr:expr, $val:expr) => {
                    write($addr as u32, $val as u8);
                };
            }

            match op {
                // ===== Most Common =====

                // JR NZ
                0x20 => {
                    pc = pc.wrapping_add(1);
                    let offset = data as i8 as i32;
                    if flags & Z_FLAG == 0 {
                        pc = (pc as i32 + offset) as u32 & 0xFFFF;
                    }
                }

                // LD HL,IMM
                0x21 => {
                    let addr = get_addr!();
                    r.set_hl(addr);
                }

                // JR Z
                0x28 => {
                    pc = pc.wrapping_add(1);
                    let offset = data as i8 as i32;
                    if flags & Z_FLAG != 0 {
                        pc = (pc as i32 + offset) as u32 & 0xFFFF;
                    }
                }

                // LD A,(0xFF00+imm)
                0xF0 => {
                    let addr = 0xFF00u16 | data as u16;
                    pc = pc.wrapping_add(1);
                    r.a = read_mem!(addr);
                }

                // LD A,(0xFF00+C)
                0xF2 => {
                    let addr = 0xFF00u16 | r.c as u16;
                    r.a = read_mem!(addr);
                }

                // LD A,(BC)
                0x0A => {
                    let addr = r.bc();
                    r.a = read_mem!(addr);
                }

                // LD A,(HL-)
                0x3A => {
                    let addr = r.hl();
                    r.set_hl(addr.wrapping_sub(1));
                    r.a = read_mem!(addr);
                }

                // LD A,(DE)
                0x1A => {
                    let addr = r.de();
                    r.a = read_mem!(addr);
                }

                // LD A,(HL+)
                0x2A => {
                    let addr = r.hl();
                    r.set_hl(addr.wrapping_add(1));
                    r.a = read_mem!(addr);
                }

                // LD A,IND16
                0xFA => {
                    let addr = get_addr!();
                    r.a = read_mem!(addr);
                }

                // CMP (HL)
                0xBE => {
                    let val = read_mem!(r.hl());
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val as u32);
                    flags = ((a & 15).wrapping_sub(val as u32 & 15)) & H_FLAG;
                    flags |= (diff >> 4) & C_FLAG;
                    flags |= N_FLAG;
                    if diff & 0xFF == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // CMP B,C,D,E,H,L
                0xB8..=0xBD => {
                    let val = r.r8((op & 7) as usize) as u32;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = ((a & 15).wrapping_sub(val & 15)) & H_FLAG;
                    flags |= (diff >> 4) & C_FLAG;
                    flags |= N_FLAG;
                    if diff & 0xFF == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // CMP IMM
                0xFE => {
                    pc = pc.wrapping_add(1);
                    let val = data as u32;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = ((a & 15).wrapping_sub(val & 15)) & H_FLAG;
                    flags |= (diff >> 4) & C_FLAG;
                    flags |= N_FLAG;
                    if diff & 0xFF == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // LD r,(HL)
                0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                    let addr = r.hl();
                    let val = read_mem!(addr);
                    r.set_r8(((op >> 3) & 7) as usize, val);
                }

                // CNZ
                0xC4 => {
                    let addr = get_addr!();
                    if flags & Z_FLAG == 0 {
                        let ret_addr = pc;
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret_addr >> 8) as u8);
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret_addr & 0xFF) as u8);
                        pc = addr as u32;
                    }
                }

                // CALL
                0xCD => {
                    let addr = get_addr!();
                    let ret_addr = pc;
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (ret_addr >> 8) as u8);
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (ret_addr & 0xFF) as u8);
                    pc = addr as u32;
                }

                // RNZ
                0xC8 => {
                    if flags & Z_FLAG != 0 {
                        let lo = read_mem!(sp) as u32;
                        let hi = read_mem!(sp.wrapping_add(1)) as u32;
                        pc = hi << 8 | lo;
                        sp = sp.wrapping_add(2) & 0xFFFF;
                    }
                }

                // RET
                0xC9 => {
                    let lo = read_mem!(sp) as u32;
                    let hi = read_mem!(sp.wrapping_add(1)) as u32;
                    pc = hi << 8 | lo;
                    sp = sp.wrapping_add(2) & 0xFFFF;
                }

                // NOP, LD r,r (same register)
                0x00 | 0x40 | 0x49 | 0x52 | 0x5B | 0x64 | 0x6D | 0x7F => {}

                // ===== CB Prefix =====
                0xCB => {
                    pc = pc.wrapping_add(1);
                    let cb_op = data;
                    self.execute_cb_op(
                        cb_op,
                        &mut r,
                        &mut flags,
                        &mut sp,
                        read,
                        write,
                    );
                }

                // ===== Rotate/Shift A =====

                // RLCA — C++ shares rl_comm/shift_comm, which also sets Z
                // when the result is zero (a deliberate transliteration of
                // the reference, not hardware-accurate behavior).
                0x07 => {
                    let val = r.a as u32;
                    let result = ((val << 1) | (val >> 7)) & 0xFF;
                    flags = ((val << 1) >> 4) & C_FLAG;
                    if result == 0 {
                        flags |= Z_FLAG;
                    }
                    r.a = result as u8;
                }

                // RLA (C++ also computes Z via shift_comm)
                0x17 => {
                    let carry = (flags >> 4) & 1;
                    let val = r.a as u32;
                    let op9 = (val << 1) | carry;
                    flags = (op9 >> 4) & C_FLAG;
                    if op9 & 0xFF == 0 {
                        flags |= Z_FLAG;
                    }
                    r.a = (op9 & 0xFF) as u8;
                }

                // RRCA (C++ also computes Z via shift_comm)
                0x0F => {
                    let val = r.a as u32;
                    let result = ((val >> 1) | (val << 7)) & 0xFF;
                    flags = (val << 4) & C_FLAG;
                    if result == 0 {
                        flags |= Z_FLAG;
                    }
                    r.a = result as u8;
                }

                // RRA (C++ also computes Z via shift_comm)
                0x1F => {
                    let carry = (flags >> 4) & 1;
                    let val = r.a as u32;
                    let result = ((val >> 1) | (carry << 7)) & 0xFF;
                    flags = (val << 4) & C_FLAG;
                    if result == 0 {
                        flags |= Z_FLAG;
                    }
                    r.a = result as u8;
                }

                // ===== Load r,r =====

                // LD r,r (different registers)
                0x41..=0x45 | 0x47 => {
                    let src = r.r8((op & 7) as usize);
                    r.set_r8(((op >> 3) & 7) as usize, src);
                }
                0x48 | 0x4A..=0x4D | 0x4F => {
                    let src = r.r8((op & 7) as usize);
                    r.set_r8(((op >> 3) & 7) as usize, src);
                }
                0x50 | 0x51 | 0x53..=0x55 | 0x57 => {
                    let src = r.r8((op & 7) as usize);
                    r.set_r8(((op >> 3) & 7) as usize, src);
                }
                0x58..=0x5D | 0x5F => {
                    let src = r.r8((op & 7) as usize);
                    r.set_r8(((op >> 3) & 7) as usize, src);
                }
                0x60..=0x63 | 0x65 | 0x67 => {
                    let src = r.r8((op & 7) as usize);
                    r.set_r8(((op >> 3) & 7) as usize, src);
                }
                0x68..=0x6C | 0x6F => {
                    let src = r.r8((op & 7) as usize);
                    r.set_r8(((op >> 3) & 7) as usize, src);
                }
                0x78..=0x7D => {
                    let src = r.r8((op & 7) as usize);
                    r.set_r8(((op >> 3) & 7) as usize, src);
                }

                // LD (HL),r
                0x70..=0x75 | 0x77 => {
                    let val = r.r8((op & 7) as usize);
                    write_mem!(r.hl(), val);
                }

                // LD IND16,SP
                0x08 => {
                    let addr = get_addr!();
                    write_mem!(addr, (sp & 0xFF) as u8);
                    write_mem!(addr.wrapping_add(1), (sp >> 8) as u8);
                }

                // LD SP,HL
                0xF9 => {
                    sp = r.hl() as u32;
                }

                // LD SP,IMM
                0x31 => {
                    sp = get_addr!() as u32;
                }

                // LD BC,IMM / LD DE,IMM
                0x01 | 0x11 => {
                    let addr = get_addr!();
                    r.set_r16((op >> 4) as usize, addr);
                }

                // LD (0xFF00+imm),A
                0xE0 => {
                    let addr = 0xFF00u16 | data as u16;
                    pc = pc.wrapping_add(1);
                    write_mem!(addr, r.a);
                }

                // LD (0xFF00+C),A
                0xE2 => {
                    let addr = 0xFF00u16 | r.c as u16;
                    write_mem!(addr, r.a);
                }

                // LD (HL-),A
                0x32 => {
                    let addr = r.hl();
                    r.set_hl(addr.wrapping_sub(1));
                    write_mem!(addr, r.a);
                }

                // LD (BC),A
                0x02 => {
                    write_mem!(r.bc(), r.a);
                }

                // LD (DE),A
                0x12 => {
                    write_mem!(r.de(), r.a);
                }

                // LD (HL+),A
                0x22 => {
                    let addr = r.hl();
                    r.set_hl(addr.wrapping_add(1));
                    write_mem!(addr, r.a);
                }

                // LD IND16,A
                0xEA => {
                    let addr = get_addr!();
                    write_mem!(addr, r.a);
                }

                // LD r,IMM
                0x06 => { r.b = data; pc = pc.wrapping_add(1); }
                0x0E => { r.c = data; pc = pc.wrapping_add(1); }
                0x16 => { r.d = data; pc = pc.wrapping_add(1); }
                0x1E => { r.e = data; pc = pc.wrapping_add(1); }
                0x26 => { r.h = data; pc = pc.wrapping_add(1); }
                0x2E => { r.l = data; pc = pc.wrapping_add(1); }

                // LD (HL),IMM
                0x36 => {
                    write_mem!(r.hl(), data);
                    pc = pc.wrapping_add(1);
                }

                // LD A,IMM
                0x3E => { r.a = data; pc = pc.wrapping_add(1); }

                // ===== Increment/Decrement =====

                // INC BC/DE/HL
                0x03 | 0x13 | 0x23 => {
                    let idx = (op >> 4) as usize;
                    r.set_r16(idx, r.r16(idx).wrapping_add(1));
                }

                // INC SP
                0x33 => {
                    sp = sp.wrapping_add(1) & 0xFFFF;
                }

                // DEC BC/DE/HL
                0x0B | 0x1B | 0x2B => {
                    let idx = (op >> 4) as usize;
                    r.set_r16(idx, r.r16(idx).wrapping_sub(1));
                }

                // DEC SP
                0x3B => {
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                }

                // INC (HL)
                0x34 => {
                    let addr = r.hl();
                    let val = read_mem!(addr) as u32;
                    let result = val.wrapping_add(1);
                    write_mem!(addr, (result & 0xFF) as u8);
                    // C++ inc_comm uses the NEW value: ((data & 15) - 1) & h_flag
                    flags = (flags & C_FLAG)
                        | (((result & 15).wrapping_sub(1)) & H_FLAG)
                        | ((result >> 1) & Z_FLAG);
                }

                // INC r
                0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => {
                    let idx = ((op >> 3) & 7) as usize;
                    let val = r.r8(idx) as u32;
                    let result = val.wrapping_add(1);
                    r.set_r8(idx, (result & 0xFF) as u8);
                    // C++ inc_comm uses the NEW value: ((data & 15) - 1) & h_flag
                    flags = (flags & C_FLAG)
                        | (((result & 15).wrapping_sub(1)) & H_FLAG)
                        | ((result >> 1) & Z_FLAG);
                }

                // DEC (HL)
                0x35 => {
                    let addr = r.hl();
                    let val = read_mem!(addr) as u32;
                    let result = val.wrapping_sub(1);
                    write_mem!(addr, (result & 0xFF) as u8);
                    // C++ dec_comm uses the NEW value: ((data & 15) + 0x31) & h_flag
                    flags =
                        (flags & C_FLAG) | N_FLAG | (((result & 15).wrapping_add(0x31)) & H_FLAG);
                    if result & 0xFF == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // DEC r
                0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => {
                    let idx = ((op >> 3) & 7) as usize;
                    let val = r.r8(idx) as u32;
                    let result = val.wrapping_sub(1);
                    r.set_r8(idx, (result & 0xFF) as u8);
                    // C++ dec_comm uses the NEW value: ((data & 15) + 0x31) & h_flag
                    flags =
                        (flags & C_FLAG) | N_FLAG | (((result & 15).wrapping_add(0x31)) & H_FLAG);
                    if result & 0xFF == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ===== Add 16-bit =====

                // LD HL,SP+imm — C++ add_16_hl/add_16_comm
                0xF8 => {
                    let temp = (data as i8 as i32 as u32).wrapping_add(sp);
                    pc = pc.wrapping_add(1);
                    let prev = sp;
                    r.set_hl((temp & 0xFFFF) as u16);
                    flags = (temp >> 12) & C_FLAG;
                    flags |= (((temp & 0x0FFF).wrapping_sub(prev & 0x0FFF)) >> 7) & H_FLAG;
                }

                // ADD SP,IMM — C++ add_16_comm
                0xE8 => {
                    let temp = (data as i8 as i32 as u32).wrapping_add(sp);
                    pc = pc.wrapping_add(1);
                    let prev = sp;
                    sp = temp & 0xFFFF;
                    flags = (temp >> 12) & C_FLAG;
                    flags |= (((temp & 0x0FFF).wrapping_sub(prev & 0x0FFF)) >> 7) & H_FLAG;
                }

                // ADD HL,SP — C++ add_hl_comm
                0x39 => {
                    let prev = r.hl() as u32;
                    let temp = sp.wrapping_add(prev);
                    flags &= Z_FLAG;
                    r.set_hl((temp & 0xFFFF) as u16);
                    flags |= (temp >> 12) & C_FLAG;
                    flags |= (((temp & 0x0FFF).wrapping_sub(prev & 0x0FFF)) >> 7) & H_FLAG;
                }

                // ADD HL,BC/DE/HL — C++ add_hl_comm
                0x09 | 0x19 | 0x29 => {
                    let idx = (op >> 4) as usize;
                    let prev = r.hl() as u32;
                    let temp = (r.r16(idx) as u32).wrapping_add(prev);
                    flags &= Z_FLAG;
                    r.set_hl((temp & 0xFFFF) as u16);
                    flags |= (temp >> 12) & C_FLAG;
                    flags |= (((temp & 0x0FFF).wrapping_sub(prev & 0x0FFF)) >> 7) & H_FLAG;
                }

                // ===== Add/Subtract =====

                // ADD (HL)
                0x86 => {
                    let val = read_mem!(r.hl()) as u32;
                    let a = r.a as u32;
                    let result = a + val;
                    flags = 0;
                    if (result & 0xF) < (a & 0xF) {
                        flags |= H_FLAG;
                    }
                    if result > 0xFF {
                        flags |= C_FLAG;
                    }
                    r.a = (result & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ADD r
                0x80..=0x85 | 0x87 => {
                    let val = r.r8((op & 7) as usize) as u32;
                    let a = r.a as u32;
                    let result = a + val;
                    flags = 0;
                    if (result & 0xF) < (a & 0xF) {
                        flags |= H_FLAG;
                    }
                    if result > 0xFF {
                        flags |= C_FLAG;
                    }
                    r.a = (result & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ADD IMM
                0xC6 => {
                    pc = pc.wrapping_add(1);
                    let val = data as u32;
                    let a = r.a as u32;
                    let result = a + val;
                    flags = 0;
                    if (result & 0xF) < (a & 0xF) {
                        flags |= H_FLAG;
                    }
                    if result > 0xFF {
                        flags |= C_FLAG;
                    }
                    r.a = (result & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ADC (HL)
                0x8E => {
                    // C++ adc_comm masks the operand+carry to 8 bits
                    let val = (read_mem!(r.hl()) as u32 + ((flags >> 4) & 1)) & 0xFF;
                    let a = r.a as u32;
                    let result = a + val;
                    flags = 0;
                    if (result & 0xF) < (a & 0xF) {
                        flags |= H_FLAG;
                    }
                    if result > 0xFF {
                        flags |= C_FLAG;
                    }
                    r.a = (result & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ADC r
                0x88..=0x8D | 0x8F => {
                    // C++ adc_comm masks the operand+carry to 8 bits
                    let val = (r.r8((op & 7) as usize) as u32 + ((flags >> 4) & 1)) & 0xFF;
                    let a = r.a as u32;
                    let result = a + val;
                    flags = 0;
                    if (result & 0xF) < (a & 0xF) {
                        flags |= H_FLAG;
                    }
                    if result > 0xFF {
                        flags |= C_FLAG;
                    }
                    r.a = (result & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ADC IMM
                0xCE => {
                    pc = pc.wrapping_add(1);
                    // C++ adc_comm masks the operand+carry to 8 bits
                    let val = (data as u32 + ((flags >> 4) & 1)) & 0xFF;
                    let a = r.a as u32;
                    let result = a + val;
                    flags = 0;
                    if (result & 0xF) < (a & 0xF) {
                        flags |= H_FLAG;
                    }
                    if result > 0xFF {
                        flags |= C_FLAG;
                    }
                    r.a = (result & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // SUB (HL)
                0x96 => {
                    let val = read_mem!(r.hl()) as u32;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = N_FLAG;
                    if (a & 15) < (val & 15) {
                        flags |= H_FLAG;
                    }
                    if a < val {
                        flags |= C_FLAG;
                    }
                    r.a = (diff & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // SUB r
                0x90..=0x95 | 0x97 => {
                    let val = r.r8((op & 7) as usize) as u32;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = N_FLAG;
                    if (a & 15) < (val & 15) {
                        flags |= H_FLAG;
                    }
                    if a < val {
                        flags |= C_FLAG;
                    }
                    r.a = (diff & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // SUB IMM
                0xD6 => {
                    pc = pc.wrapping_add(1);
                    let val = data as u32;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = N_FLAG;
                    if (a & 15) < (val & 15) {
                        flags |= H_FLAG;
                    }
                    if a < val {
                        flags |= C_FLAG;
                    }
                    r.a = (diff & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // SBC (HL)
                0x9E => {
                    // C++ sbc_comm masks the operand+carry to 8 bits
                    let val = (read_mem!(r.hl()) as u32 + ((flags >> 4) & 1)) & 0xFF;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = N_FLAG;
                    if (a & 15) < (val & 15) {
                        flags |= H_FLAG;
                    }
                    if a < val {
                        flags |= C_FLAG;
                    }
                    r.a = (diff & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // SBC r
                0x98..=0x9D | 0x9F => {
                    // C++ sbc_comm masks the operand+carry to 8 bits
                    let val = (r.r8((op & 7) as usize) as u32 + ((flags >> 4) & 1)) & 0xFF;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = N_FLAG;
                    if (a & 15) < (val & 15) {
                        flags |= H_FLAG;
                    }
                    if a < val {
                        flags |= C_FLAG;
                    }
                    r.a = (diff & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // SBC IMM
                0xDE => {
                    pc = pc.wrapping_add(1);
                    // C++ sbc_comm masks the operand+carry to 8 bits
                    let val = (data as u32 + ((flags >> 4) & 1)) & 0xFF;
                    let a = r.a as u32;
                    let diff = a.wrapping_sub(val);
                    flags = N_FLAG;
                    if (a & 15) < (val & 15) {
                        flags |= H_FLAG;
                    }
                    if a < val {
                        flags |= C_FLAG;
                    }
                    r.a = (diff & 0xFF) as u8;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ===== Logical =====

                // AND (HL)
                0xA6 => {
                    let val = read_mem!(r.hl());
                    r.a &= val;
                    flags = H_FLAG;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // AND r
                0xA0..=0xA5 => {
                    r.a &= r.r8((op & 7) as usize);
                    flags = H_FLAG;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // AND A
                0xA7 => {
                    flags = H_FLAG;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // AND IMM
                0xE6 => {
                    pc = pc.wrapping_add(1);
                    r.a &= data;
                    flags = H_FLAG;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // OR (HL)
                0xB6 => {
                    r.a |= read_mem!(r.hl());
                    flags = 0;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // OR r
                0xB0..=0xB5 => {
                    r.a |= r.r8((op & 7) as usize);
                    flags = 0;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // OR A
                0xB7 => {
                    flags = 0;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // OR IMM
                0xF6 => {
                    pc = pc.wrapping_add(1);
                    r.a |= data;
                    flags = 0;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // XOR (HL)
                0xAE => {
                    r.a ^= read_mem!(r.hl());
                    flags = 0;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // XOR r
                0xA8..=0xAD => {
                    r.a ^= r.r8((op & 7) as usize);
                    flags = 0;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // XOR A
                0xAF => {
                    r.a = 0;
                    flags = Z_FLAG;
                }

                // XOR IMM
                0xEE => {
                    pc = pc.wrapping_add(1);
                    r.a ^= data;
                    flags = 0;
                    if r.a == 0 {
                        flags |= Z_FLAG;
                    }
                }

                // ===== Stack =====

                // POP AF
                0xF1 => {
                    let lo = read_mem!(sp);
                    let hi = read_mem!(sp.wrapping_add(1));
                    sp = sp.wrapping_add(2) & 0xFFFF;
                    r.a = hi;
                    flags = (lo & 0xF0) as u32;
                }

                // POP BC/DE/HL
                0xC1 | 0xD1 | 0xE1 => {
                    let lo = read_mem!(sp);
                    let hi = read_mem!(sp.wrapping_add(1));
                    sp = sp.wrapping_add(2) & 0xFFFF;
                    let val = (hi as u16) << 8 | lo as u16;
                    r.set_r16(((op >> 4) & 3) as usize, val);
                }

                // PUSH BC
                0xC5 => {
                    let val = r.bc();
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (val >> 8) as u8);
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (val & 0xFF) as u8);
                }

                // PUSH DE
                0xD5 => {
                    let val = r.de();
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (val >> 8) as u8);
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (val & 0xFF) as u8);
                }

                // PUSH HL
                0xE5 => {
                    let val = r.hl();
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (val >> 8) as u8);
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (val & 0xFF) as u8);
                }

                // PUSH AF
                0xF5 => {
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, r.a);
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (flags & 0xF0) as u8);
                }

                // ===== Flow Control =====

                // RST 0xFF (special: halt at idle_addr)
                0xFF => {
                    if pc == Self::IDLE_ADDR as u32 + 1 {
                        break 'main_loop;
                    }
                    // Fall through to RST
                    let ret = pc;
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (ret >> 8) as u8);
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (ret & 0xFF) as u8);
                    pc = ((op & 0x38) as u32).wrapping_add(self.rst_base as u32);
                }

                // RST
                0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 => {
                    let ret = pc;
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (ret >> 8) as u8);
                    sp = sp.wrapping_sub(1) & 0xFFFF;
                    write_mem!(sp, (ret & 0xFF) as u8);
                    pc = ((op & 0x38) as u32).wrapping_add(self.rst_base as u32);
                }

                // CZ
                0xCC => {
                    let addr = get_addr!();
                    if flags & Z_FLAG != 0 {
                        let ret = pc;
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret >> 8) as u8);
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret & 0xFF) as u8);
                        pc = addr as u32;
                    }
                }

                // CNC
                0xD4 => {
                    let addr = get_addr!();
                    if flags & C_FLAG == 0 {
                        let ret = pc;
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret >> 8) as u8);
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret & 0xFF) as u8);
                        pc = addr as u32;
                    }
                }

                // CC
                0xDC => {
                    let addr = get_addr!();
                    if flags & C_FLAG != 0 {
                        let ret = pc;
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret >> 8) as u8);
                        sp = sp.wrapping_sub(1) & 0xFFFF;
                        write_mem!(sp, (ret & 0xFF) as u8);
                        pc = addr as u32;
                    }
                }

                // RETI
                0xD9 => {
                    let lo = read_mem!(sp) as u32;
                    let hi = read_mem!(sp.wrapping_add(1)) as u32;
                    pc = hi << 8 | lo;
                    sp = sp.wrapping_add(2) & 0xFFFF;
                }

                // RNZ (RET NZ)
                0xC0 => {
                    if flags & Z_FLAG == 0 {
                        let lo = read_mem!(sp) as u32;
                        let hi = read_mem!(sp.wrapping_add(1)) as u32;
                        pc = hi << 8 | lo;
                        sp = sp.wrapping_add(2) & 0xFFFF;
                    }
                }

                // RNC
                0xD0 => {
                    if flags & C_FLAG == 0 {
                        let lo = read_mem!(sp) as u32;
                        let hi = read_mem!(sp.wrapping_add(1)) as u32;
                        pc = hi << 8 | lo;
                        sp = sp.wrapping_add(2) & 0xFFFF;
                    }
                }

                // RC
                0xD8 => {
                    if flags & C_FLAG != 0 {
                        let lo = read_mem!(sp) as u32;
                        let hi = read_mem!(sp.wrapping_add(1)) as u32;
                        pc = hi << 8 | lo;
                        sp = sp.wrapping_add(2) & 0xFFFF;
                    }
                }

                // JR
                0x18 => {
                    pc = pc.wrapping_add(1);
                    let offset = data as i8 as i32;
                    pc = (pc as i32 + offset) as u32 & 0xFFFF;
                }

                // JR NC
                0x30 => {
                    pc = pc.wrapping_add(1);
                    let offset = data as i8 as i32;
                    if flags & C_FLAG == 0 {
                        pc = (pc as i32 + offset) as u32 & 0xFFFF;
                    }
                }

                // JR C
                0x38 => {
                    pc = pc.wrapping_add(1);
                    let offset = data as i8 as i32;
                    if flags & C_FLAG != 0 {
                        pc = (pc as i32 + offset) as u32 & 0xFFFF;
                    }
                }

                // JP HL
                0xE9 => {
                    pc = r.hl() as u32;
                }

                // JP
                0xC3 => {
                    pc = get_addr!() as u32;
                }

                // JP NZ
                0xC2 => {
                    let addr = get_addr!();
                    if flags & Z_FLAG == 0 {
                        pc = addr as u32;
                    }
                }

                // JP Z
                0xCA => {
                    let addr = get_addr!();
                    if flags & Z_FLAG != 0 {
                        pc = addr as u32;
                    }
                }

                // JP NC
                0xD2 => {
                    let addr = get_addr!();
                    if flags & C_FLAG == 0 {
                        pc = addr as u32;
                    }
                }

                // JP C
                0xDA => {
                    let addr = get_addr!();
                    if flags & C_FLAG != 0 {
                        pc = addr as u32;
                    }
                }

                // ===== Flags =====

                // CPL
                0x2F => {
                    r.a = !r.a;
                    flags |= N_FLAG | H_FLAG;
                }

                // CCF
                0x3F => {
                    flags = (flags ^ C_FLAG) & !(N_FLAG | H_FLAG);
                }

                // SCF
                0x37 => {
                    flags = (flags | C_FLAG) & !(N_FLAG | H_FLAG);
                }

                // DI
                0xF3 => {
                    // interrupts_enabled = false (not used in GBS)
                }

                // EI
                0xFB => {
                    // interrupts_enabled = true (not used in GBS)
                }

                // ===== Special/Illegal =====

                0xDD | 0xD3 | 0xDB | 0xE3 | 0xE4 |
                0xEB | 0xEC | 0xF4 | 0xFD | 0xFC |
                0x10 | // STOP
                0x27 | // DAA
                0xBF |
                0xED | // Z80 prefix
                0x76 => { // HALT
                    // Illegal/unimplemented — stop CPU
                    s_remain = s_remain.wrapping_add(1);
                    break 'main_loop;
                }


            }
        }

        // Copy state back (pc-1 to match C++ which decrements before storing)
        pc = pc.wrapping_sub(1);
        r.pc = pc;
        r.sp = sp as u16;
        r.flags = (flags & 0xF0) as u8;
        self.r = r;
        self.remain = s_remain as i32;

        // C++ returns `s.remain > 0`: true when stopped early by the idle
        // 0xFF opcode or an illegal/unsupported instruction.
        s_remain > 0
    }

    /// Execute a CB-prefixed opcode.
    fn execute_cb_op(
        &self,
        cb_op: u8,
        r: &mut Registers,
        flags: &mut u32,
        _sp: &mut u32,
        read: &impl Fn(u32) -> u8,
        write: &mut impl FnMut(u32, u8),
    ) {
        match cb_op {
            // BIT b,(HL)
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x76 | 0x7E => {
                let val = read(r.hl() as u32) as u32;
                let bit = ((!cb_op >> 3) & 7) as u32;
                *flags &= !N_FLAG;
                *flags |= H_FLAG | Z_FLAG;
                *flags ^= (val << bit) & Z_FLAG;
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
                let val = r.r8((cb_op & 7) as usize) as u32;
                let bit = ((!cb_op >> 3) & 7) as u32;
                *flags &= !N_FLAG;
                *flags |= H_FLAG | Z_FLAG;
                *flags ^= (val << bit) & Z_FLAG;
            }

            // RES b,(HL)
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                let val = read(r.hl() as u32);
                let bit = 1 << ((cb_op >> 3) & 7);
                write(r.hl() as u32, val & !bit);
            }

            // SET b,(HL)
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                let val = read(r.hl() as u32);
                let bit = 1 << ((cb_op >> 3) & 7);
                write(r.hl() as u32, val | bit);
            }

            // SET b,r
            0xC0..=0xC5
            | 0xC7
            | 0xC8..=0xCD
            | 0xCF
            | 0xD0..=0xD5
            | 0xD7
            | 0xD8..=0xDD
            | 0xDF
            | 0xE0..=0xE5
            | 0xE7
            | 0xE8..=0xED
            | 0xEF
            | 0xF0..=0xF5
            | 0xF7
            | 0xF8..=0xFD
            | 0xFF => {
                let bit = 1 << ((cb_op >> 3) & 7);
                let idx = (cb_op & 7) as usize;
                r.set_r8(idx, r.r8(idx) | bit);
            }

            // RES b,r
            0x80..=0x85
            | 0x87
            | 0x89..=0x8D
            | 0x8F
            | 0x90..=0x95
            | 0x97
            | 0x98..=0x9D
            | 0x9F
            | 0xA0..=0xA5
            | 0xA7
            | 0xA8..=0xAD
            | 0xAF
            | 0xB0..=0xB5
            | 0xB7
            | 0xB8..=0xBD
            | 0xBF => {
                let bit = 1 << ((cb_op >> 3) & 7);
                let idx = (cb_op & 7) as usize;
                r.set_r8(idx, r.r8(idx) & !bit);
            }

            // SWAP (HL)
            0x36 => {
                let val = read(r.hl() as u32);
                let result = val.rotate_left(4);
                write(r.hl() as u32, result);
                *flags = 0;
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // SWAP r
            0x30..=0x35 | 0x37 => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx);
                let result = val.rotate_left(4);
                r.set_r8(idx, result);
                *flags = 0;
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RLC (HL)
            0x06 => {
                let val = read(r.hl() as u32) as u32;
                let carry = (val >> 7) & 1;
                let result = ((val << 1) | (val >> 7)) & 0xFF;
                write(r.hl() as u32, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RL (HL)
            0x16 => {
                let val = read(r.hl() as u32) as u32;
                let old_carry = (*flags >> 4) & 1;
                let carry = (val >> 7) & 1;
                let result = ((val << 1) | old_carry) & 0xFF;
                write(r.hl() as u32, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // SLA (HL)
            0x26 => {
                let val = read(r.hl() as u32) as u32;
                let carry = (val >> 7) & 1;
                let result = (val << 1) & 0xFF;
                write(r.hl() as u32, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RRC (HL)
            0x0E => {
                let val = read(r.hl() as u32) as u32;
                let carry = val & 1;
                let result = ((val >> 1) | (val << 7)) & 0xFF;
                write(r.hl() as u32, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RR (HL)
            0x1E => {
                let val = read(r.hl() as u32) as u32;
                let old_carry = (*flags >> 4) & 1;
                let carry = val & 1;
                let result = ((val >> 1) | (old_carry << 7)) & 0xFF;
                write(r.hl() as u32, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // SRA (HL)
            0x2E => {
                let val = read(r.hl() as u32) as u32;
                let carry = val & 1;
                let result = ((val >> 1) | (val & 0x80)) & 0xFF; // preserve sign
                write(r.hl() as u32, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // SRL (HL)
            0x3E => {
                let val = read(r.hl() as u32) as u32;
                let carry = val & 1;
                let result = (val >> 1) & 0xFF;
                write(r.hl() as u32, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RLC r
            0x00..=0x05 | 0x07 => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx) as u32;
                let carry = (val >> 7) & 1;
                let result = ((val << 1) | (val >> 7)) & 0xFF;
                r.set_r8(idx, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RL r
            0x10..=0x15 | 0x17 => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx) as u32;
                let old_carry = (*flags >> 4) & 1;
                let carry = (val >> 7) & 1;
                let result = ((val << 1) | old_carry) & 0xFF;
                r.set_r8(idx, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // SLA r
            0x20..=0x25 | 0x27 => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx) as u32;
                let carry = (val >> 7) & 1;
                let result = (val << 1) & 0xFF;
                r.set_r8(idx, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RRC r
            0x08..=0x0D | 0x0F => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx) as u32;
                let carry = val & 1;
                let result = ((val >> 1) | (val << 7)) & 0xFF;
                r.set_r8(idx, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // RR r
            0x18..=0x1D | 0x1F => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx) as u32;
                let old_carry = (*flags >> 4) & 1;
                let carry = val & 1;
                let result = ((val >> 1) | (old_carry << 7)) & 0xFF;
                r.set_r8(idx, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // SRA r
            0x28..=0x2D | 0x2F => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx) as u32;
                let carry = val & 1;
                let result = ((val >> 1) | (val & 0x80)) & 0xFF;
                r.set_r8(idx, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            // SRL r
            0x38..=0x3D | 0x3F => {
                let idx = (cb_op & 7) as usize;
                let val = r.r8(idx) as u32;
                let carry = val & 1;
                let result = (val >> 1) & 0xFF;
                r.set_r8(idx, result as u8);
                *flags = 0;
                if carry != 0 {
                    *flags |= C_FLAG;
                }
                if result == 0 {
                    *flags |= Z_FLAG;
                }
            }

            _ => {
                // Unhandled CB opcode — ignore
            }
        }
    }
}
