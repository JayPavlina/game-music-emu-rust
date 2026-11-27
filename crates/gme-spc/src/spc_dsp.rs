//! Fast SNES SPC-700 DSP emulator (SPC_LESS_ACCURATE).
//!
//! 1:1 transliteration of game-music-emu's Spc_Dsp.cpp / Spc_Dsp.h
//! (the vendored copy in crates/game-music-emu-wrapper/src/gme/).
//! Control flow, evaluation order, and integer truncation points are kept
//! identical to the C++; the differential tests in tests/ assert bit-exact
//! output against it. C++ `goto skip_brr` / `goto exit_env` map to
//! `break` out of the labeled blocks of the same names.

pub const VOICE_COUNT: usize = 8;
pub const REGISTER_COUNT: usize = 128;
pub const EXTRA_SIZE: usize = 16;

// DSP register addresses (global)
pub const R_MVOLL: usize = 0x0C;
pub const R_MVOLR: usize = 0x1C;
pub const R_EVOLL: usize = 0x2C;
pub const R_EVOLR: usize = 0x3C;
pub const R_KON: usize = 0x4C;
pub const R_KOFF: usize = 0x5C;
pub const R_FLG: usize = 0x6C;
pub const R_ENDX: usize = 0x7C;
pub const R_EFB: usize = 0x0D;
pub const R_PMON: usize = 0x2D;
pub const R_NON: usize = 0x3D;
pub const R_EON: usize = 0x4D;
pub const R_DIR: usize = 0x5D;
pub const R_ESA: usize = 0x6D;
pub const R_EDL: usize = 0x7D;
pub const R_FIR: usize = 0x0F; // 8 coefficients at 0x0F, 0x1F ... 0x7F

// Voice register offsets
pub const V_VOLL: usize = 0x00;
pub const V_VOLR: usize = 0x01;
pub const V_PITCHL: usize = 0x02;
pub const V_PITCHH: usize = 0x03;
pub const V_SRCN: usize = 0x04;
pub const V_ADSR0: usize = 0x05;
pub const V_ADSR1: usize = 0x06;
pub const V_GAIN: usize = 0x07;
pub const V_ENVX: usize = 0x08;
pub const V_OUTX: usize = 0x09;

const ECHO_HIST_SIZE: usize = 8;
const BRR_BUF_SIZE: usize = 12;
const BRR_BLOCK_SIZE: i32 = 9;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum EnvMode {
    Release = 0,
    Attack = 1,
    Decay = 2,
    Sustain = 3,
}

#[derive(Clone)]
struct Voice {
    buf: [i32; BRR_BUF_SIZE * 2], // decoded samples (twice the size to simplify wrap handling)
    buf_pos: usize,               // place in buffer where next samples will be decoded
    interp_pos: i32,              // relative fractional position in sample (0x1000 = 1.0)
    brr_addr: i32,                // address of current BRR block
    brr_offset: i32,              // current decoding offset in BRR block
    kon_delay: i32,               // KON delay/current setup phase
    env_mode: EnvMode,
    env: i32,        // current envelope level
    hidden_env: i32, // used by GAIN mode 7, very obscure quirk
    volume: [i32; 2],
    enabled: i32, // -1 if enabled, 0 if muted
}

impl Voice {
    const fn new() -> Self {
        Self {
            buf: [0; BRR_BUF_SIZE * 2],
            buf_pos: 0,
            interp_pos: 0,
            brr_addr: 0,
            brr_offset: 0,
            kon_delay: 0,
            env_mode: EnvMode::Release,
            env: 0,
            hidden_env: 0,
            volume: [0; 2],
            enabled: 0,
        }
    }

    // Mirrors C++ Spc_Dsp::load()'s memset over state_t: every emulation
    // field of voice_t is zeroed (volume/enabled are then recomputed by
    // mute_voices, brr_offset/buf_pos set by load()).
    fn zero(&mut self) {
        self.buf = [0; BRR_BUF_SIZE * 2];
        self.buf_pos = 0;
        self.interp_pos = 0;
        self.brr_addr = 0;
        self.brr_offset = 0;
        self.kon_delay = 0;
        self.env_mode = EnvMode::Release;
        self.env = 0;
        self.hidden_env = 0;
        self.volume = [0; 2];
        self.enabled = 0;
    }
}

