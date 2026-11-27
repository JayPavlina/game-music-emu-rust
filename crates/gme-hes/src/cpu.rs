//! PC Engine HuC6280 CPU state.
//! Port of C++ Hes_Cpu.h (the run loop lives in run.rs as `impl HesEmu`).

pub type HesTime = i32;

pub const FUTURE_HES_TIME: HesTime = i32::MAX / 2 + 1;

pub const PAGE_SIZE: usize = 0x2000;
pub const PAGE_SHIFT: u32 = 13;
pub const PAGE_COUNT: usize = 8;

/// Attempt to execute instruction here results in CPU advancing time to
/// lesser of irq_time() and end_time().
pub const IDLE_ADDR: u16 = 0x1FFF;

/// Can read this many bytes past end of a page.
pub const CPU_PAD: usize = 8;

// Status flags
pub const ST_N: u8 = 0x80;
pub const ST_V: u8 = 0x40;
#[allow(dead_code)]
pub const ST_T: u8 = 0x20;
pub const ST_B: u8 = 0x10;
pub const ST_D: u8 = 0x08;
pub const ST_I: u8 = 0x04;
pub const ST_Z: u8 = 0x02;
pub const ST_C: u8 = 0x01;

/// Where a code page maps to. Indices instead of raw pointers so the
/// emulator struct stays freely movable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageRef {
    /// Offset into RomData::data.
    Rom(usize),
    /// The CPU's 8K RAM page.
    Ram,
    /// Offset into the SGX RAM block.
    Sgx(usize),
}

/// Where writes to a page go (C++ write_pages; None = unmapped or I/O).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WritePage {
    None,
    Ram,
    Sgx(usize),
}

#[derive(Clone, Debug, Default)]
pub struct Registers {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub status: u8,
    pub sp: u8,
}

pub struct HesCpu {
    pub r: Registers,
    /// Page mapping registers.
    pub mmr: [u8; PAGE_COUNT + 1],
    /// 8K RAM page (+ padding for reads past the page end; C++ reads
    /// adjacent struct memory there, we read zeros).
    pub ram: [u8; PAGE_SIZE + CPU_PAD],
    pub code_map: [PageRef; PAGE_COUNT + 1],
    pub state_base: HesTime,
    pub state_time: i32,
    pub irq_time: HesTime,
    pub end_time: HesTime,
}

impl Default for HesCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl HesCpu {
    pub fn new() -> Self {
        Self {
            r: Registers::default(),
            mmr: [0; PAGE_COUNT + 1],
            ram: [0; PAGE_SIZE + CPU_PAD],
            code_map: [PageRef::Rom(0); PAGE_COUNT + 1],
            state_base: 0,
            state_time: 0,
            irq_time: FUTURE_HES_TIME,
            end_time: FUTURE_HES_TIME,
        }
    }

    /// C++ Hes_Cpu::reset.
    pub fn reset(&mut self) {
        self.state_time = 0;
        self.state_base = 0;
        self.irq_time = FUTURE_HES_TIME;
        self.end_time = FUTURE_HES_TIME;

        self.r.status = ST_I;
        self.r.sp = 0;
        self.r.pc = 0;
        self.r.a = 0;
        self.r.x = 0;
        self.r.y = 0;
    }

    /// Time of beginning of next instruction to be executed.
    #[inline]
    pub fn time(&self) -> HesTime {
        self.state_time + self.state_base
    }

    /// C++ Hes_Cpu::end_frame.
    pub fn end_frame(&mut self, t: HesTime) {
        self.state_base -= t;
        if self.irq_time < FUTURE_HES_TIME {
            self.irq_time -= t;
        }
        if self.end_time < FUTURE_HES_TIME {
            self.end_time -= t;
        }
    }
}

/// Per-opcode clock counts from C++ Hes_Cpu.cpp.
pub const CLOCK_TABLE: [u8; 256] = [
    // 0  1  2   3  4  5  6  7  8  9  A  B  C  D  E  F
    1, 7, 3, 4, 6, 4, 6, 7, 3, 2, 2, 2, 7, 5, 7, 6, // 0
    4, 7, 7, 4, 6, 4, 6, 7, 2, 5, 2, 2, 7, 5, 7, 6, // 1
    7, 7, 3, 4, 4, 4, 6, 7, 4, 2, 2, 2, 5, 5, 7, 6, // 2
    4, 7, 7, 2, 4, 4, 6, 7, 2, 5, 2, 2, 5, 5, 7, 6, // 3
    7, 7, 3, 4, 8, 4, 6, 7, 3, 2, 2, 2, 4, 5, 7, 6, // 4
    4, 7, 7, 5, 2, 4, 6, 7, 2, 5, 3, 2, 2, 5, 7, 6, // 5
    7, 7, 2, 2, 4, 4, 6, 7, 4, 2, 2, 2, 7, 5, 7, 6, // 6
    4, 7, 7, 17, 4, 4, 6, 7, 2, 5, 4, 2, 7, 5, 7, 6, // 7
    4, 7, 2, 7, 4, 4, 4, 7, 2, 2, 2, 2, 5, 5, 5, 6, // 8
    4, 7, 7, 8, 4, 4, 4, 7, 2, 5, 2, 2, 5, 5, 5, 6, // 9
    2, 7, 2, 7, 4, 4, 4, 7, 2, 2, 2, 2, 5, 5, 5, 6, // A
    4, 7, 7, 8, 4, 4, 4, 7, 2, 5, 2, 2, 5, 5, 5, 6, // B
    2, 7, 2, 17, 4, 4, 6, 7, 2, 2, 2, 2, 5, 5, 7, 6, // C
    4, 7, 7, 17, 2, 4, 6, 7, 2, 5, 3, 2, 2, 5, 7, 6, // D
    2, 7, 2, 17, 4, 4, 6, 7, 2, 2, 2, 2, 5, 5, 7, 6, // E
    4, 7, 7, 17, 2, 4, 6, 7, 2, 5, 4, 2, 2, 5, 7, 6, // F
];
