// SPDX-License-Identifier: GPL-2.0-or-later
// YM2612 by Jarek Burczynski and Tatsuyuki Satoh, with Genesis Plus GX changes by Eke-Eke.
// Adapted in Rust from Game Music Emu's Ym2612_MAME.cpp on November 27, 2026.
// Distributed without warranty. See COPYING.
// The register, table and operator names follow the upstream synthesis core.
#![allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    dead_code,
    unused_mut,
    unused_assignments,
    unsafe_op_in_unsafe_fn,
    clippy::all
)]
pub type UINT8 = u8;
pub type INT16 = i16;
pub type UINT32 = u32;
pub type INT32 = i32;
pub type UINT64 = u64;
pub type stream_sample_t = INT16;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct _ssg_callbacks {
    pub set_clock: Option<unsafe fn(*mut ::core::ffi::c_void, i32) -> ()>,
    pub write: Option<unsafe fn(*mut ::core::ffi::c_void, i32, i32) -> ()>,
    pub read: Option<unsafe fn(*mut ::core::ffi::c_void) -> i32>,
    pub reset: Option<unsafe fn(*mut ::core::ffi::c_void) -> ()>,
}
pub type ssg_callbacks = _ssg_callbacks;
pub type FMSAMPLE = stream_sample_t;
pub type FM_TIMERHANDLER = Option<unsafe fn(*mut ::core::ffi::c_void, i32, i32, i32) -> ()>;
pub type FM_IRQHANDLER = Option<unsafe fn(*mut ::core::ffi::c_void, i32) -> ()>;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct YM2612 {
    pub REGS: [UINT8; 512],
    pub OPN: FM_OPN,
    pub CH: [FM_CH; 6],
    pub addr_A1: UINT8,
    pub dacen: UINT8,
    pub dac_test: UINT8,
    pub dacout: INT32,
    pub MuteDAC: UINT8,
    pub WaveOutMode: UINT8,
    pub WaveL: INT32,
    pub WaveR: INT32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct FM_CH {
    pub SLOT: [FM_SLOT; 4],
    pub ALGO: UINT8,
    pub FB: UINT8,
    pub op1_out: [INT32; 2],
    pub connect1: *mut INT32,
    pub connect3: *mut INT32,
    pub connect2: *mut INT32,
    pub connect4: *mut INT32,
    pub mem_connect: *mut INT32,
    pub mem_value: INT32,
    pub pms: INT32,
    pub ams: UINT8,
    pub fc: UINT32,
    pub kcode: UINT8,
    pub block_fnum: UINT32,
    pub Muted: UINT8,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct FM_SLOT {
    pub DT: *mut INT32,
    pub KSR: UINT8,
    pub ar: UINT32,
    pub d1r: UINT32,
    pub d2r: UINT32,
    pub rr: UINT32,
    pub ksr: UINT8,
    pub mul: UINT32,
    pub phase: UINT32,
    pub Incr: INT32,
    pub state: UINT8,
    pub tl: UINT32,
    pub volume: INT32,
    pub sl: UINT32,
    pub vol_out: UINT32,
    pub eg_sh_ar: UINT8,
    pub eg_sel_ar: UINT8,
    pub eg_sh_d1r: UINT8,
    pub eg_sel_d1r: UINT8,
    pub eg_sh_d2r: UINT8,
    pub eg_sel_d2r: UINT8,
    pub eg_sh_rr: UINT8,
    pub eg_sel_rr: UINT8,
    pub ssg: UINT8,
    pub ssgn: UINT8,
    pub key: UINT8,
    pub AMmask: UINT32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct FM_OPN {
    pub type_0: UINT8,
    pub ST: FM_ST,
    pub SL3: FM_3SLOT,
    pub P_CH: *mut FM_CH,
    pub pan: [u32; 12],
    pub eg_cnt: UINT32,
    pub eg_timer: UINT32,
    pub eg_timer_add: UINT32,
    pub eg_timer_overflow: UINT32,
    pub fn_table: [UINT32; 4096],
    pub fn_max: UINT32,
    pub lfo_cnt: UINT8,
    pub lfo_timer: UINT32,
    pub lfo_timer_add: UINT32,
    pub lfo_timer_overflow: UINT32,
    pub LFO_AM: UINT32,
    pub LFO_PM: UINT32,
    pub m2: INT32,
    pub c1: INT32,
    pub c2: INT32,
    pub mem: INT32,
    pub out_fm: [INT32; 6],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct FM_3SLOT {
    pub fc: [UINT32; 3],
    pub fn_h: UINT8,
    pub kcode: [UINT8; 3],
    pub block_fnum: [UINT32; 3],
    pub key_csm: UINT8,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct FM_ST {
    pub param: *mut ::core::ffi::c_void,
    pub freqbase: f64,
    pub timer_prescaler: i32,
    pub irq: UINT8,
    pub irqmask: UINT8,
    pub clock: UINT32,
    pub rate: UINT32,
    pub rateratio: INT32,
    pub framecnt: INT32,
    pub cur_sample: [FMSAMPLE; 2],
    pub prev_sample: [FMSAMPLE; 2],
    pub address: UINT8,
    pub status: UINT8,
    pub mode: UINT32,
    pub fn_h: UINT8,
    pub prescaler_sel: UINT8,
    pub TA: INT32,
    pub TAC: INT32,
    pub TB: UINT8,
    pub TBC: INT32,
    pub dt_tab: [[INT32; 32]; 8],
    pub timer_handler: FM_TIMERHANDLER,
    pub IRQ_Handler: FM_IRQHANDLER,
    pub SSG: *const ssg_callbacks,
}
pub const NULL: *mut ::core::ffi::c_void = ::core::ptr::null_mut::<::core::ffi::c_void>();
pub const M_PI: f64 = 3.14159265358979323846f64;
const DUMMYBUF: *mut stream_sample_t =
    ::core::ptr::null::<stream_sample_t>() as *mut stream_sample_t;
pub const RSM_FRAC: i32 = 10_i32;
pub const TYPE_LFOPAN: i32 = 0x2_i32;
pub const TYPE_6CH: i32 = 0x4_i32;
pub const TYPE_DAC: i32 = 0x8_i32;
pub const TYPE_YM2612: i32 = TYPE_DAC | TYPE_LFOPAN | TYPE_6CH;
pub const FREQ_SH: i32 = 16_i32;
pub const EG_SH: i32 = 16_i32;
pub const LFO_SH: i32 = 24_i32;
pub const FREQ_MASK: i32 = ((1_i32) << FREQ_SH) - 1_i32;
pub const ENV_BITS: i32 = 10_i32;
pub const ENV_LEN: i32 = (1_i32) << ENV_BITS;
pub const ENV_STEP: f64 = 128.0f64 / ENV_LEN as f64;
pub const MAX_ATT_INDEX: i32 = ENV_LEN - 1_i32;
pub const MIN_ATT_INDEX: i32 = 0_i32;
pub const EG_ATT: i32 = 4_i32;
pub const EG_DEC: i32 = 3_i32;
pub const EG_SUS: i32 = 2_i32;
pub const EG_REL: i32 = 1_i32;
pub const EG_OFF: i32 = 0_i32;
pub const SIN_BITS: i32 = 10_i32;
pub const SIN_LEN: i32 = (1_i32) << SIN_BITS;
pub const SIN_MASK: i32 = SIN_LEN - 1_i32;
pub const TL_RES_LEN: i32 = 256_i32;
pub const TL_TAB_LEN: i32 = 13_i32 * 2_i32 * TL_RES_LEN;
static mut tl_tab: [i32; 6656] = [0; 6656];
pub const ENV_QUIET: i32 = TL_TAB_LEN >> 3_i32;
static mut sin_tab: [u32; 1024] = [0; 1024];
const sl_table: [UINT32; 16] = [
    (0_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (1_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (2_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (3_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (4_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (5_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (6_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (7_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (8_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (9_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (10_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (11_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (12_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (13_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (14_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
    (31_i32 as f64 * (4.0f64 / ENV_STEP)) as UINT32,
];
pub const RATE_STEPS: i32 = 8_i32;
const eg_inc: [UINT8; 152] = [
    0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 2, 2, 1, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 4, 2, 4, 2, 4, 2, 4, 2, 4, 2, 4, 4, 4, 2, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 8, 4, 4, 4, 8, 4, 8, 4, 8, 4, 8, 4, 8, 4, 8, 8, 8, 4, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 16, 16, 16, 16, 16, 16, 16, 16, 0, 0, 0, 0, 0, 0, 0, 0,
];
const eg_rate_select2612: [UINT8; 128] = [
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (18_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (0_i32 * RATE_STEPS) as UINT8,
    (1_i32 * RATE_STEPS) as UINT8,
    (2_i32 * RATE_STEPS) as UINT8,
    (3_i32 * RATE_STEPS) as UINT8,
    (4_i32 * RATE_STEPS) as UINT8,
    (5_i32 * RATE_STEPS) as UINT8,
    (6_i32 * RATE_STEPS) as UINT8,
    (7_i32 * RATE_STEPS) as UINT8,
    (8_i32 * RATE_STEPS) as UINT8,
    (9_i32 * RATE_STEPS) as UINT8,
    (10_i32 * RATE_STEPS) as UINT8,
    (11_i32 * RATE_STEPS) as UINT8,
    (12_i32 * RATE_STEPS) as UINT8,
    (13_i32 * RATE_STEPS) as UINT8,
    (14_i32 * RATE_STEPS) as UINT8,
    (15_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
    (16_i32 * RATE_STEPS) as UINT8,
];
const eg_rate_shift: [UINT8; 128] = [
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (11_i32 * 1_i32) as UINT8,
    (10_i32 * 1_i32) as UINT8,
    (10_i32 * 1_i32) as UINT8,
    (10_i32 * 1_i32) as UINT8,
    (10_i32 * 1_i32) as UINT8,
    (9_i32 * 1_i32) as UINT8,
    (9_i32 * 1_i32) as UINT8,
    (9_i32 * 1_i32) as UINT8,
    (9_i32 * 1_i32) as UINT8,
    (8_i32 * 1_i32) as UINT8,
    (8_i32 * 1_i32) as UINT8,
    (8_i32 * 1_i32) as UINT8,
    (8_i32 * 1_i32) as UINT8,
    (7_i32 * 1_i32) as UINT8,
    (7_i32 * 1_i32) as UINT8,
    (7_i32 * 1_i32) as UINT8,
    (7_i32 * 1_i32) as UINT8,
    (6_i32 * 1_i32) as UINT8,
    (6_i32 * 1_i32) as UINT8,
    (6_i32 * 1_i32) as UINT8,
    (6_i32 * 1_i32) as UINT8,
    (5_i32 * 1_i32) as UINT8,
    (5_i32 * 1_i32) as UINT8,
    (5_i32 * 1_i32) as UINT8,
    (5_i32 * 1_i32) as UINT8,
    (4_i32 * 1_i32) as UINT8,
    (4_i32 * 1_i32) as UINT8,
    (4_i32 * 1_i32) as UINT8,
    (4_i32 * 1_i32) as UINT8,
    (3_i32 * 1_i32) as UINT8,
    (3_i32 * 1_i32) as UINT8,
    (3_i32 * 1_i32) as UINT8,
    (3_i32 * 1_i32) as UINT8,
    (2_i32 * 1_i32) as UINT8,
    (2_i32 * 1_i32) as UINT8,
    (2_i32 * 1_i32) as UINT8,
    (2_i32 * 1_i32) as UINT8,
    (1_i32 * 1_i32) as UINT8,
    (1_i32 * 1_i32) as UINT8,
    (1_i32 * 1_i32) as UINT8,
    (1_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
    (0_i32 * 1_i32) as UINT8,
];
const dt_tab: [UINT8; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 8, 8, 8, 8,
    1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 8, 8, 9, 10, 11, 12, 13, 14, 16,
    16, 16, 16, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 8, 8, 9, 10, 11, 12, 13, 14, 16,
    17, 19, 20, 22, 22, 22, 22,
];
const opn_fktable: [UINT8; 16] = [0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 3, 3, 3, 3, 3, 3];
const lfo_samples_per_step: [UINT32; 8] = [108, 77, 71, 67, 62, 44, 8, 5];
const lfo_ams_depth_shift: [UINT8; 4] = [8, 3, 1, 0];
const lfo_pm_output: [[UINT8; 8]; 56] = [
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 1, 1, 1],
    [0, 0, 1, 1, 2, 2, 2, 3],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 1],
    [0, 0, 0, 0, 1, 1, 1, 1],
    [0, 0, 1, 1, 2, 2, 2, 3],
    [0, 0, 2, 3, 4, 4, 5, 6],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 1, 1],
    [0, 0, 0, 0, 1, 1, 1, 1],
    [0, 0, 0, 1, 1, 1, 1, 2],
    [0, 0, 1, 1, 2, 2, 2, 3],
    [0, 0, 2, 3, 4, 4, 5, 6],
    [0, 0, 4, 6, 8, 8, 0xa_i32 as UINT8, 0xc_i32 as UINT8],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 1, 1, 1],
    [0, 0, 0, 1, 1, 1, 2, 2],
    [0, 0, 1, 1, 2, 2, 3, 3],
    [0, 0, 1, 2, 2, 2, 3, 4],
    [0, 0, 2, 3, 4, 4, 5, 6],
    [0, 0, 4, 6, 8, 8, 0xa_i32 as UINT8, 0xc_i32 as UINT8],
    [
        0,
        0,
        8,
        0xc_i32 as UINT8,
        0x10_i32 as UINT8,
        0x10_i32 as UINT8,
        0x14_i32 as UINT8,
        0x18_i32 as UINT8,
    ],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 2, 2, 2, 2],
    [0, 0, 0, 2, 2, 2, 4, 4],
    [0, 0, 2, 2, 4, 4, 6, 6],
    [0, 0, 2, 4, 4, 4, 6, 8],
    [0, 0, 4, 6, 8, 8, 0xa_i32 as UINT8, 0xc_i32 as UINT8],
    [
        0,
        0,
        8,
        0xc_i32 as UINT8,
        0x10_i32 as UINT8,
        0x10_i32 as UINT8,
        0x14_i32 as UINT8,
        0x18_i32 as UINT8,
    ],
    [
        0,
        0,
        0x10_i32 as UINT8,
        0x18_i32 as UINT8,
        0x20_i32 as UINT8,
        0x20_i32 as UINT8,
        0x28_i32 as UINT8,
        0x30_i32 as UINT8,
    ],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 4, 4, 4, 4],
    [0, 0, 0, 4, 4, 4, 8, 8],
    [0, 0, 4, 4, 8, 8, 0xc_i32 as UINT8, 0xc_i32 as UINT8],
    [0, 0, 4, 8, 8, 8, 0xc_i32 as UINT8, 0x10_i32 as UINT8],
    [
        0,
        0,
        8,
        0xc_i32 as UINT8,
        0x10_i32 as UINT8,
        0x10_i32 as UINT8,
        0x14_i32 as UINT8,
        0x18_i32 as UINT8,
    ],
    [
        0,
        0,
        0x10_i32 as UINT8,
        0x18_i32 as UINT8,
        0x20_i32 as UINT8,
        0x20_i32 as UINT8,
        0x28_i32 as UINT8,
        0x30_i32 as UINT8,
    ],
    [
        0,
        0,
        0x20_i32 as UINT8,
        0x30_i32 as UINT8,
        0x40_i32 as UINT8,
        0x40_i32 as UINT8,
        0x50_i32 as UINT8,
        0x60_i32 as UINT8,
    ],
];
static mut lfo_pm_table: [INT32; 32768] = [0; 32768];
pub const SLOT1: i32 = 0_i32;
pub const SLOT2: i32 = 2_i32;
pub const SLOT3: i32 = 1_i32;
pub const SLOT4: i32 = 3_i32;
const PseudoSt: UINT8 = 0 as UINT8;
#[inline]
unsafe fn FM_STATUS_SET(mut ST: *mut FM_ST, mut flag: i32) {
    (*ST).status = ((*ST).status as i32 | flag) as UINT8;
    if (*ST).irq == 0 && (*ST).status as i32 & (*ST).irqmask as i32 != 0 {
        (*ST).irq = 1 as UINT8;
        if (*ST).IRQ_Handler.is_some() {
            (*ST).IRQ_Handler.expect("non-null function pointer")((*ST).param, 1_i32);
        }
    }
}
#[inline]
unsafe fn FM_STATUS_RESET(mut ST: *mut FM_ST, mut flag: i32) {
    (*ST).status = ((*ST).status as i32 & !flag) as UINT8;
    if (*ST).irq as i32 != 0 && (*ST).status as i32 & (*ST).irqmask as i32 == 0 {
        (*ST).irq = 0 as UINT8;
        if (*ST).IRQ_Handler.is_some() {
            (*ST).IRQ_Handler.expect("non-null function pointer")((*ST).param, 0_i32);
        }
    }
}
#[inline]
unsafe fn FM_IRQMASK_SET(mut ST: *mut FM_ST, mut flag: i32) {
    (*ST).irqmask = flag as UINT8;
    FM_STATUS_SET(ST, 0_i32);
    FM_STATUS_RESET(ST, 0_i32);
}
#[inline]
unsafe fn FM_KEYON(mut OPN: *mut FM_OPN, mut CH: *mut FM_CH, mut s: i32) {
    let mut SLOT: *mut FM_SLOT =
        (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(s as isize) as *mut FM_SLOT;
    if (*SLOT).key == 0 && ((*OPN).SL3.key_csm == 0 || CH == (*OPN).P_CH.add(3) as *mut FM_CH) {
        (*SLOT).phase = 0 as UINT32;
        (*SLOT).ssgn = 0 as UINT8;
        if (*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) < 94 as UINT32 {
            (*SLOT).state = (if (*SLOT).volume <= MIN_ATT_INDEX {
                if (*SLOT).sl == MIN_ATT_INDEX as UINT32 {
                    EG_SUS
                } else {
                    EG_DEC
                }
            } else {
                EG_ATT
            }) as UINT8;
        } else {
            (*SLOT).volume = MIN_ATT_INDEX as INT32;
            (*SLOT).state = (if (*SLOT).sl == MIN_ATT_INDEX as UINT32 {
                EG_SUS
            } else {
                EG_DEC
            }) as UINT8;
        }
        if (*SLOT).ssg as i32 & 0x8_i32 != 0
            && (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0
        {
            (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32
                & MAX_ATT_INDEX as UINT32)
                .wrapping_add((*SLOT).tl);
        } else {
            (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
        }
    }
    (*SLOT).key = 1 as UINT8;
}
#[inline]
unsafe fn FM_KEYOFF(mut OPN: *mut FM_OPN, mut CH: *mut FM_CH, mut s: i32) {
    let mut SLOT: *mut FM_SLOT =
        (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(s as isize) as *mut FM_SLOT;
    if (*SLOT).key as i32 != 0
        && ((*OPN).SL3.key_csm == 0 || CH == (*OPN).P_CH.add(3) as *mut FM_CH)
    {
        if (*SLOT).state as i32 > EG_REL {
            (*SLOT).state = EG_REL as UINT8;
            if (*SLOT).ssg as i32 & 0x8_i32 != 0 {
                if (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0 {
                    (*SLOT).volume = 0x200 as INT32 - (*SLOT).volume;
                }
                if (*SLOT).volume >= 0x200_i32 {
                    (*SLOT).volume = MAX_ATT_INDEX as INT32;
                    (*SLOT).state = EG_OFF as UINT8;
                }
                (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
            }
        }
    }
    (*SLOT).key = 0 as UINT8;
}
#[inline]
unsafe fn FM_KEYON_CSM(mut OPN: *mut FM_OPN, mut CH: *mut FM_CH, mut s: i32) {
    let mut SLOT: *mut FM_SLOT =
        (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(s as isize) as *mut FM_SLOT;
    if (*SLOT).key == 0 && (*OPN).SL3.key_csm == 0 {
        (*SLOT).phase = 0 as UINT32;
        (*SLOT).ssgn = 0 as UINT8;
        if (*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) < 94 as UINT32 {
            (*SLOT).state = (if (*SLOT).volume <= MIN_ATT_INDEX {
                if (*SLOT).sl == MIN_ATT_INDEX as UINT32 {
                    EG_SUS
                } else {
                    EG_DEC
                }
            } else {
                EG_ATT
            }) as UINT8;
        } else {
            (*SLOT).volume = MIN_ATT_INDEX as INT32;
            (*SLOT).state = (if (*SLOT).sl == MIN_ATT_INDEX as UINT32 {
                EG_SUS
            } else {
                EG_DEC
            }) as UINT8;
        }
        if (*SLOT).ssg as i32 & 0x8_i32 != 0
            && (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0
        {
            (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32
                & MAX_ATT_INDEX as UINT32)
                .wrapping_add((*SLOT).tl);
        } else {
            (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
        }
    }
}
#[inline]
unsafe fn FM_KEYOFF_CSM(mut CH: *mut FM_CH, mut s: i32) {
    let mut SLOT: *mut FM_SLOT =
        (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(s as isize) as *mut FM_SLOT;
    if (*SLOT).key == 0 {
        if (*SLOT).state as i32 > EG_REL {
            (*SLOT).state = EG_REL as UINT8;
            if (*SLOT).ssg as i32 & 0x8_i32 != 0 {
                if (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0 {
                    (*SLOT).volume = 0x200 as INT32 - (*SLOT).volume;
                }
                if (*SLOT).volume >= 0x200_i32 {
                    (*SLOT).volume = MAX_ATT_INDEX as INT32;
                    (*SLOT).state = EG_OFF as UINT8;
                }
                (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
            }
        }
    }
}
#[inline]
unsafe fn set_timers(
    mut OPN: *mut FM_OPN,
    mut ST: *mut FM_ST,
    mut n: *mut ::core::ffi::c_void,
    mut v: i32,
) {
    if ((*OPN).ST.mode ^ v as UINT32) & 0xc0 as UINT32 != 0 {
        (*(*OPN).P_CH.add(2)).SLOT[SLOT1 as usize].Incr = -(1_i32) as INT32;
        if v & 0xc0_i32 != 0x80_i32 && (*OPN).SL3.key_csm as i32 != 0 {
            FM_KEYOFF_CSM((*OPN).P_CH.add(2) as *mut FM_CH, SLOT1);
            FM_KEYOFF_CSM((*OPN).P_CH.add(2) as *mut FM_CH, SLOT2);
            FM_KEYOFF_CSM((*OPN).P_CH.add(2) as *mut FM_CH, SLOT3);
            FM_KEYOFF_CSM((*OPN).P_CH.add(2) as *mut FM_CH, SLOT4);
            (*OPN).SL3.key_csm = 0 as UINT8;
        }
    }
    if v & 0x20_i32 != 0 {
        FM_STATUS_RESET(ST, 0x2_i32);
    }
    if v & 0x10_i32 != 0 {
        FM_STATUS_RESET(ST, 0x1_i32);
    }
    if v & 2_i32 != 0 && (*ST).mode & 2 as UINT32 == 0 {
        (*ST).TBC = ((256_i32 - (*ST).TB as i32) << 4_i32) as INT32;
        if (*ST).timer_handler.is_some() {
            (*ST).timer_handler.expect("non-null function pointer")(
                n,
                1_i32,
                (*ST).TBC as i32 * (*ST).timer_prescaler,
                (*ST).clock as i32,
            );
        }
    }
    if v & 1_i32 != 0 && (*ST).mode & 1 as UINT32 == 0 {
        (*ST).TAC = 1024 as INT32 - (*ST).TA;
        if (*ST).timer_handler.is_some() {
            (*ST).timer_handler.expect("non-null function pointer")(
                n,
                0_i32,
                (*ST).TAC as i32 * (*ST).timer_prescaler,
                (*ST).clock as i32,
            );
        }
        (*ST).TAC *= 4096_i32;
    }
    (*ST).mode = v as UINT32;
}
#[inline]
unsafe fn TimerAOver(mut ST: *mut FM_ST) {
    if (*ST).mode & 0x4 as UINT32 != 0 {
        FM_STATUS_SET(ST, 0x1_i32);
    }
    (*ST).TAC = 1024 as INT32 - (*ST).TA;
    if (*ST).timer_handler.is_some() {
        (*ST).timer_handler.expect("non-null function pointer")(
            (*ST).param,
            0_i32,
            (*ST).TAC as i32 * (*ST).timer_prescaler,
            (*ST).clock as i32,
        );
    }
    (*ST).TAC *= 4096_i32;
}
#[inline]
unsafe fn setup_connection(mut OPN: *mut FM_OPN, mut CH: *mut FM_CH, mut ch: i32) {
    let mut carrier: *mut INT32 =
        (&raw mut (*OPN).out_fm as *mut INT32).offset(ch as isize) as *mut INT32;
    let mut om1: *mut *mut INT32 = &raw mut (*CH).connect1;
    let mut om2: *mut *mut INT32 = &raw mut (*CH).connect3;
    let mut oc1: *mut *mut INT32 = &raw mut (*CH).connect2;
    let mut memc: *mut *mut INT32 = &raw mut (*CH).mem_connect;
    match (*CH).ALGO as i32 {
        0 => {
            *om1 = &raw mut (*OPN).c1;
            *oc1 = &raw mut (*OPN).mem;
            *om2 = &raw mut (*OPN).c2;
            *memc = &raw mut (*OPN).m2;
        }
        1 => {
            *om1 = &raw mut (*OPN).mem;
            *oc1 = &raw mut (*OPN).mem;
            *om2 = &raw mut (*OPN).c2;
            *memc = &raw mut (*OPN).m2;
        }
        2 => {
            *om1 = &raw mut (*OPN).c2;
            *oc1 = &raw mut (*OPN).mem;
            *om2 = &raw mut (*OPN).c2;
            *memc = &raw mut (*OPN).m2;
        }
        3 => {
            *om1 = &raw mut (*OPN).c1;
            *oc1 = &raw mut (*OPN).mem;
            *om2 = &raw mut (*OPN).c2;
            *memc = &raw mut (*OPN).c2;
        }
        4 => {
            *om1 = &raw mut (*OPN).c1;
            *oc1 = carrier;
            *om2 = &raw mut (*OPN).c2;
            *memc = &raw mut (*OPN).mem;
        }
        5 => {
            *om1 = ::core::ptr::null_mut::<INT32>();
            *oc1 = carrier;
            *om2 = carrier;
            *memc = &raw mut (*OPN).m2;
        }
        6 => {
            *om1 = &raw mut (*OPN).c1;
            *oc1 = carrier;
            *om2 = carrier;
            *memc = &raw mut (*OPN).mem;
        }
        7 => {
            *om1 = carrier;
            *oc1 = carrier;
            *om2 = carrier;
            *memc = &raw mut (*OPN).mem;
        }
        _ => {}
    }
    (*CH).connect4 = carrier;
}
#[inline]
unsafe fn set_det_mul(mut ST: *mut FM_ST, mut CH: *mut FM_CH, mut SLOT: *mut FM_SLOT, mut v: i32) {
    (*SLOT).mul = (if v & 0xf_i32 != 0 {
        (v & 0xf_i32) * 2_i32
    } else {
        1_i32
    }) as UINT32;
    (*SLOT).DT = &raw mut *(&raw mut (*ST).dt_tab as *mut [INT32; 32])
        .offset((v >> 4_i32 & 7_i32) as isize) as *mut INT32;
    (*CH).SLOT[SLOT1 as usize].Incr = -(1_i32) as INT32;
}
#[inline]
unsafe fn set_tl(_CH: *mut FM_CH, mut SLOT: *mut FM_SLOT, mut v: i32) {
    (*SLOT).tl = ((v & 0x7f_i32) << ENV_BITS - 7_i32) as UINT32;
    if (*SLOT).ssg as i32 & 0x8_i32 != 0
        && (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0
        && (*SLOT).state as i32 > EG_REL
    {
        (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32 & MAX_ATT_INDEX as UINT32)
            .wrapping_add((*SLOT).tl);
    } else {
        (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
    };
}
#[inline]
unsafe fn set_ar_ksr(_type_0: UINT8, mut CH: *mut FM_CH, mut SLOT: *mut FM_SLOT, mut v: i32) {
    let mut old_KSR = (*SLOT).KSR;
    (*SLOT).ar = (if v & 0x1f_i32 != 0 {
        32_i32 + ((v & 0x1f_i32) << 1_i32)
    } else {
        0_i32
    }) as UINT32;
    (*SLOT).KSR = (3_i32 - (v >> 6_i32)) as UINT8;
    if (*SLOT).KSR as i32 != old_KSR as i32 {
        (*CH).SLOT[SLOT1 as usize].Incr = -(1_i32) as INT32;
    }
    if (*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) < 94 as UINT32 {
        (*SLOT).eg_sh_ar = eg_rate_shift[(*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) as usize];
        (*SLOT).eg_sel_ar =
            eg_rate_select2612[(*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) as usize];
    } else {
        (*SLOT).eg_sh_ar = 0 as UINT8;
        (*SLOT).eg_sel_ar = (18_i32 * RATE_STEPS) as UINT8;
    };
}
#[inline]
unsafe fn set_dr(_type_0: UINT8, mut SLOT: *mut FM_SLOT, mut v: i32) {
    (*SLOT).d1r = (if v & 0x1f_i32 != 0 {
        32_i32 + ((v & 0x1f_i32) << 1_i32)
    } else {
        0_i32
    }) as UINT32;
    (*SLOT).eg_sh_d1r = eg_rate_shift[(*SLOT).d1r.wrapping_add((*SLOT).ksr as UINT32) as usize];
    (*SLOT).eg_sel_d1r =
        eg_rate_select2612[(*SLOT).d1r.wrapping_add((*SLOT).ksr as UINT32) as usize];
}
#[inline]
unsafe fn set_sr(_type_0: UINT8, mut SLOT: *mut FM_SLOT, mut v: i32) {
    (*SLOT).d2r = (if v & 0x1f_i32 != 0 {
        32_i32 + ((v & 0x1f_i32) << 1_i32)
    } else {
        0_i32
    }) as UINT32;
    (*SLOT).eg_sh_d2r = eg_rate_shift[(*SLOT).d2r.wrapping_add((*SLOT).ksr as UINT32) as usize];
    (*SLOT).eg_sel_d2r =
        eg_rate_select2612[(*SLOT).d2r.wrapping_add((*SLOT).ksr as UINT32) as usize];
}
#[inline]
unsafe fn set_sl_rr(_type_0: UINT8, mut SLOT: *mut FM_SLOT, mut v: i32) {
    (*SLOT).sl = sl_table[(v >> 4_i32) as usize];
    if (*SLOT).state as i32 == EG_DEC && (*SLOT).volume >= (*SLOT).sl as INT32 {
        (*SLOT).state = EG_SUS as UINT8;
    }
    (*SLOT).rr = (34_i32 + ((v & 0xf_i32) << 2_i32)) as UINT32;
    (*SLOT).eg_sh_rr = eg_rate_shift[(*SLOT).rr.wrapping_add((*SLOT).ksr as UINT32) as usize];
    (*SLOT).eg_sel_rr = eg_rate_select2612[(*SLOT).rr.wrapping_add((*SLOT).ksr as UINT32) as usize];
}
#[inline]
unsafe fn advance_lfo(mut OPN: *mut FM_OPN) {
    if (*OPN).lfo_timer_overflow != 0 {
        (*OPN).lfo_timer = (*OPN).lfo_timer.wrapping_add((*OPN).lfo_timer_add);
        while (*OPN).lfo_timer >= (*OPN).lfo_timer_overflow {
            (*OPN).lfo_timer = (*OPN).lfo_timer.wrapping_sub((*OPN).lfo_timer_overflow);
            (*OPN).lfo_cnt = ((*OPN).lfo_cnt as i32 + 1_i32 & 127_i32) as UINT8;
            if ((*OPN).lfo_cnt as i32) < 64_i32 {
                (*OPN).LFO_AM = (((*OPN).lfo_cnt as i32 ^ 63_i32) as UINT32) << 1_i32;
            } else {
                (*OPN).LFO_AM = (((*OPN).lfo_cnt as i32 & 63_i32) as UINT32) << 1_i32;
            }
            (*OPN).LFO_PM = ((*OPN).lfo_cnt as i32 >> 2_i32) as UINT32;
        }
    }
}
#[inline]
unsafe fn advance_eg_channel(mut OPN: *mut FM_OPN, mut SLOT: *mut FM_SLOT) {
    let mut i = 4_u32;
    loop {
        match (*SLOT).state as i32 {
            EG_ATT => {
                if (*OPN).eg_cnt & (((1_i32) << (*SLOT).eg_sh_ar as i32) - 1_i32) as UINT32 == 0 {
                    (*SLOT).volume += !((*SLOT).volume as i32)
                        * eg_inc[((*SLOT).eg_sel_ar as UINT32)
                            .wrapping_add((*OPN).eg_cnt >> (*SLOT).eg_sh_ar as i32 & 7 as UINT32)
                            as usize] as i32
                        >> 4_i32;
                    if (*SLOT).volume <= MIN_ATT_INDEX {
                        (*SLOT).volume = MIN_ATT_INDEX as INT32;
                        (*SLOT).state = (if (*SLOT).sl == MIN_ATT_INDEX as UINT32 {
                            EG_SUS
                        } else {
                            EG_DEC
                        }) as UINT8;
                    }
                    if (*SLOT).ssg as i32 & 0x8_i32 != 0
                        && (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0
                    {
                        (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32
                            & MAX_ATT_INDEX as UINT32)
                            .wrapping_add((*SLOT).tl);
                    } else {
                        (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
                    }
                }
            }
            EG_DEC => {
                if (*OPN).eg_cnt & (((1_i32) << (*SLOT).eg_sh_d1r as i32) - 1_i32) as UINT32 == 0 {
                    if (*SLOT).ssg as i32 & 0x8_i32 != 0 {
                        if (*SLOT).volume < 0x200_i32 {
                            (*SLOT).volume += 4_i32
                                * eg_inc[((*SLOT).eg_sel_d1r as UINT32).wrapping_add(
                                    (*OPN).eg_cnt >> (*SLOT).eg_sh_d1r as i32 & 7 as UINT32,
                                ) as usize] as i32;
                            if (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0 {
                                (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32
                                    & MAX_ATT_INDEX as UINT32)
                                    .wrapping_add((*SLOT).tl);
                            } else {
                                (*SLOT).vol_out =
                                    ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
                            }
                        }
                    } else {
                        (*SLOT).volume += eg_inc[((*SLOT).eg_sel_d1r as UINT32)
                            .wrapping_add((*OPN).eg_cnt >> (*SLOT).eg_sh_d1r as i32 & 7 as UINT32)
                            as usize] as i32;
                        (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
                    }
                    if (*SLOT).volume >= (*SLOT).sl as INT32 {
                        (*SLOT).state = EG_SUS as UINT8;
                    }
                }
            }
            EG_SUS => {
                if (*OPN).eg_cnt & (((1_i32) << (*SLOT).eg_sh_d2r as i32) - 1_i32) as UINT32 == 0 {
                    if (*SLOT).ssg as i32 & 0x8_i32 != 0 {
                        if (*SLOT).volume < 0x200_i32 {
                            (*SLOT).volume += 4_i32
                                * eg_inc[((*SLOT).eg_sel_d2r as UINT32).wrapping_add(
                                    (*OPN).eg_cnt >> (*SLOT).eg_sh_d2r as i32 & 7 as UINT32,
                                ) as usize] as i32;
                            if (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0 {
                                (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32
                                    & MAX_ATT_INDEX as UINT32)
                                    .wrapping_add((*SLOT).tl);
                            } else {
                                (*SLOT).vol_out =
                                    ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
                            }
                        }
                    } else {
                        (*SLOT).volume += eg_inc[((*SLOT).eg_sel_d2r as UINT32)
                            .wrapping_add((*OPN).eg_cnt >> (*SLOT).eg_sh_d2r as i32 & 7 as UINT32)
                            as usize] as i32;
                        if (*SLOT).volume >= MAX_ATT_INDEX {
                            (*SLOT).volume = MAX_ATT_INDEX as INT32;
                        }
                        (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
                    }
                }
            }
            EG_REL => {
                if (*OPN).eg_cnt & (((1_i32) << (*SLOT).eg_sh_rr as i32) - 1_i32) as UINT32 == 0 {
                    if (*SLOT).ssg as i32 & 0x8_i32 != 0 {
                        if (*SLOT).volume < 0x200_i32 {
                            (*SLOT).volume += 4_i32
                                * eg_inc[((*SLOT).eg_sel_rr as UINT32).wrapping_add(
                                    (*OPN).eg_cnt >> (*SLOT).eg_sh_rr as i32 & 7 as UINT32,
                                ) as usize] as i32;
                        }
                        if (*SLOT).volume >= 0x200_i32 {
                            (*SLOT).volume = MAX_ATT_INDEX as INT32;
                            (*SLOT).state = EG_OFF as UINT8;
                        }
                    } else {
                        (*SLOT).volume += eg_inc[((*SLOT).eg_sel_rr as UINT32)
                            .wrapping_add((*OPN).eg_cnt >> (*SLOT).eg_sh_rr as i32 & 7 as UINT32)
                            as usize] as i32;
                        if (*SLOT).volume >= MAX_ATT_INDEX {
                            (*SLOT).volume = MAX_ATT_INDEX as INT32;
                            (*SLOT).state = EG_OFF as UINT8;
                        }
                    }
                    (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
                }
            }
            _ => {}
        }
        SLOT = SLOT.offset(1);
        i = i.wrapping_sub(1);
        if !(i != 0) {
            break;
        }
    }
}
#[inline]
unsafe fn update_ssg_eg_channel(mut SLOT: *mut FM_SLOT) {
    let mut i = 4_u32;
    loop {
        if (*SLOT).ssg as i32 & 0x8_i32 != 0
            && (*SLOT).volume >= 0x200_i32
            && (*SLOT).state as i32 > EG_REL
        {
            if (*SLOT).ssg as i32 & 0x1_i32 != 0 {
                if (*SLOT).ssg as i32 & 0x2_i32 != 0 {
                    (*SLOT).ssgn = 4 as UINT8;
                }
                if (*SLOT).state as i32 != EG_ATT
                    && (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 == 0
                {
                    (*SLOT).volume = MAX_ATT_INDEX as INT32;
                }
            } else {
                if (*SLOT).ssg as i32 & 0x2_i32 != 0 {
                    (*SLOT).ssgn = ((*SLOT).ssgn as i32 ^ 4_i32) as UINT8;
                } else {
                    (*SLOT).phase = 0 as UINT32;
                }
                if (*SLOT).state as i32 != EG_ATT {
                    if (*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) < 94 as UINT32 {
                        (*SLOT).state = (if (*SLOT).volume <= MIN_ATT_INDEX {
                            if (*SLOT).sl == MIN_ATT_INDEX as UINT32 {
                                EG_SUS
                            } else {
                                EG_DEC
                            }
                        } else {
                            EG_ATT
                        }) as UINT8;
                    } else {
                        (*SLOT).volume = MIN_ATT_INDEX as INT32;
                        (*SLOT).state = (if (*SLOT).sl == MIN_ATT_INDEX as UINT32 {
                            EG_SUS
                        } else {
                            EG_DEC
                        }) as UINT8;
                    }
                }
            }
            if (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0 {
                (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32
                    & MAX_ATT_INDEX as UINT32)
                    .wrapping_add((*SLOT).tl);
            } else {
                (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
            }
        }
        SLOT = SLOT.offset(1);
        i = i.wrapping_sub(1);
        if !(i != 0) {
            break;
        }
    }
}
#[inline]
unsafe fn update_phase_lfo_slot(
    mut OPN: *mut FM_OPN,
    mut SLOT: *mut FM_SLOT,
    mut pms: INT32,
    mut block_fnum: UINT32,
) {
    let mut fnum_lfo = ((block_fnum & 0x7f0 as UINT32) >> 4_i32)
        .wrapping_mul(32 as UINT32)
        .wrapping_mul(8 as UINT32);
    let mut lfo_fn_table_index_offset = lfo_pm_table[fnum_lfo
        .wrapping_add(pms as UINT32)
        .wrapping_add((*OPN).LFO_PM) as usize];
    block_fnum = block_fnum
        .wrapping_mul(2 as UINT32)
        .wrapping_add(lfo_fn_table_index_offset as UINT32);
    if lfo_fn_table_index_offset != 0 {
        let mut blk = ((block_fnum & 0x7000 as UINT32) >> 12_i32) as UINT8;
        let mut fn_0 = block_fnum & 0xfff as UINT32;
        let mut kc = (blk as i32) << 2_i32 | opn_fktable[(fn_0 >> 8_i32) as usize] as i32;
        let mut fc = ((*OPN).fn_table[fn_0 as usize] >> 7_i32 - blk as i32)
            .wrapping_add(*(*SLOT).DT.offset(kc as isize) as UINT32) as i32;
        if fc < 0_i32 {
            fc = (fc as UINT32).wrapping_add((*OPN).fn_max) as i32 as i32;
        }
        (*SLOT).phase = (*SLOT)
            .phase
            .wrapping_add((fc as UINT32).wrapping_mul((*SLOT).mul) >> 1_i32);
    } else {
        (*SLOT).phase = (*SLOT).phase.wrapping_add((*SLOT).Incr as UINT32);
    };
}
#[inline]
unsafe fn update_phase_lfo_channel(mut OPN: *mut FM_OPN, mut CH: *mut FM_CH) {
    let mut block_fnum = (*CH).block_fnum;
    let mut fnum_lfo = ((block_fnum & 0x7f0 as UINT32) >> 4_i32)
        .wrapping_mul(32 as UINT32)
        .wrapping_mul(8 as UINT32);
    let mut lfo_fn_table_index_offset = lfo_pm_table[fnum_lfo
        .wrapping_add((*CH).pms as UINT32)
        .wrapping_add((*OPN).LFO_PM) as usize];
    block_fnum = block_fnum
        .wrapping_mul(2 as UINT32)
        .wrapping_add(lfo_fn_table_index_offset as UINT32);
    if lfo_fn_table_index_offset != 0 {
        let mut blk = ((block_fnum & 0x7000 as UINT32) >> 12_i32) as UINT8;
        let mut fn_0 = block_fnum & 0xfff as UINT32;
        let mut kc = (blk as i32) << 2_i32 | opn_fktable[(fn_0 >> 8_i32) as usize] as i32;
        let mut fc = ((*OPN).fn_table[fn_0 as usize] >> 7_i32 - blk as i32) as i32;
        let mut finc = fc + *(*CH).SLOT[SLOT1 as usize].DT.offset(kc as isize) as i32;
        if finc < 0_i32 {
            finc = (finc as UINT32).wrapping_add((*OPN).fn_max) as i32 as i32;
        }
        (*CH).SLOT[SLOT1 as usize].phase = (*CH).SLOT[SLOT1 as usize]
            .phase
            .wrapping_add((finc as UINT32).wrapping_mul((*CH).SLOT[SLOT1 as usize].mul) >> 1_i32);
        finc = (fc as INT32 + *(*CH).SLOT[SLOT2 as usize].DT.offset(kc as isize)) as i32;
        if finc < 0_i32 {
            finc = (finc as UINT32).wrapping_add((*OPN).fn_max) as i32 as i32;
        }
        (*CH).SLOT[SLOT2 as usize].phase = (*CH).SLOT[SLOT2 as usize]
            .phase
            .wrapping_add((finc as UINT32).wrapping_mul((*CH).SLOT[SLOT2 as usize].mul) >> 1_i32);
        finc = (fc as INT32 + *(*CH).SLOT[SLOT3 as usize].DT.offset(kc as isize)) as i32;
        if finc < 0_i32 {
            finc = (finc as UINT32).wrapping_add((*OPN).fn_max) as i32 as i32;
        }
        (*CH).SLOT[SLOT3 as usize].phase = (*CH).SLOT[SLOT3 as usize]
            .phase
            .wrapping_add((finc as UINT32).wrapping_mul((*CH).SLOT[SLOT3 as usize].mul) >> 1_i32);
        finc = (fc as INT32 + *(*CH).SLOT[SLOT4 as usize].DT.offset(kc as isize)) as i32;
        if finc < 0_i32 {
            finc = (finc as UINT32).wrapping_add((*OPN).fn_max) as i32 as i32;
        }
        (*CH).SLOT[SLOT4 as usize].phase = (*CH).SLOT[SLOT4 as usize]
            .phase
            .wrapping_add((finc as UINT32).wrapping_mul((*CH).SLOT[SLOT4 as usize].mul) >> 1_i32);
    } else {
        (*CH).SLOT[SLOT1 as usize].phase = (*CH).SLOT[SLOT1 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT1 as usize].Incr as UINT32);
        (*CH).SLOT[SLOT2 as usize].phase = (*CH).SLOT[SLOT2 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT2 as usize].Incr as UINT32);
        (*CH).SLOT[SLOT3 as usize].phase = (*CH).SLOT[SLOT3 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT3 as usize].Incr as UINT32);
        (*CH).SLOT[SLOT4 as usize].phase = (*CH).SLOT[SLOT4 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT4 as usize].Incr as UINT32);
    };
}
#[inline]
unsafe fn refresh_fc_eg_slot(
    mut OPN: *mut FM_OPN,
    mut SLOT: *mut FM_SLOT,
    mut fc: i32,
    mut kc: i32,
) {
    let mut ksr = kc >> (*SLOT).KSR as i32;
    fc += *(*SLOT).DT.offset(kc as isize) as i32;
    if fc < 0_i32 {
        fc = (fc as UINT32).wrapping_add((*OPN).fn_max) as i32 as i32;
    }
    (*SLOT).Incr = ((fc as UINT32).wrapping_mul((*SLOT).mul) >> 1_i32) as INT32;
    if (*SLOT).ksr as i32 != ksr {
        (*SLOT).ksr = ksr as UINT8;
        if (*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) < (32_i32 + 62_i32) as UINT32 {
            (*SLOT).eg_sh_ar =
                eg_rate_shift[(*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) as usize];
            (*SLOT).eg_sel_ar =
                eg_rate_select2612[(*SLOT).ar.wrapping_add((*SLOT).ksr as UINT32) as usize];
        } else {
            (*SLOT).eg_sh_ar = 0 as UINT8;
            (*SLOT).eg_sel_ar = (18_i32 * RATE_STEPS) as UINT8;
        }
        (*SLOT).eg_sh_d1r = eg_rate_shift[(*SLOT).d1r.wrapping_add((*SLOT).ksr as UINT32) as usize];
        (*SLOT).eg_sh_d2r = eg_rate_shift[(*SLOT).d2r.wrapping_add((*SLOT).ksr as UINT32) as usize];
        (*SLOT).eg_sh_rr = eg_rate_shift[(*SLOT).rr.wrapping_add((*SLOT).ksr as UINT32) as usize];
        (*SLOT).eg_sel_d1r =
            eg_rate_select2612[(*SLOT).d1r.wrapping_add((*SLOT).ksr as UINT32) as usize];
        (*SLOT).eg_sel_d2r =
            eg_rate_select2612[(*SLOT).d2r.wrapping_add((*SLOT).ksr as UINT32) as usize];
        (*SLOT).eg_sel_rr =
            eg_rate_select2612[(*SLOT).rr.wrapping_add((*SLOT).ksr as UINT32) as usize];
    }
}
#[inline]
unsafe fn refresh_fc_eg_chan(mut OPN: *mut FM_OPN, mut CH: *mut FM_CH) {
    if (*CH).SLOT[SLOT1 as usize].Incr == -(1_i32) {
        let mut fc = (*CH).fc as i32;
        let mut kc = (*CH).kcode as i32;
        refresh_fc_eg_slot(
            OPN,
            (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
            fc,
            kc,
        );
        refresh_fc_eg_slot(
            OPN,
            (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT2 as isize) as *mut FM_SLOT,
            fc,
            kc,
        );
        refresh_fc_eg_slot(
            OPN,
            (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT3 as isize) as *mut FM_SLOT,
            fc,
            kc,
        );
        refresh_fc_eg_slot(
            OPN,
            (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT4 as isize) as *mut FM_SLOT,
            fc,
            kc,
        );
    }
}
#[inline]
unsafe fn op_calc(mut phase: UINT32, mut env: u32, mut pm: i32) -> i32 {
    let mut p: UINT32 = 0;
    p = (env << 3_i32).wrapping_add(
        sin_tab[((phase & !FREQ_MASK as UINT32).wrapping_add((pm << 15_i32) as UINT32) as i32
            >> FREQ_SH
            & SIN_MASK) as usize],
    ) as UINT32;
    if p >= TL_TAB_LEN as UINT32 {
        return 0_i32;
    }
    return tl_tab[p as usize];
}
#[inline]
unsafe fn op_calc1(mut phase: UINT32, mut env: u32, mut pm: i32) -> i32 {
    let mut p = ((env as UINT32) << 3_i32).wrapping_add(
        sin_tab[((phase & !FREQ_MASK as UINT32).wrapping_add(pm as UINT32) as i32 >> FREQ_SH
            & SIN_MASK) as usize],
    );
    if p >= TL_TAB_LEN as UINT32 {
        return 0_i32;
    }
    return tl_tab[p as usize];
}
#[inline]
unsafe fn chan_calc(mut F2612: *mut YM2612, mut OPN: *mut FM_OPN, mut CH: *mut FM_CH) {
    let mut AM = (*OPN).LFO_AM >> (*CH).ams as i32;
    let mut eg_out: u32 = 0;
    if (*CH).Muted != 0 {
        return;
    }
    (*OPN).mem = 0_i32 as INT32;
    (*OPN).c2 = (*OPN).mem;
    (*OPN).c1 = (*OPN).c2;
    (*OPN).m2 = (*OPN).c1;
    *(*CH).mem_connect = (*CH).mem_value;
    eg_out = (*CH).SLOT[0]
        .vol_out
        .wrapping_add(AM & (*CH).SLOT[0].AMmask) as u32;
    let mut out = (*CH).op1_out[0] + (*CH).op1_out[1];
    (*CH).op1_out[0] = (*CH).op1_out[1];
    if (*CH).connect1.is_null() {
        (*OPN).c2 = (*CH).op1_out[0];
        (*OPN).c1 = (*OPN).c2;
        (*OPN).mem = (*OPN).c1;
    } else {
        *(*CH).connect1 += (*CH).op1_out[0];
    }
    (*CH).op1_out[1] = 0_i32 as INT32;
    if eg_out < ENV_QUIET as u32 {
        if (*CH).FB == 0 {
            out = 0_i32 as INT32;
        }
        (*CH).op1_out[1] = op_calc1(
            (*CH).SLOT[SLOT1 as usize].phase,
            eg_out,
            (out as i32) << (*CH).FB as i32,
        ) as INT32;
    }
    eg_out = (*CH).SLOT[1]
        .vol_out
        .wrapping_add(AM & (*CH).SLOT[1].AMmask) as u32;
    if eg_out < ENV_QUIET as u32 {
        *(*CH).connect3 += op_calc((*CH).SLOT[SLOT3 as usize].phase, eg_out, (*OPN).m2 as i32);
    }
    eg_out = (*CH).SLOT[2]
        .vol_out
        .wrapping_add(AM & (*CH).SLOT[2].AMmask) as u32;
    if eg_out < ENV_QUIET as u32 {
        *(*CH).connect2 += op_calc((*CH).SLOT[SLOT2 as usize].phase, eg_out, (*OPN).c1 as i32);
    }
    eg_out = (*CH).SLOT[3]
        .vol_out
        .wrapping_add(AM & (*CH).SLOT[3].AMmask) as u32;
    if eg_out < ENV_QUIET as u32 {
        *(*CH).connect4 += op_calc((*CH).SLOT[SLOT4 as usize].phase, eg_out, (*OPN).c2 as i32);
    }
    (*CH).mem_value = (*OPN).mem;
    if (*CH).pms != 0 {
        if (*OPN).ST.mode & 0xc0 as UINT32 != 0
            && CH == (&raw mut (*F2612).CH as *mut FM_CH).add(2) as *mut FM_CH
        {
            update_phase_lfo_slot(
                OPN,
                (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
                (*CH).pms,
                (*OPN).SL3.block_fnum[1],
            );
            update_phase_lfo_slot(
                OPN,
                (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT2 as isize) as *mut FM_SLOT,
                (*CH).pms,
                (*OPN).SL3.block_fnum[2],
            );
            update_phase_lfo_slot(
                OPN,
                (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT3 as isize) as *mut FM_SLOT,
                (*CH).pms,
                (*OPN).SL3.block_fnum[0],
            );
            update_phase_lfo_slot(
                OPN,
                (&raw mut (*CH).SLOT as *mut FM_SLOT).offset(SLOT4 as isize) as *mut FM_SLOT,
                (*CH).pms,
                (*CH).block_fnum,
            );
        } else {
            update_phase_lfo_channel(OPN, CH);
        }
    } else {
        (*CH).SLOT[SLOT1 as usize].phase = (*CH).SLOT[SLOT1 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT1 as usize].Incr as UINT32);
        (*CH).SLOT[SLOT2 as usize].phase = (*CH).SLOT[SLOT2 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT2 as usize].Incr as UINT32);
        (*CH).SLOT[SLOT3 as usize].phase = (*CH).SLOT[SLOT3 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT3 as usize].Incr as UINT32);
        (*CH).SLOT[SLOT4 as usize].phase = (*CH).SLOT[SLOT4 as usize]
            .phase
            .wrapping_add((*CH).SLOT[SLOT4 as usize].Incr as UINT32);
    };
}
#[inline]
unsafe fn CSMKeyControll(mut OPN: *mut FM_OPN, mut CH: *mut FM_CH) {
    FM_KEYON_CSM(OPN, CH, SLOT1);
    FM_KEYON_CSM(OPN, CH, SLOT2);
    FM_KEYON_CSM(OPN, CH, SLOT3);
    FM_KEYON_CSM(OPN, CH, SLOT4);
    (*OPN).SL3.key_csm = 1 as UINT8;
}
unsafe fn OPNWriteMode(mut OPN: *mut FM_OPN, mut r: i32, mut v: i32) {
    let mut c: UINT8 = 0;
    let mut CH = ::core::ptr::null_mut::<FM_CH>();
    match r {
        34 => {
            if v & 8_i32 != 0 {
                (*OPN).lfo_timer_overflow = lfo_samples_per_step[(v & 7_i32) as usize] << LFO_SH;
            } else {
                (*OPN).lfo_timer_overflow = 0 as UINT32;
                (*OPN).lfo_timer = 0 as UINT32;
                (*OPN).lfo_cnt = 0 as UINT8;
                (*OPN).LFO_PM = 0 as UINT32;
                (*OPN).LFO_AM = 126 as UINT32;
            }
        }
        36 => {
            (*OPN).ST.TA = ((*OPN).ST.TA as i32 & 0x3_i32 | v << 2_i32) as INT32;
        }
        37 => {
            (*OPN).ST.TA = ((*OPN).ST.TA as i32 & 0x3fc_i32 | v & 3_i32) as INT32;
        }
        38 => {
            (*OPN).ST.TB = v as UINT8;
        }
        39 => {
            set_timers(OPN, &raw mut (*OPN).ST, (*OPN).ST.param, v);
        }
        40 => {
            c = (v & 0x3_i32) as UINT8;
            if !(c as i32 == 3_i32) {
                if v & 0x4_i32 != 0 && (*OPN).type_0 as i32 & TYPE_6CH != 0 {
                    c = (c as i32 + 3_i32) as UINT8;
                }
                CH = (*OPN).P_CH;
                CH = CH.offset(c as isize) as *mut FM_CH;
                if v & 0x10_i32 != 0 {
                    FM_KEYON(OPN, CH, SLOT1);
                } else {
                    FM_KEYOFF(OPN, CH, SLOT1);
                }
                if v & 0x20_i32 != 0 {
                    FM_KEYON(OPN, CH, SLOT2);
                } else {
                    FM_KEYOFF(OPN, CH, SLOT2);
                }
                if v & 0x40_i32 != 0 {
                    FM_KEYON(OPN, CH, SLOT3);
                } else {
                    FM_KEYOFF(OPN, CH, SLOT3);
                }
                if v & 0x80_i32 != 0 {
                    FM_KEYON(OPN, CH, SLOT4);
                } else {
                    FM_KEYOFF(OPN, CH, SLOT4);
                }
            }
        }
        33 | _ => {}
    };
}
unsafe fn OPNWriteReg(mut OPN: *mut FM_OPN, mut r: i32, mut v: i32) {
    let mut CH = ::core::ptr::null_mut::<FM_CH>();
    let mut SLOT = ::core::ptr::null_mut::<FM_SLOT>();
    let mut c = (r & 3_i32) as UINT8;
    if c as i32 == 3_i32 {
        return;
    }
    if r >= 0x100_i32 {
        c = (c as i32 + 3_i32) as UINT8;
    }
    CH = (*OPN).P_CH;
    CH = CH.offset(c as isize) as *mut FM_CH;
    SLOT =
        (&raw mut (*CH).SLOT as *mut FM_SLOT).offset((r >> 2_i32 & 3_i32) as isize) as *mut FM_SLOT;
    match r & 0xf0_i32 {
        48 => {
            set_det_mul(&raw mut (*OPN).ST, CH, SLOT, v);
        }
        64 => {
            set_tl(CH, SLOT, v);
        }
        80 => {
            set_ar_ksr((*OPN).type_0, CH, SLOT, v);
        }
        96 => {
            set_dr((*OPN).type_0, SLOT, v);
            if (*OPN).type_0 as i32 & TYPE_LFOPAN != 0 {
                (*SLOT).AMmask = (if v & 0x80_i32 != 0 { !(0_i32) } else { 0_i32 }) as UINT32;
            }
        }
        112 => {
            set_sr((*OPN).type_0, SLOT, v);
        }
        128 => {
            set_sl_rr((*OPN).type_0, SLOT, v);
        }
        144 => {
            (*SLOT).ssg = (v & 0xf_i32) as UINT8;
            if (*SLOT).state as i32 > EG_REL {
                if (*SLOT).ssg as i32 & 0x8_i32 != 0
                    && (*SLOT).ssgn as i32 ^ (*SLOT).ssg as i32 & 0x4_i32 != 0
                {
                    (*SLOT).vol_out = ((0x200 as INT32 - (*SLOT).volume) as UINT32
                        & MAX_ATT_INDEX as UINT32)
                        .wrapping_add((*SLOT).tl);
                } else {
                    (*SLOT).vol_out = ((*SLOT).volume as UINT32).wrapping_add((*SLOT).tl);
                }
            }
        }
        160 => match r >> 2_i32 & 3_i32 {
            0 => {
                let mut fn_0 = ((((*OPN).ST.fn_h as i32 & 7_i32) as UINT32) << 8_i32)
                    .wrapping_add(v as UINT32);
                let mut blk = ((*OPN).ST.fn_h as i32 >> 3_i32) as UINT8;
                (*CH).kcode =
                    ((blk as i32) << 2_i32 | opn_fktable[(fn_0 >> 7_i32) as usize] as i32) as UINT8;
                (*CH).fc =
                    (*OPN).fn_table[fn_0.wrapping_mul(2 as UINT32) as usize] >> 7_i32 - blk as i32;
                (*CH).block_fnum = ((blk as i32) << 11_i32) as UINT32 | fn_0;
                (*CH).SLOT[SLOT1 as usize].Incr = -(1_i32) as INT32;
            }
            1 => {
                (*OPN).ST.fn_h = (v & 0x3f_i32) as UINT8;
            }
            2 => {
                if r < 0x100_i32 {
                    let mut fn_1 = ((((*OPN).SL3.fn_h as i32 & 7_i32) as UINT32) << 8_i32)
                        .wrapping_add(v as UINT32);
                    let mut blk_0 = ((*OPN).SL3.fn_h as i32 >> 3_i32) as UINT8;
                    (*OPN).SL3.kcode[c as usize] = ((blk_0 as i32) << 2_i32
                        | opn_fktable[(fn_1 >> 7_i32) as usize] as i32)
                        as UINT8;
                    (*OPN).SL3.fc[c as usize] = (*OPN).fn_table
                        [fn_1.wrapping_mul(2 as UINT32) as usize]
                        >> 7_i32 - blk_0 as i32;
                    (*OPN).SL3.block_fnum[c as usize] = ((blk_0 as i32) << 11_i32) as UINT32 | fn_1;
                    (*(*OPN).P_CH.add(2)).SLOT[SLOT1 as usize].Incr = -(1_i32) as INT32;
                }
            }
            3 => {
                if r < 0x100_i32 {
                    (*OPN).SL3.fn_h = (v & 0x3f_i32) as UINT8;
                }
            }
            _ => {}
        },
        176 => match r >> 2_i32 & 3_i32 {
            0 => {
                let mut feedback = (v >> 3_i32 & 7_i32) as u8;
                (*CH).ALGO = (v & 7_i32) as UINT8;
                (*CH).FB = (if feedback as i32 != 0 {
                    feedback as i32 + 6_i32
                } else {
                    0_i32
                }) as UINT8;
                setup_connection(OPN, CH, c as i32);
            }
            1 => {
                if (*OPN).type_0 as i32 & TYPE_LFOPAN != 0 {
                    (*CH).pms = ((v & 7_i32) * 32_i32) as INT32;
                    (*CH).ams = lfo_ams_depth_shift[(v >> 4_i32 & 0x3_i32) as usize];
                    (*OPN).pan[(c as i32 * 2_i32) as usize] =
                        (if v & 0x80_i32 != 0 { !(0_i32) } else { 0_i32 }) as u32;
                    (*OPN).pan[(c as i32 * 2_i32 + 1_i32) as usize] =
                        (if v & 0x40_i32 != 0 { !(0_i32) } else { 0_i32 }) as u32;
                }
            }
            _ => {}
        },
        _ => {}
    };
}
unsafe fn init_timetables(mut OPN: *mut FM_OPN, mut freqbase: f64) {
    let mut i: i32 = 0;
    let mut d: i32 = 0;
    let mut rate: f64 = 0.;
    d = 0_i32;
    while d <= 3_i32 {
        i = 0_i32;
        while i <= 31_i32 {
            rate = dt_tab[(d * 32_i32 + i) as usize] as f64
                * freqbase
                * ((1_i32) << FREQ_SH - 10_i32) as f64;
            (*OPN).ST.dt_tab[d as usize][i as usize] = rate as INT32;
            (*OPN).ST.dt_tab[(d + 4_i32) as usize][i as usize] =
                -(*OPN).ST.dt_tab[d as usize][i as usize];
            i += 1;
        }
        d += 1;
    }
    i = 0_i32;
    while i < 4096_i32 {
        (*OPN).fn_table[i as usize] =
            (i as f64 * 32_i32 as f64 * freqbase * ((1_i32) << FREQ_SH - 10_i32) as f64) as UINT32;
        i += 1;
    }
    (*OPN).fn_max =
        (0x20000_i32 as f64 * freqbase * ((1_i32) << FREQ_SH - 10_i32) as f64) as UINT32;
}
unsafe fn OPNSetPres(
    mut OPN: *mut FM_OPN,
    mut pres: i32,
    mut timer_prescaler: i32,
    mut SSGpres: i32,
) {
    (*OPN).ST.freqbase = if (*OPN).ST.rate != 0 {
        (*OPN).ST.clock as f64 / (*OPN).ST.rate as f64 / pres as f64
    } else {
        0_i32 as f64
    };
    (*OPN).eg_timer_add = (((1_i32) << EG_SH) as f64 * (*OPN).ST.freqbase) as UINT32;
    (*OPN).eg_timer_overflow = (3_i32 * ((1_i32) << EG_SH)) as UINT32;
    (*OPN).lfo_timer_add = (((1_i32) << LFO_SH) as f64 * (*OPN).ST.freqbase) as UINT32;
    (*OPN).ST.timer_prescaler = timer_prescaler;
    if SSGpres != 0 {
        Some(
            (*(*OPN).ST.SSG)
                .set_clock
                .expect("non-null function pointer"),
        )
        .expect("non-null function pointer")(
            (*OPN).ST.param,
            (*OPN)
                .ST
                .clock
                .wrapping_mul(2 as UINT32)
                .wrapping_div(SSGpres as UINT32) as i32,
        );
    }
    init_timetables(OPN, (*OPN).ST.freqbase);
}
unsafe fn reset_channels(_ST: *mut FM_ST, mut CH: *mut FM_CH, mut num: i32) {
    let mut c: i32 = 0;
    let mut s: i32 = 0;
    c = 0_i32;
    while c < num {
        (*CH.offset(c as isize)).mem_value = 0_i32 as INT32;
        (*CH.offset(c as isize)).op1_out[0] = 0_i32 as INT32;
        (*CH.offset(c as isize)).op1_out[1] = 0_i32 as INT32;
        (*CH.offset(c as isize)).fc = 0 as UINT32;
        s = 0_i32;
        while s < 4_i32 {
            (*CH.offset(c as isize)).SLOT[s as usize].Incr = -(1_i32) as INT32;
            (*CH.offset(c as isize)).SLOT[s as usize].key = 0 as UINT8;
            (*CH.offset(c as isize)).SLOT[s as usize].phase = 0 as UINT32;
            (*CH.offset(c as isize)).SLOT[s as usize].ssg = 0 as UINT8;
            (*CH.offset(c as isize)).SLOT[s as usize].ssgn = 0 as UINT8;
            (*CH.offset(c as isize)).SLOT[s as usize].state = EG_OFF as UINT8;
            (*CH.offset(c as isize)).SLOT[s as usize].volume = MAX_ATT_INDEX as INT32;
            (*CH.offset(c as isize)).SLOT[s as usize].vol_out = MAX_ATT_INDEX as UINT32;
            s += 1;
        }
        c += 1;
    }
}
pub(super) unsafe fn initialize_tables() {
    let mut i: i32 = 0;
    let mut x: i32 = 0;
    let mut n: i32 = 0;
    let mut o: f64 = 0.;
    let mut m: f64 = 0.;
    x = 0_i32;
    while x < TL_RES_LEN {
        m = ((1_i32) << 16_i32) as f64
            / f64::powf(
                2_i32 as f64,
                (x + 1_i32) as f64 * (ENV_STEP / 4.0f64) / 8.0f64,
            );
        m = f64::floor(m);
        n = m as i32;
        n >>= 4_i32;
        if n & 1_i32 != 0 {
            n = (n >> 1_i32) + 1_i32;
        } else {
            n = n >> 1_i32;
        }
        n <<= 2_i32;
        tl_tab[(x * 2_i32 + 0_i32) as usize] = n;
        tl_tab[(x * 2_i32 + 1_i32) as usize] = -tl_tab[(x * 2_i32 + 0_i32) as usize];
        i = 1_i32;
        while i < 13_i32 {
            tl_tab[(x * 2_i32 + 0_i32 + i * 2_i32 * TL_RES_LEN) as usize] =
                tl_tab[(x * 2_i32 + 0_i32) as usize] >> i;
            tl_tab[(x * 2_i32 + 1_i32 + i * 2_i32 * TL_RES_LEN) as usize] =
                -tl_tab[(x * 2_i32 + 0_i32 + i * 2_i32 * TL_RES_LEN) as usize];
            i += 1;
        }
        x += 1;
    }
    i = 0_i32;
    while i < SIN_LEN {
        m = f64::sin((i * 2_i32 + 1_i32) as f64 * M_PI / SIN_LEN as f64);
        if m > 0.0f64 {
            o = 8_i32 as f64 * f64::ln(1.0f64 / m) / f64::ln(2.0f64);
        } else {
            o = 8_i32 as f64 * f64::ln(-1.0f64 / m) / f64::ln(2.0f64);
        }
        o = o / (ENV_STEP / 4_i32 as f64);
        n = (2.0f64 * o) as i32;
        if n & 1_i32 != 0 {
            n = (n >> 1_i32) + 1_i32;
        } else {
            n = n >> 1_i32;
        }
        sin_tab[i as usize] = (n * 2_i32 + (if m >= 0.0f64 { 0_i32 } else { 1_i32 })) as u32;
        i += 1;
    }
    i = 0_i32;
    while i < 8_i32 {
        let mut fnum: UINT8 = 0;
        fnum = 0 as UINT8;
        while (fnum as i32) < 128_i32 {
            let mut value: UINT8 = 0;
            let mut step: UINT8 = 0;
            let mut offset_depth = i as UINT32;
            let mut offset_fnum_bit: UINT32 = 0;
            let mut bit_tmp: UINT32 = 0;
            step = 0 as UINT8;
            while (step as i32) < 8_i32 {
                value = 0 as UINT8;
                bit_tmp = 0 as UINT32;
                while bit_tmp < 7 as UINT32 {
                    if fnum as i32 & (1_i32) << bit_tmp != 0 {
                        offset_fnum_bit = bit_tmp.wrapping_mul(8 as UINT32);
                        value = (value as i32
                            + lfo_pm_output[offset_fnum_bit.wrapping_add(offset_depth) as usize]
                                [step as usize] as i32) as UINT8;
                    }
                    bit_tmp = bit_tmp.wrapping_add(1);
                }
                lfo_pm_table
                    [(fnum as i32 * 32_i32 * 8_i32 + i * 32_i32 + step as i32 + 0_i32) as usize] =
                    value as INT32;
                lfo_pm_table[(fnum as i32 * 32_i32 * 8_i32
                    + i * 32_i32
                    + (step as i32 ^ 7_i32)
                    + 8_i32) as usize] = value as INT32;
                lfo_pm_table
                    [(fnum as i32 * 32_i32 * 8_i32 + i * 32_i32 + step as i32 + 16_i32) as usize] =
                    -(value as i32) as INT32;
                lfo_pm_table[(fnum as i32 * 32_i32 * 8_i32
                    + i * 32_i32
                    + (step as i32 ^ 7_i32)
                    + 24_i32) as usize] = -(value as i32) as INT32;
                step = step.wrapping_add(1);
            }
            fnum = fnum.wrapping_add(1);
        }
        i += 1;
    }
}
pub unsafe fn ym2612_generate(
    mut chip: *mut ::core::ffi::c_void,
    mut buffer: *mut FMSAMPLE,
    mut frames: i32,
    mut mix: i32,
) {
    let mut F2612 = chip as *mut YM2612;
    let mut cch = &raw mut (*F2612).CH as *mut FM_CH;
    let mut bufOut = buffer;
    let mut i: i32 = 0;
    ym2612_pre_generate(chip);
    if frames == 0 {
        update_ssg_eg_channel(
            (&raw mut (*cch.add(0)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        update_ssg_eg_channel(
            (&raw mut (*cch.add(1)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        update_ssg_eg_channel(
            (&raw mut (*cch.add(2)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        update_ssg_eg_channel(
            (&raw mut (*cch.add(3)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        update_ssg_eg_channel(
            (&raw mut (*cch.add(4)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        update_ssg_eg_channel(
            (&raw mut (*cch.add(5)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
    }
    i = 0_i32;
    while i < frames {
        while (*F2612).OPN.ST.framecnt >= (*F2612).OPN.ST.rateratio {
            (*F2612).OPN.ST.prev_sample[0] = (*F2612).OPN.ST.cur_sample[0];
            (*F2612).OPN.ST.prev_sample[1] = (*F2612).OPN.ST.cur_sample[1];
            ym2612_generate_one_native(chip, &raw mut (*F2612).OPN.ST.cur_sample as *mut FMSAMPLE);
            (*F2612).OPN.ST.framecnt -= (*F2612).OPN.ST.rateratio;
        }
        if mix != 0 {
            let fresh0 = bufOut;
            bufOut = bufOut.offset(1);
            *fresh0 = (*fresh0 as i32
                + (((*F2612).OPN.ST.prev_sample[0] as INT32
                    * ((*F2612).OPN.ST.rateratio - (*F2612).OPN.ST.framecnt)
                    + (*F2612).OPN.ST.cur_sample[0] as INT32 * (*F2612).OPN.ST.framecnt)
                    / (*F2612).OPN.ST.rateratio) as FMSAMPLE as i32)
                as FMSAMPLE;
            let fresh1 = bufOut;
            bufOut = bufOut.offset(1);
            *fresh1 = (*fresh1 as i32
                + (((*F2612).OPN.ST.prev_sample[1] as INT32
                    * ((*F2612).OPN.ST.rateratio - (*F2612).OPN.ST.framecnt)
                    + (*F2612).OPN.ST.cur_sample[1] as INT32 * (*F2612).OPN.ST.framecnt)
                    / (*F2612).OPN.ST.rateratio) as FMSAMPLE as i32)
                as FMSAMPLE;
        } else {
            let fresh2 = bufOut;
            bufOut = bufOut.offset(1);
            *fresh2 = (((*F2612).OPN.ST.prev_sample[0] as INT32
                * ((*F2612).OPN.ST.rateratio - (*F2612).OPN.ST.framecnt)
                + (*F2612).OPN.ST.cur_sample[0] as INT32 * (*F2612).OPN.ST.framecnt)
                / (*F2612).OPN.ST.rateratio) as FMSAMPLE;
            let fresh3 = bufOut;
            bufOut = bufOut.offset(1);
            *fresh3 = (((*F2612).OPN.ST.prev_sample[1] as INT32
                * ((*F2612).OPN.ST.rateratio - (*F2612).OPN.ST.framecnt)
                + (*F2612).OPN.ST.cur_sample[1] as INT32 * (*F2612).OPN.ST.framecnt)
                / (*F2612).OPN.ST.rateratio) as FMSAMPLE;
        }
        (*F2612).OPN.ST.framecnt += (1_i32) << RSM_FRAC;
        i += 1;
    }
}
unsafe fn ym2612_pre_generate(mut chip: *mut ::core::ffi::c_void) {
    let mut F2612 = chip as *mut YM2612;
    let mut OPN: *mut FM_OPN = &raw mut (*F2612).OPN;
    let mut cch = &raw mut (*F2612).CH as *mut FM_CH;
    refresh_fc_eg_chan(OPN, cch.add(0) as *mut FM_CH);
    refresh_fc_eg_chan(OPN, cch.add(1) as *mut FM_CH);
    if (*OPN).ST.mode & 0xc0 as UINT32 != 0 {
        if (*cch.add(2)).SLOT[SLOT1 as usize].Incr == -(1_i32) {
            refresh_fc_eg_slot(
                OPN,
                (&raw mut (*cch.add(2)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize)
                    as *mut FM_SLOT,
                (*OPN).SL3.fc[1] as i32,
                (*OPN).SL3.kcode[1] as i32,
            );
            refresh_fc_eg_slot(
                OPN,
                (&raw mut (*cch.add(2)).SLOT as *mut FM_SLOT).offset(SLOT2 as isize)
                    as *mut FM_SLOT,
                (*OPN).SL3.fc[2] as i32,
                (*OPN).SL3.kcode[2] as i32,
            );
            refresh_fc_eg_slot(
                OPN,
                (&raw mut (*cch.add(2)).SLOT as *mut FM_SLOT).offset(SLOT3 as isize)
                    as *mut FM_SLOT,
                (*OPN).SL3.fc[0] as i32,
                (*OPN).SL3.kcode[0] as i32,
            );
            refresh_fc_eg_slot(
                OPN,
                (&raw mut (*cch.add(2)).SLOT as *mut FM_SLOT).offset(SLOT4 as isize)
                    as *mut FM_SLOT,
                (*cch.add(2)).fc as i32,
                (*cch.add(2)).kcode as i32,
            );
        }
    } else {
        refresh_fc_eg_chan(OPN, cch.add(2) as *mut FM_CH);
    }
    refresh_fc_eg_chan(OPN, cch.add(3) as *mut FM_CH);
    refresh_fc_eg_chan(OPN, cch.add(4) as *mut FM_CH);
    refresh_fc_eg_chan(OPN, cch.add(5) as *mut FM_CH);
}
unsafe fn ym2612_generate_one_native(
    mut chip: *mut ::core::ffi::c_void,
    mut buffer: *mut FMSAMPLE,
) {
    let mut F2612 = chip as *mut YM2612;
    let mut OPN: *mut FM_OPN = &raw mut (*F2612).OPN;
    let mut out_fm = &raw mut (*OPN).out_fm as *mut INT32;
    let mut cch = &raw mut (*F2612).CH as *mut FM_CH;
    let mut dacout: INT32 = 0;
    let mut lt: i32 = 0;
    let mut rt: i32 = 0;
    if (*F2612).MuteDAC == 0 {
        dacout = (*F2612).dacout;
    } else {
        dacout = 0_i32 as INT32;
    }
    *out_fm.add(0) = 0_i32 as INT32;
    *out_fm.add(1) = 0_i32 as INT32;
    *out_fm.add(2) = 0_i32 as INT32;
    *out_fm.add(3) = 0_i32 as INT32;
    *out_fm.add(4) = 0_i32 as INT32;
    *out_fm.add(5) = 0_i32 as INT32;
    update_ssg_eg_channel(
        (&raw mut (*cch.add(0)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
    );
    update_ssg_eg_channel(
        (&raw mut (*cch.add(1)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
    );
    update_ssg_eg_channel(
        (&raw mut (*cch.add(2)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
    );
    update_ssg_eg_channel(
        (&raw mut (*cch.add(3)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
    );
    update_ssg_eg_channel(
        (&raw mut (*cch.add(4)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
    );
    update_ssg_eg_channel(
        (&raw mut (*cch.add(5)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
    );
    if (*F2612).dac_test == 0 {
        chan_calc(F2612, OPN, cch.add(0) as *mut FM_CH);
        chan_calc(F2612, OPN, cch.add(1) as *mut FM_CH);
        chan_calc(F2612, OPN, cch.add(2) as *mut FM_CH);
        chan_calc(F2612, OPN, cch.add(3) as *mut FM_CH);
        chan_calc(F2612, OPN, cch.add(4) as *mut FM_CH);
        if (*F2612).dacen != 0 {
            // DAC replaces channel six. Its destination pointer must stay in the chip.
            // The VGM/GYM stream mixers handle the DAC externally with its level centered.
            *(*cch.add(5)).connect4 += dacout;
        } else {
            chan_calc(F2612, OPN, cch.add(5) as *mut FM_CH);
        }
    } else {
        let ref mut fresh5 = *out_fm.add(1);
        *fresh5 = dacout;
        *out_fm.add(0) = *fresh5;
        let ref mut fresh6 = *out_fm.add(3);
        *fresh6 = dacout;
        *out_fm.add(2) = *fresh6;
        *out_fm.add(5) = dacout;
    }
    advance_lfo(OPN);
    (*OPN).eg_timer = (*OPN).eg_timer.wrapping_add((*OPN).eg_timer_add);
    while (*OPN).eg_timer >= (*OPN).eg_timer_overflow {
        (*OPN).eg_timer = (*OPN).eg_timer.wrapping_sub((*OPN).eg_timer_overflow);
        (*OPN).eg_cnt = (*OPN).eg_cnt.wrapping_add(1);
        if (*OPN).eg_cnt == 4096 as UINT32 {
            (*OPN).eg_cnt = 1 as UINT32;
        }
        advance_eg_channel(
            OPN,
            (&raw mut (*cch.add(0)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        advance_eg_channel(
            OPN,
            (&raw mut (*cch.add(1)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        advance_eg_channel(
            OPN,
            (&raw mut (*cch.add(2)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        advance_eg_channel(
            OPN,
            (&raw mut (*cch.add(3)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        advance_eg_channel(
            OPN,
            (&raw mut (*cch.add(4)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
        advance_eg_channel(
            OPN,
            (&raw mut (*cch.add(5)).SLOT as *mut FM_SLOT).offset(SLOT1 as isize) as *mut FM_SLOT,
        );
    }
    if *out_fm.add(0) > 8192_i32 {
        *out_fm.add(0) = 8192_i32 as INT32;
    } else if *out_fm.add(0) < -(8192_i32) {
        *out_fm.add(0) = -(8192_i32) as INT32;
    }
    if *out_fm.add(1) > 8192_i32 {
        *out_fm.add(1) = 8192_i32 as INT32;
    } else if *out_fm.add(1) < -(8192_i32) {
        *out_fm.add(1) = -(8192_i32) as INT32;
    }
    if *out_fm.add(2) > 8192_i32 {
        *out_fm.add(2) = 8192_i32 as INT32;
    } else if *out_fm.add(2) < -(8192_i32) {
        *out_fm.add(2) = -(8192_i32) as INT32;
    }
    if *out_fm.add(3) > 8192_i32 {
        *out_fm.add(3) = 8192_i32 as INT32;
    } else if *out_fm.add(3) < -(8192_i32) {
        *out_fm.add(3) = -(8192_i32) as INT32;
    }
    if *out_fm.add(4) > 8192_i32 {
        *out_fm.add(4) = 8192_i32 as INT32;
    } else if *out_fm.add(4) < -(8192_i32) {
        *out_fm.add(4) = -(8192_i32) as INT32;
    }
    if *out_fm.add(5) > 8192_i32 {
        *out_fm.add(5) = 8192_i32 as INT32;
    } else if *out_fm.add(5) < -(8192_i32) {
        *out_fm.add(5) = -(8192_i32) as INT32;
    }
    lt = ((*out_fm.add(0) >> 0_i32) as u32 & (*OPN).pan[0]) as i32;
    rt = ((*out_fm.add(0) >> 0_i32) as u32 & (*OPN).pan[1]) as i32;
    lt = (lt as u32).wrapping_add((*out_fm.add(1) >> 0_i32) as u32 & (*OPN).pan[2]) as i32 as i32;
    rt = (rt as u32).wrapping_add((*out_fm.add(1) >> 0_i32) as u32 & (*OPN).pan[3]) as i32 as i32;
    lt = (lt as u32).wrapping_add((*out_fm.add(2) >> 0_i32) as u32 & (*OPN).pan[4]) as i32 as i32;
    rt = (rt as u32).wrapping_add((*out_fm.add(2) >> 0_i32) as u32 & (*OPN).pan[5]) as i32 as i32;
    lt = (lt as u32).wrapping_add((*out_fm.add(3) >> 0_i32) as u32 & (*OPN).pan[6]) as i32 as i32;
    rt = (rt as u32).wrapping_add((*out_fm.add(3) >> 0_i32) as u32 & (*OPN).pan[7]) as i32 as i32;
    if (*F2612).dac_test == 0 {
        lt = (lt as u32).wrapping_add((*out_fm.add(4) >> 0_i32) as u32 & (*OPN).pan[8]) as i32
            as i32;
        rt = (rt as u32).wrapping_add((*out_fm.add(4) >> 0_i32) as u32 & (*OPN).pan[9]) as i32
            as i32;
    } else {
        lt += dacout as i32;
        lt += dacout as i32;
    }
    lt = (lt as u32).wrapping_add((*out_fm.add(5) >> 0_i32) as u32 & (*OPN).pan[10]) as i32 as i32;
    rt = (rt as u32).wrapping_add((*out_fm.add(5) >> 0_i32) as u32 & (*OPN).pan[11]) as i32 as i32;
    if (*F2612).WaveOutMode as i32 & 0x1_i32 != 0 {
        (*F2612).WaveL = lt as INT32;
    }
    if (*F2612).WaveOutMode as i32 & 0x2_i32 != 0 {
        (*F2612).WaveR = rt as INT32;
    }
    if (*F2612).WaveOutMode as i32 ^ 0x3_i32 != 0 {
        (*F2612).WaveOutMode = ((*F2612).WaveOutMode as i32 ^ 0x3_i32) as UINT8;
    }
    *buffer.add(0) = ((*F2612).WaveL as i32 / 2_i32) as FMSAMPLE;
    *buffer.add(1) = ((*F2612).WaveR as i32 / 2_i32) as FMSAMPLE;
    (*OPN).SL3.key_csm = (((*OPN).SL3.key_csm as i32) << 1_i32) as UINT8;
    if (*OPN).ST.TAC != 0 && (*OPN).ST.timer_handler.is_none() {
        (*OPN).ST.TAC -= ((*OPN).ST.freqbase * 4096_i32 as f64) as i32;
        if (*OPN).ST.TAC <= 0_i32 {
            TimerAOver(&raw mut (*OPN).ST);
            if (*OPN).ST.mode & 0x80 as UINT32 != 0 {
                CSMKeyControll(OPN, cch.add(2) as *mut FM_CH);
            }
        }
    }
    if (*OPN).SL3.key_csm as i32 & 2_i32 != 0 {
        FM_KEYOFF_CSM(cch.add(2) as *mut FM_CH, SLOT1);
        FM_KEYOFF_CSM(cch.add(2) as *mut FM_CH, SLOT2);
        FM_KEYOFF_CSM(cch.add(2) as *mut FM_CH, SLOT3);
        FM_KEYOFF_CSM(cch.add(2) as *mut FM_CH, SLOT4);
        (*OPN).SL3.key_csm = 0 as UINT8;
    }
}
pub unsafe fn ym2612_init(
    mut storage: *mut ::core::ffi::c_void,
    mut param: *mut ::core::ffi::c_void,
    mut clock: i32,
    mut rate: i32,
    mut timer_handler: FM_TIMERHANDLER,
    mut IRQHandler: FM_IRQHANDLER,
) -> *mut ::core::ffi::c_void {
    let mut F2612 = storage as *mut YM2612;
    if clock <= 0_i32 || rate <= 0_i32 {
        return NULL;
    }
    (*F2612).OPN.ST.param = param;
    (*F2612).OPN.type_0 = TYPE_YM2612 as UINT8;
    (*F2612).OPN.P_CH = &raw mut (*F2612).CH as *mut FM_CH;
    (*F2612).OPN.ST.clock = clock as UINT32;
    (*F2612).OPN.ST.rate = 53267 as UINT32;
    (*F2612).OPN.ST.rateratio = ((144_i32 as UINT64).wrapping_mul(rate as UINT64) << RSM_FRAC)
        .wrapping_div(7670454 as UINT64) as UINT32 as INT32;
    (*F2612).OPN.ST.framecnt = ((1_i32) << RSM_FRAC) as INT32;
    (*F2612).OPN.ST.cur_sample.fill(0);
    (*F2612).OPN.ST.prev_sample.fill(0);
    (*F2612).OPN.ST.timer_handler = timer_handler;
    (*F2612).OPN.ST.IRQ_Handler = IRQHandler;
    if PseudoSt != 0 {
        (*F2612).WaveOutMode = 0x1 as UINT8;
    } else {
        (*F2612).WaveOutMode = 0x3 as UINT8;
    }
    return F2612 as *mut ::core::ffi::c_void;
}
pub unsafe fn ym2612_reset_chip(mut chip: *mut ::core::ffi::c_void) {
    let mut i: i32 = 0;
    let mut F2612 = chip as *mut YM2612;
    let mut OPN: *mut FM_OPN = &raw mut (*F2612).OPN;
    OPNSetPres(OPN, 6_i32 * 24_i32, 6_i32 * 24_i32, 0_i32);
    FM_IRQMASK_SET(&raw mut (*OPN).ST, 0x3_i32);
    (*F2612).OPN.ST.framecnt = ((1_i32) << RSM_FRAC) as INT32;
    (*F2612).OPN.ST.cur_sample.fill(0);
    (*F2612).OPN.ST.prev_sample.fill(0);
    (*OPN).eg_timer = 0 as UINT32;
    (*OPN).eg_cnt = 0 as UINT32;
    (*OPN).lfo_timer = 0 as UINT32;
    (*OPN).lfo_cnt = 0 as UINT8;
    (*OPN).LFO_AM = 126 as UINT32;
    (*OPN).LFO_PM = 0 as UINT32;
    (*OPN).ST.TAC = 0_i32 as INT32;
    (*OPN).ST.TBC = 0_i32 as INT32;
    (*OPN).SL3.key_csm = 0 as UINT8;
    (*OPN).ST.status = 0 as UINT8;
    (*OPN).ST.mode = 0 as UINT32;
    (*F2612).REGS.fill(0);
    OPNWriteMode(OPN, 0x22_i32, 0_i32);
    OPNWriteMode(OPN, 0x27_i32, 0x30_i32);
    OPNWriteMode(OPN, 0x26_i32, 0_i32);
    OPNWriteMode(OPN, 0x25_i32, 0_i32);
    OPNWriteMode(OPN, 0x24_i32, 0_i32);
    reset_channels(
        &raw mut (*OPN).ST,
        (&raw mut (*F2612).CH as *mut FM_CH).add(0) as *mut FM_CH,
        6_i32,
    );
    i = 0xb6_i32;
    while i >= 0xb4_i32 {
        OPNWriteReg(OPN, i, 0xc0_i32);
        OPNWriteReg(OPN, i | 0x100_i32, 0xc0_i32);
        i -= 1;
    }
    i = 0xb2_i32;
    while i >= 0x30_i32 {
        OPNWriteReg(OPN, i, 0_i32);
        OPNWriteReg(OPN, i | 0x100_i32, 0_i32);
        i -= 1;
    }
    (*F2612).dacen = 0 as UINT8;
    (*F2612).dac_test = 0 as UINT8;
    (*F2612).dacout = 0_i32 as INT32;
    if (*F2612).WaveOutMode as i32 == 0x2_i32 {
        (*F2612).WaveOutMode = ((*F2612).WaveOutMode as i32 >> 1_i32) as UINT8;
    }
}
pub unsafe fn ym2612_write(mut chip: *mut ::core::ffi::c_void, mut a: i32, mut v: UINT8) -> i32 {
    let mut F2612 = chip as *mut YM2612;
    let mut addr: i32 = 0;
    v = (v as i32 & 0xff_i32) as UINT8;
    match a & 3_i32 {
        0 => {
            (*F2612).OPN.ST.address = v;
            (*F2612).addr_A1 = 0 as UINT8;
        }
        1 => {
            if !((*F2612).addr_A1 as i32 != 0_i32) {
                addr = (*F2612).OPN.ST.address as i32;
                (*F2612).REGS[addr as usize] = v;
                match addr & 0xf0_i32 {
                    32 => match addr {
                        42 => {
                            ym2612_generate(chip, DUMMYBUF as *mut FMSAMPLE, 0_i32, 0_i32);
                            (*F2612).dacout = ((v as i32 - 0x80_i32) << 6_i32) as INT32;
                        }
                        43 => {
                            (*F2612).dacen = (v as i32 & 0x80_i32) as UINT8;
                        }
                        44 => {
                            (*F2612).dac_test = (v as i32 & 0x20_i32) as UINT8;
                        }
                        _ => {
                            ym2612_generate(chip, DUMMYBUF as *mut FMSAMPLE, 0_i32, 0_i32);
                            OPNWriteMode(&raw mut (*F2612).OPN, addr, v as i32);
                        }
                    },
                    _ => {
                        ym2612_generate(chip, DUMMYBUF as *mut FMSAMPLE, 0_i32, 0_i32);
                        OPNWriteReg(&raw mut (*F2612).OPN, addr, v as i32);
                    }
                }
            }
        }
        2 => {
            (*F2612).OPN.ST.address = v;
            (*F2612).addr_A1 = 1 as UINT8;
        }
        3 => {
            if !((*F2612).addr_A1 as i32 != 1_i32) {
                addr = (*F2612).OPN.ST.address as i32;
                (*F2612).REGS[(addr | 0x100_i32) as usize] = v;
                ym2612_generate(chip, DUMMYBUF as *mut FMSAMPLE, 0_i32, 0_i32);
                OPNWriteReg(&raw mut (*F2612).OPN, addr | 0x100_i32, v as i32);
            }
        }
        _ => {}
    }
    return (*F2612).OPN.ST.irq as i32;
}
pub unsafe fn ym2612_set_mutemask(mut chip: *mut ::core::ffi::c_void, mut MuteMask: UINT32) {
    let mut F2612 = chip as *mut YM2612;
    let mut CurChn: UINT8 = 0;
    CurChn = 0 as UINT8;
    while (CurChn as i32) < 6_i32 {
        (*F2612).CH[CurChn as usize].Muted = (MuteMask >> CurChn as i32 & 0x1 as UINT32) as UINT8;
        CurChn = CurChn.wrapping_add(1);
    }
    (*F2612).MuteDAC = (MuteMask >> 6_i32 & 0x1 as UINT32) as UINT8;
}