// Interleved gauss table (to improve cache coherency)
// interleved_gauss [i] = gauss [(i & 1) * 256 + 255 - (i >> 1 & 0xFF)]
static INTERLEVED_GAUSS: [i16; 512] = [
    370, 1305, 366, 1305, 362, 1304, 358, 1304, 354, 1304, 351, 1304, 347, 1304, 343, 1303, 339,
    1303, 336, 1303, 332, 1302, 328, 1302, 325, 1301, 321, 1300, 318, 1300, 314, 1299, 311, 1298,
    307, 1297, 304, 1297, 300, 1296, 297, 1295, 293, 1294, 290, 1293, 286, 1292, 283, 1291, 280,
    1290, 276, 1288, 273, 1287, 270, 1286, 267, 1284, 263, 1283, 260, 1282, 257, 1280, 254, 1279,
    251, 1277, 248, 1275, 245, 1274, 242, 1272, 239, 1270, 236, 1269, 233, 1267, 230, 1265, 227,
    1263, 224, 1261, 221, 1259, 218, 1257, 215, 1255, 212, 1253, 210, 1251, 207, 1248, 204, 1246,
    201, 1244, 199, 1241, 196, 1239, 193, 1237, 191, 1234, 188, 1232, 186, 1229, 183, 1227, 180,
    1224, 178, 1221, 175, 1219, 173, 1216, 171, 1213, 168, 1210, 166, 1207, 163, 1205, 161, 1202,
    159, 1199, 156, 1196, 154, 1193, 152, 1190, 150, 1186, 147, 1183, 145, 1180, 143, 1177, 141,
    1174, 139, 1170, 137, 1167, 134, 1164, 132, 1160, 130, 1157, 128, 1153, 126, 1150, 124, 1146,
    122, 1143, 120, 1139, 118, 1136, 117, 1132, 115, 1128, 113, 1125, 111, 1121, 109, 1117, 107,
    1113, 106, 1109, 104, 1106, 102, 1102, 100, 1098, 99, 1094, 97, 1090, 95, 1086, 94, 1082, 92,
    1078, 90, 1074, 89, 1070, 87, 1066, 86, 1061, 84, 1057, 83, 1053, 81, 1049, 80, 1045, 78, 1040,
    77, 1036, 76, 1032, 74, 1027, 73, 1023, 71, 1019, 70, 1014, 69, 1010, 67, 1005, 66, 1001, 65,
    997, 64, 992, 62, 988, 61, 983, 60, 978, 59, 974, 58, 969, 56, 965, 55, 960, 54, 955, 53, 951,
    52, 946, 51, 941, 50, 937, 49, 932, 48, 927, 47, 923, 46, 918, 45, 913, 44, 908, 43, 904, 42,
    899, 41, 894, 40, 889, 39, 884, 38, 880, 37, 875, 36, 870, 36, 865, 35, 860, 34, 855, 33, 851,
    32, 846, 32, 841, 31, 836, 30, 831, 29, 826, 29, 821, 28, 816, 27, 811, 27, 806, 26, 802, 25,
    797, 24, 792, 24, 787, 23, 782, 23, 777, 22, 772, 21, 767, 21, 762, 20, 757, 20, 752, 19, 747,
    19, 742, 18, 737, 17, 732, 17, 728, 16, 723, 16, 718, 15, 713, 15, 708, 15, 703, 14, 698, 14,
    693, 13, 688, 13, 683, 12, 678, 12, 674, 11, 669, 11, 664, 11, 659, 10, 654, 10, 649, 10, 644,
    9, 640, 9, 635, 9, 630, 8, 625, 8, 620, 8, 615, 7, 611, 7, 606, 7, 601, 6, 596, 6, 592, 6, 587,
    6, 582, 5, 577, 5, 573, 5, 568, 5, 563, 4, 559, 4, 554, 4, 550, 4, 545, 4, 540, 3, 536, 3, 531,
    3, 527, 3, 522, 3, 517, 2, 513, 2, 508, 2, 504, 2, 499, 2, 495, 2, 491, 2, 486, 1, 482, 1, 477,
    1, 473, 1, 469, 1, 464, 1, 460, 1, 456, 1, 451, 1, 447, 1, 443, 1, 439, 0, 434, 0, 430, 0, 426,
    0, 422, 0, 418, 0, 414, 0, 410, 0, 405, 0, 401, 0, 397, 0, 393, 0, 389, 0, 385, 0, 381, 0, 378,
    0, 374,
];

/// Counters
const fn rate(rate: u32, div: u32) -> u32 {
    if rate >= div {
        rate / div * 8 - 1
    } else {
        rate.wrapping_sub(1)
    }
}

#[rustfmt::skip]
static COUNTER_MASK: [u32; 32] = [
    rate(   2, 2), rate(2048, 4), rate(1536, 3),
    rate(1280, 5), rate(1024, 4), rate( 768, 3),
    rate( 640, 5), rate( 512, 4), rate( 384, 3),
    rate( 320, 5), rate( 256, 4), rate( 192, 3),
    rate( 160, 5), rate( 128, 4), rate(  96, 3),
    rate(  80, 5), rate(  64, 4), rate(  48, 3),
    rate(  40, 5), rate(  32, 4), rate(  24, 3),
    rate(  20, 5), rate(  16, 4), rate(  12, 3),
    rate(  10, 5), rate(   8, 4), rate(   6, 3),
    rate(   5, 5), rate(   4, 4), rate(   3, 3),
                   rate(   2, 4),
                   rate(   1, 4),
];

fn run_counter(counters: &mut [u32; 4], i: usize) {
    // C++: int n = m.counters[i]; if (!(n-- & 7)) n -= 6 - i; m.counters[i] = n;
    let old = counters[i];
    let mut n = old.wrapping_sub(1);
    if old & 7 == 0 {
        n = n.wrapping_sub(6 - i as u32);
    }
    counters[i] = n;
}

fn read_counter(counters: &[u32; 4], counter_select: &[usize; 32], rate: usize) -> u32 {
    counters[counter_select[rate]] & COUNTER_MASK[rate]
}

static INITIAL_REGS: [u8; REGISTER_COUNT] = [
    0x45, 0x8B, 0x5A, 0x9A, 0xE4, 0x82, 0x1B, 0x78, 0x00, 0x00, 0xAA, 0x96, 0x89, 0x0E, 0xE0, 0x80,
    0x2A, 0x49, 0x3D, 0xBA, 0x14, 0xA0, 0xAC, 0xC5, 0x00, 0x00, 0x51, 0xBB, 0x9C, 0x4E, 0x7B, 0xFF,
    0xF4, 0xFD, 0x57, 0x32, 0x37, 0xD9, 0x42, 0x22, 0x00, 0x00, 0x5B, 0x3C, 0x9F, 0x1B, 0x87, 0x9A,
    0x6F, 0x27, 0xAF, 0x7B, 0xE5, 0x68, 0x0A, 0xD9, 0x00, 0x00, 0x9A, 0xC5, 0x9C, 0x4E, 0x7B, 0xFF,
    0xEA, 0x21, 0x78, 0x4F, 0xDD, 0xED, 0x24, 0x14, 0x00, 0x00, 0x77, 0xB1, 0xD1, 0x36, 0xC1, 0x67,
    0x52, 0x57, 0x46, 0x3D, 0x59, 0xF4, 0x87, 0xA4, 0x00, 0x00, 0x7E, 0x44, 0x9C, 0x4E, 0x7B, 0xFF,
    0x75, 0xF5, 0x06, 0x97, 0x10, 0xC3, 0x24, 0xBB, 0x00, 0x00, 0x7B, 0x7A, 0xE0, 0x60, 0x12, 0x0F,
    0xF7, 0x74, 0x1C, 0xE5, 0x39, 0x3D, 0x73, 0xC1, 0x00, 0x00, 0x7A, 0xB3, 0xFF, 0x4E, 0x7B, 0xFF,
];

// 0: >>1  1: <<0  2: <<1 ... 12: <<11  13-15: >>4 <<11
#[rustfmt::skip]
static BRR_SHIFTS: [u8; 16 * 2] = [
    13, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 16, 16, 16,
     0,  0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 11, 11, 11,
];

#[inline]
fn clamp16(io: &mut i32) {
    if *io as i16 as i32 != *io {
        *io = (*io >> 31) ^ 0x7FFF;
    }
}

pub struct SpcDsp {
    // Mirror of C++ state_t (emulation fields, in order)
    regs: [u8; REGISTER_COUNT],
    // Echo history keeps most recent 8 samples (twice the size to simplify wrap handling)
    echo_hist: [[i32; 2]; ECHO_HIST_SIZE * 2],
    echo_hist_pos: usize, // 0 to 7
    every_other_sample: i32,
    kon: i32, // KON value when last checked
    noise: i32,
    echo_offset: i32, // offset from ESA in echo buffer
    echo_length: i32, // number of bytes that echo_offset will stop at
    phase: i32,       // next clock cycle to run (0-31)
    counters: [u32; 4],
    new_kon: i32,
    t_koff: i32,
    voices: [Voice; VOICE_COUNT],
    counter_select: [usize; 32],
    // Non-emulation state
    mute_mask: i32,
    surround_threshold: i32,
    echo_enable: i32,
    // Output. C++ keeps out/out_end/out_begin pointers and WRITE_SAMPLES
    // switches out into m.extra on overflow. Modeled here as: out_ptr null
    // means "writing to extra at extra_pos" (also the set_output(None) state,
    // where out_begin is null too), so no self-referential pointer survives a
    // move of this struct.
    out_begin: *mut i16,
    out_ptr: *mut i16,
    out_end: *mut i16,
    extra_pos: usize,
    extra: [i16; EXTRA_SIZE],
}

impl Default for SpcDsp {
    fn default() -> Self {
        Self::new()
    }
}

impl SpcDsp {
    pub fn new() -> Self {
        Self {
            regs: [0; REGISTER_COUNT],
            echo_hist: [[0; 2]; ECHO_HIST_SIZE * 2],
            echo_hist_pos: 0,
            every_other_sample: 0,
            kon: 0,
            noise: 0,
            echo_offset: 0,
            echo_length: 0,
            phase: 0,
            counters: [0; 4],
            new_kon: 0,
            t_koff: 0,
            voices: [const { Voice::new() }; VOICE_COUNT],
            counter_select: [0; 32],
            mute_mask: 0,
            surround_threshold: 0,
            echo_enable: 0,
            out_begin: std::ptr::null_mut(),
            out_ptr: std::ptr::null_mut(),
            out_end: std::ptr::null_mut(),
            extra_pos: 0,
            extra: [0; EXTRA_SIZE],
        }
    }

    pub fn init(&mut self) {
        self.mute_voices(0);
        self.disable_surround(false);
        self.disable_echo(false);
        self.set_output(None);
        self.reset();
    }

    /// Sets destination for output samples. `None` means samples are
    /// discarded into the internal extra buffer.
    pub fn set_output(&mut self, out: Option<&mut [i16]>) {
        if let Some(buf) = out {
            debug_assert!(buf.len() & 1 == 0); // must be even
            let len = buf.len();
            self.out_begin = buf.as_mut_ptr();
            self.out_ptr = buf.as_mut_ptr();
            self.out_end = unsafe { buf.as_mut_ptr().add(len) };
        } else {
            self.out_begin = std::ptr::null_mut();
            self.out_ptr = std::ptr::null_mut();
            self.out_end = std::ptr::null_mut();
            self.extra_pos = 0;
        }
    }

    /// Number of i16 samples written to the output buffer since set_output.
    pub fn sample_count(&self) -> usize {
        if !self.out_ptr.is_null() {
            (self.out_ptr as usize - self.out_begin as usize) / std::mem::size_of::<i16>()
        } else if !self.out_begin.is_null() {
            // Main buffer filled; overflow went to extra
            (self.out_end as usize - self.out_begin as usize) / std::mem::size_of::<i16>()
                + self.extra_pos
        } else {
            self.extra_pos
        }
    }

    /// Current output write position (C++ out_pos()); points into `extra`
    /// once the main buffer has filled (or when there is no main buffer).
    pub fn out_pos_ptr(&self) -> usize {
        if !self.out_ptr.is_null() {
            self.out_ptr as usize
        } else {
            unsafe { self.extra.as_ptr().add(self.extra_pos) as usize }
        }
    }

    pub fn extra(&mut self) -> &mut [i16] {
        &mut self.extra
    }

    pub fn regs(&self) -> &[u8; REGISTER_COUNT] {
        &self.regs
    }

    pub fn voice_env(&self, v: usize) -> i32 {
        self.voices[v].env
    }

    pub fn voice_kon_delay(&self, v: usize) -> i32 {
        self.voices[v].kon_delay
    }

    pub fn voice_brr_addr(&self, v: usize) -> i32 {
        self.voices[v].brr_addr
    }

    pub fn voice_brr_offset(&self, v: usize) -> i32 {
        self.voices[v].brr_offset
    }

    pub fn voice_interp_pos(&self, v: usize) -> i32 {
        self.voices[v].interp_pos
    }

    pub fn voice_buf_pos(&self, v: usize) -> usize {
        self.voices[v].buf_pos
    }

    pub fn voice_env_mode(&self, v: usize) -> u8 {
        self.voices[v].env_mode as u8
    }

    pub fn voice_hidden_env(&self, v: usize) -> i32 {
        self.voices[v].hidden_env
    }

    pub fn kon(&self) -> i32 {
        self.kon
    }

    pub fn new_kon(&self) -> i32 {
        self.new_kon
    }

    pub fn t_koff(&self) -> i32 {
        self.t_koff
    }

    pub fn noise(&self) -> i32 {
        self.noise
    }

    pub fn every_other_sample(&self) -> i32 {
        self.every_other_sample
    }

    pub fn echo_offset(&self) -> i32 {
        self.echo_offset
    }

    pub fn phase(&self) -> i32 {
        self.phase
    }

    pub fn counter(&self, i: usize) -> u32 {
        self.counters[i]
    }

    pub fn read(&self, addr: usize) -> u8 {
        debug_assert!(addr < REGISTER_COUNT);
        self.regs[addr]
    }

    pub fn write(&mut self, addr: usize, data: u8) {
        debug_assert!(addr < REGISTER_COUNT);
        self.regs[addr] = data;
        let low = addr & 0x0F;
        if low < 0x2 {
            // voice volumes
            self.update_voice_vol(low ^ addr);
        } else if low == 0xC {
            if addr == R_KON {
                self.new_kon = data as i32;
            }
            if addr == R_ENDX {
                // always cleared, regardless of data written
                self.regs[R_ENDX] = 0;
            }
        }
    }

    fn update_voice_vol(&mut self, addr: usize) {
        let mut l = self.regs[addr + V_VOLL] as i8 as i32;
        let mut r = self.regs[addr + V_VOLR] as i8 as i32;

        if l * r < self.surround_threshold {
            // signs differ, so negate those that are negative
            l ^= l >> 7;
            r ^= r >> 7;
        }

        let v = &mut self.voices[addr >> 4];
        let enabled = v.enabled;
        v.volume[0] = l & enabled;
        v.volume[1] = r & enabled;
    }

    pub fn mute_voices(&mut self, mask: i32) {
        self.mute_mask = mask;
        for i in 0..VOICE_COUNT {
            self.voices[i].enabled = ((mask >> i) & 1) - 1;
            self.update_voice_vol(i * 0x10);
        }
    }

    pub fn disable_surround(&mut self, disable: bool) {
        self.surround_threshold = if disable { 0 } else { -0x4000 };
    }

    pub fn disable_echo(&mut self, disable: bool) {
        self.echo_enable = (!disable) as i32;
    }

    fn init_counter(&mut self) {
        // counters start out with this synchronization
        self.counters[0] = 1;
        self.counters[1] = 0;
        self.counters[2] = 0u32.wrapping_sub(0x20);
        self.counters[3] = 0x0B;

        let mut n: usize = 2;
        for select in &mut self.counter_select[1..] {
            *select = n;
            n -= 1;
            if n == 0 {
                n = 3;
            }
        }
        self.counter_select[0] = 0;
        self.counter_select[30] = 2;
    }

    fn soft_reset_common(&mut self) {
        self.noise = 0x4000;
        self.echo_hist_pos = 0;
        self.every_other_sample = 1;
        self.echo_offset = 0;
        self.phase = 0;

        self.init_counter();
    }

    pub fn soft_reset(&mut self) {
        self.regs[R_FLG] = 0xE0;
        self.soft_reset_common();
    }

    pub fn load(&mut self, regs: &[u8; REGISTER_COUNT]) {
        self.regs.copy_from_slice(regs);

        // C++: memset(&m.regs[register_count], 0, offsetof(state_t, ram) - register_count);
        // i.e. zero every emulation field between regs and the non-emulation state.
        self.echo_hist = [[0; 2]; ECHO_HIST_SIZE * 2];
        self.echo_hist_pos = 0;
        self.every_other_sample = 0;
        self.kon = 0;
        self.noise = 0;
        self.echo_offset = 0;
        self.echo_length = 0;
        self.phase = 0;
        self.counters = [0; 4];
        self.new_kon = 0;
        self.t_koff = 0;
        for v in self.voices.iter_mut() {
            v.zero();
        }
        self.counter_select = [0; 32];

        // Internal state
        for v in self.voices.iter_mut() {
            v.brr_offset = 1;
            v.buf_pos = 0;
        }
        self.new_kon = self.regs[R_KON] as i32;

        self.mute_voices(self.mute_mask);
        self.soft_reset_common();
    }

    pub fn reset(&mut self) {
        self.load(&INITIAL_REGS);
    }

    /// Runs DSP for specified number of clocks (~1024000 per second). Every
    /// 32 clocks a pair of samples is generated. `ram` is the 64K shared RAM.
    pub fn run(&mut self, clock_count: i32, ram: &mut [u8; 0x10000]) {
        let new_phase = self.phase + clock_count;
        let mut count = new_phase >> 5;
        self.phase = new_phase & 31;
        if count == 0 {
            return;
        }

        let dir = self.regs[R_DIR] as usize * 0x100;
        let slow_gaussian = (self.regs[R_PMON] >> 1) as i32 | self.regs[R_NON] as i32;
        let noise_rate = (self.regs[R_FLG] & 0x1F) as usize;

        // Global volume
        let mut mvoll = self.regs[R_MVOLL] as i8 as i32;
        let mut mvolr = self.regs[R_MVOLR] as i8 as i32;
        let mut evoll = self.regs[R_EVOLL] as i8 as i32;
        let mut evolr = self.regs[R_EVOLR] as i8 as i32;

        if self.echo_enable == 0 {
            mvoll = 127;
            mvolr = 127;
            evoll = 0;
            evolr = 0;
        }

        if mvoll * mvolr < self.surround_threshold {
            mvoll = -mvoll; // eliminate surround
        }

        // Disjoint borrows of self's fields for the hot loop
        let Self {
            regs,
            echo_hist,
            echo_hist_pos,
            every_other_sample,
            kon,
            noise,
            echo_offset,
            echo_length,
            counters,
            new_kon,
            t_koff,
            voices,
            counter_select,
            out_ptr,
            out_end,
            extra_pos,
            extra,
            ..
        } = self;

        loop {
            // KON/KOFF reading
            *every_other_sample ^= 1;
            if *every_other_sample != 0 {
                *new_kon &= !*kon;
                *kon = *new_kon;
                *t_koff = regs[R_KOFF] as i32;
            }

            run_counter(counters, 1);
            run_counter(counters, 2);
            run_counter(counters, 3);

            // Noise
            if read_counter(counters, counter_select, noise_rate) == 0 {
                let feedback = (*noise << 13) ^ (*noise << 14);
                *noise = (feedback & 0x4000) ^ (*noise >> 1);
            }

            // Voices
            let mut pmon_input: i32 = 0;
            let mut main_out_l: i32 = 0;
            let mut main_out_r: i32 = 0;
            let mut echo_out_l: i32 = 0;
            let mut echo_out_r: i32 = 0;

            for (v_idx, v) in voices.iter_mut().enumerate() {
                let vbit = 1i32 << v_idx;
                let v_regs = v_idx * 0x10;

                let mut brr_header = ram[(v.brr_addr & 0xFFFF) as usize] as i32;
                let mut kon_delay = v.kon_delay;

                // Pitch
                let mut pitch = (regs[v_regs + V_PITCHL] as i32
                    | ((regs[v_regs + V_PITCHH] as i32) << 8))
                    & 0x3FFF;
                if regs[R_PMON] as i32 & vbit != 0 {
                    pitch += ((pmon_input >> 5) * pitch) >> 10;
                }

                // KON phases
                kon_delay -= 1;
                if kon_delay >= 0 {
                    v.kon_delay = kon_delay;

                    // Get ready to start BRR decoding on next sample
                    if kon_delay == 4 {
                        let a = dir + regs[v_regs + V_SRCN] as usize * 4;
                        v.brr_addr = ram[a & 0xFFFF] as i32 | ((ram[(a + 1) & 0xFFFF] as i32) << 8);
                        v.brr_offset = 1;
                        v.buf_pos = 0;
                        brr_header = 0; // header is ignored on this sample
                    }

                    // Envelope is never run during KON
                    v.env = 0;
                    v.hidden_env = 0;

                    // Disable BRR decoding until last three samples
                    v.interp_pos = if kon_delay & 3 != 0 { 0x4000 } else { 0 };

                    // Pitch is never added during KON
                    pitch = 0;
                }

                let mut env = v.env;

                // Gaussian interpolation
                {
                    let mut output = 0;
                    regs[v_regs + V_ENVX] = (env >> 4) as u8;
                    if env != 0 {
                        // Make pointers into gaussian based on fractional position between samples
                        let offset = ((v.interp_pos as u32) >> 3 & 0x1FE) as usize;
                        let fwd = &INTERLEVED_GAUSS[offset..offset + 2];
                        let rev = &INTERLEVED_GAUSS[510 - offset..512 - offset]; // mirror left half

                        let in_pos = v.buf_pos + ((v.interp_pos as u32) >> 12) as usize;
                        let inb = &v.buf[in_pos..in_pos + 4];

                        if slow_gaussian & vbit == 0 {
                            // 99%
                            // Faster approximation when exact sample value isn't necessary for pitch mod
                            output = (fwd[0] as i32 * inb[0]
                                + fwd[1] as i32 * inb[1]
                                + rev[1] as i32 * inb[2]
                                + rev[0] as i32 * inb[3])
                                >> 11;
                            output = (output * env) >> 11;
                        } else {
                            output = (*noise * 2) as i16 as i32;
                            if regs[R_NON] as i32 & vbit == 0 {
                                output = (fwd[0] as i32 * inb[0]) >> 11;
                                output += (fwd[1] as i32 * inb[1]) >> 11;
                                output += (rev[1] as i32 * inb[2]) >> 11;
                                output = output as i16 as i32;
                                output += (rev[0] as i32 * inb[3]) >> 11;

                                clamp16(&mut output);
                                output &= !1;
                            }
                            output = (output * env) >> 11 & !1;
                        }

                        // Output
                        let l = output * v.volume[0];
                        let r = output * v.volume[1];

                        main_out_l += l;
                        main_out_r += r;

                        if regs[R_EON] as i32 & vbit != 0 {
                            echo_out_l += l;
                            echo_out_r += r;
                        }
                    }

                    pmon_input = output;
                    regs[v_regs + V_OUTX] = (output >> 8) as u8;
                }

                // Soft reset or end of sample
                if regs[R_FLG] & 0x80 != 0 || (brr_header & 3) == 1 {
                    v.env_mode = EnvMode::Release;
                    env = 0;
                }

                if *every_other_sample != 0 {
                    // KOFF
                    if *t_koff & vbit != 0 {
                        v.env_mode = EnvMode::Release;
                    }

                    // KON
                    if *kon & vbit != 0 {
                        v.kon_delay = 5;
                        v.env_mode = EnvMode::Attack;
                        regs[R_ENDX] &= !(vbit as u8);
                    }
                }

                'skip_brr: {
                    // Envelope
                    if v.kon_delay == 0 {
                        'exit_env: {
                            if v.env_mode == EnvMode::Release {
                                // 97%
                                env -= 0x8;
                                v.env = env;
                                if env <= 0 {
                                    v.env = 0;
                                    break 'skip_brr; // no BRR decoding for you!
                                }
                            } else {
                                // 3%
                                let rate;
                                let adsr0 = regs[v_regs + V_ADSR0] as i32;
                                let mut env_data = regs[v_regs + V_ADSR1] as i32;
                                if adsr0 >= 0x80 {
                                    // 97% ADSR
                                    if v.env_mode as i32 > EnvMode::Decay as i32 {
                                        // 89%
                                        env -= 1;
                                        env -= env >> 8;
                                        rate = env_data & 0x1F;

                                        // optimized handling
                                        v.hidden_env = env;
                                        if read_counter(counters, counter_select, rate as usize)
                                            != 0
                                        {
                                            break 'exit_env;
                                        }
                                        v.env = env;
                                        break 'exit_env;
                                    } else if v.env_mode == EnvMode::Decay {
                                        env -= 1;
                                        env -= env >> 8;
                                        rate = (adsr0 >> 3 & 0x0E) + 0x10;
                                    } else {
                                        // env_attack
                                        rate = (adsr0 & 0x0F) * 2 + 1;
                                        env += if rate < 31 { 0x20 } else { 0x400 };
                                    }
                                } else {
                                    // GAIN
                                    env_data = regs[v_regs + V_GAIN] as i32;
                                    let mode = env_data >> 5;
                                    if mode < 4 {
                                        // direct
                                        env = env_data * 0x10;
                                        rate = 31;
                                    } else {
                                        rate = env_data & 0x1F;
                                        if mode == 4 {
                                            // 4: linear decrease
                                            env -= 0x20;
                                        } else if mode < 6 {
                                            // 5: exponential decrease
                                            env -= 1;
                                            env -= env >> 8;
                                        } else {
                                            // 6,7: linear increase
                                            env += 0x20;
                                            if mode > 6 && v.hidden_env as u32 >= 0x600 {
                                                env += 0x8 - 0x20; // 7: two-slope linear increase
                                            }
                                        }
                                    }
                                }

                                // Sustain level
                                if (env >> 8) == (env_data >> 5) && v.env_mode == EnvMode::Decay {
                                    v.env_mode = EnvMode::Sustain;
                                }

                                v.hidden_env = env;

                                // unsigned cast because linear decrease going negative also triggers this
                                if env as u32 > 0x7FF {
                                    env = if env < 0 { 0 } else { 0x7FF };
                                    if v.env_mode == EnvMode::Attack {
                                        v.env_mode = EnvMode::Decay;
                                    }
                                }

                                if read_counter(counters, counter_select, rate as usize) == 0 {
                                    v.env = env; // nothing else is controlled by the counter
                                }
                            }
                        } // exit_env:
                    }

                    {
                        // Apply pitch
                        let old_pos = v.interp_pos;
                        let mut interp_pos = (old_pos & 0x3FFF) + pitch;
                        if interp_pos > 0x7FFF {
                            interp_pos = 0x7FFF;
                        }
                        v.interp_pos = interp_pos;

                        // BRR decode if necessary
                        if old_pos >= 0x4000 {
                            // Arrange the four input nybbles in 0xABCD order for easy decoding
                            let mut nybbles = ram[((v.brr_addr + v.brr_offset) & 0xFFFF) as usize]
                                as i32
                                * 0x100
                                + ram[((v.brr_addr + v.brr_offset + 1) & 0xFFFF) as usize] as i32;

                            // Advance read position
                            let mut brr_offset = v.brr_offset;
                            brr_offset += 2;
                            if brr_offset >= BRR_BLOCK_SIZE {
                                // Next BRR block
                                let mut brr_addr = (v.brr_addr + BRR_BLOCK_SIZE) & 0xFFFF;
                                debug_assert!(brr_offset == BRR_BLOCK_SIZE);
                                if brr_header & 1 != 0 {
                                    let a = dir + regs[v_regs + V_SRCN] as usize * 4 + 2;
                                    brr_addr = ram[a & 0xFFFF] as i32
                                        | ((ram[(a + 1) & 0xFFFF] as i32) << 8);
                                    if v.kon_delay == 0 {
                                        regs[R_ENDX] |= vbit as u8;
                                    }
                                }
                                v.brr_addr = brr_addr;
                                brr_offset = 1;
                            }
                            v.brr_offset = brr_offset;

                            // Decode
                            let scale = (brr_header >> 4) as usize;
                            let right_shift = BRR_SHIFTS[scale] as i32;
                            let left_shift = BRR_SHIFTS[scale + 16] as u32;

                            // Decode four samples
                            let mut pos = v.buf_pos;
                            for _ in 0..4 {
                                // Extract upper nybble and scale appropriately. Matches C++:
                                // int16_t(uint16_t((int16_t) nybbles >> right_shift) << left_shift)
                                let mut s = (((((nybbles as i16 as i32) >> right_shift) as u16)
                                    << left_shift)
                                    as i16) as i32;

                                // Apply IIR filter (8 is the most commonly used)
                                let filter = brr_header & 0x0C;
                                let p1 = v.buf[pos + BRR_BUF_SIZE - 1];
                                let p2 = v.buf[pos + BRR_BUF_SIZE - 2] >> 1;
                                if filter >= 8 {
                                    s += p1;
                                    s -= p2;
                                    if filter == 8 {
                                        // s += p1 * 0.953125 - p2 * 0.46875
                                        s += p2 >> 4;
                                        s += (p1 * -3) >> 6;
                                    } else {
                                        // s += p1 * 0.8984375 - p2 * 0.40625
                                        s += (p1 * -13) >> 7;
                                        s += (p2 * 3) >> 4;
                                    }
                                } else if filter != 0 {
                                    // s += p1 * 0.46875
                                    s += p1 >> 1;
                                    s += (-p1) >> 5;
                                }

                                // Adjust and write sample
                                clamp16(&mut s);
                                s = (s.wrapping_mul(2) as i16) as i32;
                                // second copy simplifies wrap-around
                                v.buf[pos + BRR_BUF_SIZE] = s;
                                v.buf[pos] = s;

                                pos += 1;
                                nybbles <<= 4;
                            }

                            if pos >= BRR_BUF_SIZE {
                                pos = 0;
                            }
                            v.buf_pos = pos;
                        }
                    }
                } // skip_brr:
            }

            // Echo position
            let mut echo_off = *echo_offset;
            let echo_ptr = (regs[R_ESA] as usize * 0x100 + echo_off as usize) & 0xFFFF;
            if echo_off == 0 {
                *echo_length = (regs[R_EDL] as i32 & 0x0F) * 0x800;
            }
            echo_off += 4;
            if echo_off >= *echo_length {
                echo_off = 0;
            }
            *echo_offset = echo_off;

            // FIR
            let mut echo_in_l =
                i16::from_le_bytes([ram[echo_ptr], ram[(echo_ptr + 1) & 0xFFFF]]) as i32;
            let mut echo_in_r =
                i16::from_le_bytes([ram[(echo_ptr + 2) & 0xFFFF], ram[(echo_ptr + 3) & 0xFFFF]])
                    as i32;

            let mut hist_pos = *echo_hist_pos + 1;
            if hist_pos >= ECHO_HIST_SIZE {
                hist_pos = 0;
            }
            *echo_hist_pos = hist_pos;

            echo_hist[hist_pos][0] = echo_in_l;
            echo_hist[hist_pos][1] = echo_in_r;
            echo_hist[hist_pos + ECHO_HIST_SIZE][0] = echo_in_l;
            echo_hist[hist_pos + ECHO_HIST_SIZE][1] = echo_in_r;

            echo_in_l *= regs[R_FIR + 7 * 0x10] as i8 as i32;
            echo_in_r *= regs[R_FIR + 7 * 0x10] as i8 as i32;

            for i in 0..7 {
                echo_in_l += echo_hist[hist_pos + i + 1][0] * (regs[R_FIR + i * 0x10] as i8 as i32);
                echo_in_r += echo_hist[hist_pos + i + 1][1] * (regs[R_FIR + i * 0x10] as i8 as i32);
            }

            // Echo out
            if regs[R_FLG] & 0x20 == 0 {
                let mut l = (echo_out_l >> 7) + ((echo_in_l * (regs[R_EFB] as i8 as i32)) >> 14);
                let mut r = (echo_out_r >> 7) + ((echo_in_r * (regs[R_EFB] as i8 as i32)) >> 14);

                clamp16(&mut l);
                clamp16(&mut r);

                let lb = (l as i16).to_le_bytes();
                let rb = (r as i16).to_le_bytes();
                ram[echo_ptr] = lb[0];
                ram[(echo_ptr + 1) & 0xFFFF] = lb[1];
                ram[(echo_ptr + 2) & 0xFFFF] = rb[0];
                ram[(echo_ptr + 3) & 0xFFFF] = rb[1];
            }

            // Sound out
            let mut l = (main_out_l * mvoll + echo_in_l * evoll) >> 14;
            let mut r = (main_out_r * mvolr + echo_in_r * evolr) >> 14;

            clamp16(&mut l);
            clamp16(&mut r);

            if regs[R_FLG] & 0x40 != 0 {
                l = 0;
                r = 0;
            }

            // WRITE_SAMPLES: write pair, switching into extra on overflow
            if !out_ptr.is_null() {
                unsafe {
                    **out_ptr = l as i16;
                    *out_ptr.add(1) = r as i16;
                    *out_ptr = out_ptr.add(2);
                }
                if *out_ptr >= *out_end {
                    debug_assert!(*out_ptr == *out_end);
                    *out_ptr = std::ptr::null_mut();
                    *extra_pos = 0;
                }
            } else {
                extra[*extra_pos] = l as i16;
                extra[*extra_pos + 1] = r as i16;
                *extra_pos += 2;
                if *extra_pos >= EXTRA_SIZE {
                    *extra_pos = 0;
                }
            }

            count -= 1;
            if count == 0 {
                break;
            }
        }
    }
}
