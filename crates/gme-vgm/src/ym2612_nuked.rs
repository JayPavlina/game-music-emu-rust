// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2017 Alexey Khokholov (Nuke.YKT)
// Rust adaptation of Ym2612_Nuked.cpp (2026-06-11), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
//! YM2612 (Nuked OPN2) emulator.
//!
//! 1:1 transliteration of `Ym2612_Nuked.cpp` (Nuked OPN2 / ym3438 core
//! v1.0.7 by Alexey Khokholov, wrapped in `Ym2612_Nuked_Emu`).
//!
//! Integer widths follow the C++ `Bit8u`/`Bit16u`/`Bit16s`/`Bit32u`/... types
//! exactly; where C++ relies on integer promotion to `int` the math is done in
//! `i32`/`u32` and truncated back, matching the original wrapping behavior.

const RSM_FRAC: u32 = 10;
const OPN_WRITEBUF_SIZE: usize = 2048;
const OPN_WRITEBUF_DELAY: u64 = 15;

#[allow(dead_code)]
const YM3438_TYPE_DISCRETE: u32 = 0; // Discrete YM3438 (Teradrive)
const YM3438_TYPE_ASIC: u32 = 1; // ASIC YM3438 (MD1 VA7, MD2, MD3, etc)
#[allow(dead_code)]
const YM3438_TYPE_YM2612: u32 = 2; // YM2612 (MD1, MD2 VA2)

// C++ has a mutable global `chip_type`; the wrapper's Ym2612_Nuked_Emu
// constructor always sets it to ym3438_type_asic before any use, so it is a
// constant here.
const CHIP_TYPE: u32 = YM3438_TYPE_ASIC;

const EG_NUM_ATTACK: u8 = 0;
const EG_NUM_DECAY: u8 = 1;
const EG_NUM_SUSTAIN: u8 = 2;
const EG_NUM_RELEASE: u8 = 3;

// ---- tables (generated from Ym2612_Nuked.cpp; do not edit by hand) ----
const LOGSINROM: [u16; 256] = [
    2137, 1731, 1543, 1419, 1326, 1252, 1190, 1137, 1091, 1050, 1013, 979, 949, 920, 894, 869, 846,
    825, 804, 785, 767, 749, 732, 717, 701, 687, 672, 659, 646, 633, 621, 609, 598, 587, 576, 566,
    556, 546, 536, 527, 518, 509, 501, 492, 484, 476, 468, 461, 453, 446, 439, 432, 425, 418, 411,
    405, 399, 392, 386, 380, 375, 369, 363, 358, 352, 347, 341, 336, 331, 326, 321, 316, 311, 307,
    302, 297, 293, 289, 284, 280, 276, 271, 267, 263, 259, 255, 251, 248, 244, 240, 236, 233, 229,
    226, 222, 219, 215, 212, 209, 205, 202, 199, 196, 193, 190, 187, 184, 181, 178, 175, 172, 169,
    167, 164, 161, 159, 156, 153, 151, 148, 146, 143, 141, 138, 136, 134, 131, 129, 127, 125, 122,
    120, 118, 116, 114, 112, 110, 108, 106, 104, 102, 100, 98, 96, 94, 92, 91, 89, 87, 85, 83, 82,
    80, 78, 77, 75, 74, 72, 70, 69, 67, 66, 64, 63, 62, 60, 59, 57, 56, 55, 53, 52, 51, 49, 48, 47,
    46, 45, 43, 42, 41, 40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 23,
    22, 21, 20, 20, 19, 18, 17, 17, 16, 15, 15, 14, 13, 13, 12, 12, 11, 10, 10, 9, 9, 8, 8, 7, 7,
    7, 6, 6, 5, 5, 5, 4, 4, 4, 3, 3, 3, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0,
];

const EXPROM: [u16; 256] = [
    0, 3, 6, 8, 11, 14, 17, 20, 22, 25, 28, 31, 34, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 69,
    72, 75, 78, 81, 84, 87, 90, 93, 96, 99, 102, 105, 108, 111, 114, 117, 120, 123, 126, 130, 133,
    136, 139, 142, 145, 148, 152, 155, 158, 161, 164, 168, 171, 174, 177, 181, 184, 187, 190, 194,
    197, 200, 204, 207, 210, 214, 217, 220, 224, 227, 231, 234, 237, 241, 244, 248, 251, 255, 258,
    262, 265, 268, 272, 276, 279, 283, 286, 290, 293, 297, 300, 304, 308, 311, 315, 318, 322, 326,
    329, 333, 337, 340, 344, 348, 352, 355, 359, 363, 367, 370, 374, 378, 382, 385, 389, 393, 397,
    401, 405, 409, 412, 416, 420, 424, 428, 432, 436, 440, 444, 448, 452, 456, 460, 464, 468, 472,
    476, 480, 484, 488, 492, 496, 501, 505, 509, 513, 517, 521, 526, 530, 534, 538, 542, 547, 551,
    555, 560, 564, 568, 572, 577, 581, 585, 590, 594, 599, 603, 607, 612, 616, 621, 625, 630, 634,
    639, 643, 648, 652, 657, 661, 666, 670, 675, 680, 684, 689, 693, 698, 703, 708, 712, 717, 722,
    726, 731, 736, 741, 745, 750, 755, 760, 765, 770, 774, 779, 784, 789, 794, 799, 804, 809, 814,
    819, 824, 829, 834, 839, 844, 849, 854, 859, 864, 869, 874, 880, 885, 890, 895, 900, 906, 911,
    916, 921, 927, 932, 937, 942, 948, 953, 959, 964, 969, 975, 980, 986, 991, 996, 1002, 1007,
    1013, 1018,
];

const FN_NOTE: [u32; 16] = [0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 3, 3, 3, 3, 3, 3];

const EG_STEPHI: [[u32; 4]; 4] = [[0, 0, 0, 0], [1, 0, 0, 0], [1, 0, 1, 0], [1, 1, 1, 0]];

const EG_AM_SHIFT: [u8; 4] = [7, 3, 1, 0];

const PG_DETUNE: [u32; 8] = [16, 17, 19, 20, 22, 24, 27, 29];

const PG_LFO_SH1: [[u32; 8]; 8] = [
    [7, 7, 7, 7, 7, 7, 7, 7],
    [7, 7, 7, 7, 7, 7, 7, 7],
    [7, 7, 7, 7, 7, 7, 1, 1],
    [7, 7, 7, 7, 1, 1, 1, 1],
    [7, 7, 7, 1, 1, 1, 1, 0],
    [7, 7, 1, 1, 0, 0, 0, 0],
    [7, 7, 1, 1, 0, 0, 0, 0],
    [7, 7, 1, 1, 0, 0, 0, 0],
];

const PG_LFO_SH2: [[u32; 8]; 8] = [
    [7, 7, 7, 7, 7, 7, 7, 7],
    [7, 7, 7, 7, 2, 2, 2, 2],
    [7, 7, 7, 2, 2, 2, 7, 7],
    [7, 7, 2, 2, 7, 7, 2, 2],
    [7, 7, 2, 7, 7, 7, 2, 7],
    [7, 7, 7, 2, 7, 7, 2, 1],
    [7, 7, 7, 2, 7, 7, 2, 1],
    [7, 7, 7, 2, 7, 7, 2, 1],
];

const OP_OFFSET: [u32; 12] = [0, 1, 2, 256, 257, 258, 4, 5, 6, 260, 261, 262];

const CH_OFFSET: [u32; 6] = [0, 1, 2, 256, 257, 258];

const LFO_CYCLES: [u32; 8] = [108, 77, 71, 67, 62, 44, 8, 5];

const FM_ALGORITHM: [[[u32; 8]; 6]; 4] = [
    [
        [1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 1],
    ],
    [
        [0, 1, 0, 0, 0, 1, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [1, 1, 1, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 1, 1, 1],
    ],
    [
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [1, 0, 0, 1, 1, 1, 1, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 1, 1, 1, 1],
    ],
    [
        [0, 0, 1, 0, 0, 1, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 1, 0, 0, 0, 0],
        [1, 1, 0, 1, 1, 0, 0, 0],
        [0, 0, 1, 0, 0, 0, 0, 0],
        [1, 1, 1, 1, 1, 1, 1, 1],
    ],
];
// ---- end tables ----

#[derive(Clone, Copy)]
struct OpnWriteBuf {
    time: u64,
    port: u8,
    data: u8,
}

/// C++ `ym3438_t`
pub struct Ym3438 {
    cycles: u32,
    channel: u32,
    mol: i16,
    mor: i16,
    /* IO */
    write_data: u16,
    write_a: u8,
    write_d: u8,
    write_a_en: u8,
    write_d_en: u8,
    write_busy: u8,
    write_busy_cnt: u8,
    write_fm_address: u8,
    write_fm_data: u8,
    write_fm_mode_a: u8,
    address: u16,
    data: u8,
    pin_test_in: u8,
    #[allow(dead_code)]
    pin_irq: u8,
    busy: u8,
    /* LFO */
    lfo_en: u8,
    lfo_freq: u8,
    lfo_pm: u8,
    lfo_am: u8,
    lfo_cnt: u8,
    lfo_inc: u8,
    lfo_quotient: u8,
    /* Phase generator */
    pg_fnum: u16,
    pg_block: u8,
    pg_kcode: u8,
    pg_inc: [u32; 24],
    pg_phase: [u32; 24],
    pg_reset: [u8; 24],
    pg_read: u32,
    /* Envelope generator */
    eg_cycle: u8,
    eg_cycle_stop: u8,
    eg_shift: u8,
    eg_shift_lock: u8,
    eg_timer_low_lock: u8,
    eg_timer: u16,
    eg_timer_inc: u8,
    eg_quotient: u16,
    eg_custom_timer: u8,
    eg_rate: u8,
    eg_ksv: u8,
    eg_inc: u8,
    eg_ratemax: u8,
    eg_sl: [u8; 2],
    eg_lfo_am: u8,
    eg_tl: [u8; 2],
    eg_state: [u8; 24],
    eg_level: [u16; 24],
    eg_out: [u16; 24],
    eg_kon: [u8; 24],
    eg_kon_csm: [u8; 24],
    eg_kon_latch: [u8; 24],
    #[allow(dead_code)]
    eg_csm_mode: [u8; 24],
    eg_ssg_enable: [u8; 24],
    eg_ssg_pgrst_latch: [u8; 24],
    eg_ssg_repeat_latch: [u8; 24],
    eg_ssg_hold_up_latch: [u8; 24],
    eg_ssg_dir: [u8; 24],
    eg_ssg_inv: [u8; 24],
    eg_read: [u32; 2],
    eg_read_inc: u8,
    /* FM */
    fm_op1: [[i16; 2]; 6],
    fm_op2: [i16; 6],
    fm_out: [i16; 24],
    fm_mod: [u16; 24],
    /* Channel */
    ch_acc: [i16; 6],
    ch_out: [i16; 6],
    ch_lock: i16,
    ch_lock_l: u8,
    ch_lock_r: u8,
    ch_read: i16,
    /* Timer */
    timer_a_cnt: u16,
    timer_a_reg: u16,
    timer_a_load_lock: u8,
    timer_a_load: u8,
    timer_a_enable: u8,
    timer_a_reset: u8,
    timer_a_load_latch: u8,
    timer_a_overflow_flag: u8,
    timer_a_overflow: u8,

    timer_b_cnt: u16,
    timer_b_subcnt: u8,
    timer_b_reg: u16,
    timer_b_load_lock: u8,
    timer_b_load: u8,
    timer_b_enable: u8,
    timer_b_reset: u8,
    timer_b_load_latch: u8,
    timer_b_overflow_flag: u8,
    timer_b_overflow: u8,

    /* Register set */
    mode_test_21: [u8; 8],
    mode_test_2c: [u8; 8],
    mode_ch3: u8,
    mode_kon_channel: u8,
    mode_kon_operator: [u8; 4],
    mode_kon: [u8; 24],
    mode_csm: u8,
    mode_kon_csm: u8,
    dacen: u8,
    dacdata: i16,

    ks: [u8; 24],
    ar: [u8; 24],
    sr: [u8; 24],
    dt: [u8; 24],
    multi: [u8; 24],
    sl: [u8; 24],
    rr: [u8; 24],
    dr: [u8; 24],
    am: [u8; 24],
    tl: [u8; 24],
    ssg_eg: [u8; 24],

    fnum: [u16; 6],
    block: [u8; 6],
    kcode: [u8; 6],
    fnum_3ch: [u16; 6],
    block_3ch: [u8; 6],
    kcode_3ch: [u8; 6],
    reg_a4: u8,
    reg_ac: u8,
    connect: [u8; 6],
    fb: [u8; 6],
    pan_l: [u8; 6],
    pan_r: [u8; 6],
    ams: [u8; 6],
    pms: [u8; 6],

    /*EXTRA*/
    mute: [u32; 7],
    rateratio: i32,
    samplecnt: i32,
    oldsamples: [i32; 2],
    samples: [i32; 2],

    writebuf_samplecnt: u64,
    writebuf_cur: u32,
    writebuf_last: u32,
    writebuf_lasttime: u64,
    writebuf: [OpnWriteBuf; OPN_WRITEBUF_SIZE],
}

impl Ym3438 {
    fn zeroed() -> Self {
        // C++ allocates with `new ym3438_t` (uninitialized) and relies on
        // OPN2_Reset's memset; the struct is plain integer data so zeroing is
        // the exact equivalent.
        unsafe { std::mem::zeroed() }
    }

    /// C++ `OPN2_DoIO`
    fn do_io(&mut self) {
        /* Write signal check */
        self.write_a_en = ((self.write_a & 0x03) == 0x01) as u8;
        self.write_d_en = ((self.write_d & 0x03) == 0x01) as u8;
        self.write_a <<= 1;
        self.write_d <<= 1;
        /* Busy counter */
        self.busy = self.write_busy;
        self.write_busy_cnt = self.write_busy_cnt.wrapping_add(self.write_busy);
        self.write_busy = ((self.write_busy != 0 && (self.write_busy_cnt >> 5) == 0)
            || self.write_d_en != 0) as u8;
        self.write_busy_cnt &= 0x1f;
    }

    /// C++ `OPN2_DoRegWrite`
    fn do_reg_write(&mut self) {
        let mut slot = (self.cycles % 12) as usize;
        let channel = self.channel as usize;
        /* Update registers */
        if self.write_fm_data != 0 {
            /* Slot */
            if OP_OFFSET[slot] == (self.address as u32 & 0x107) {
                if self.address & 0x08 != 0 {
                    /* OP2, OP4 */
                    slot += 12;
                }
                let address = self.address & 0xf0;
                match address {
                    0x30 => {
                        /* DT, MULTI */
                        self.multi[slot] = self.data & 0x0f;
                        if self.multi[slot] == 0 {
                            self.multi[slot] = 1;
                        } else {
                            self.multi[slot] <<= 1;
                        }
                        self.dt[slot] = (self.data >> 4) & 0x07;
                    }
                    0x40 => {
                        /* TL */
                        self.tl[slot] = self.data & 0x7f;
                    }
                    0x50 => {
                        /* KS, AR */
                        self.ar[slot] = self.data & 0x1f;
                        self.ks[slot] = (self.data >> 6) & 0x03;
                    }
                    0x60 => {
                        /* AM, DR */
                        self.dr[slot] = self.data & 0x1f;
                        self.am[slot] = (self.data >> 7) & 0x01;
                    }
                    0x70 => {
                        /* SR */
                        self.sr[slot] = self.data & 0x1f;
                    }
                    0x80 => {
                        /* SL, RR */
                        self.rr[slot] = self.data & 0x0f;
                        self.sl[slot] = (self.data >> 4) & 0x0f;
                        self.sl[slot] |= (self.sl[slot] + 1) & 0x10;
                    }
                    0x90 => {
                        /* SSG-EG */
                        self.ssg_eg[slot] = self.data & 0x0f;
                    }
                    _ => {}
                }
            }

            /* Channel */
            if CH_OFFSET[channel] == (self.address as u32 & 0x103) {
                let address = self.address & 0xfc;
                match address {
                    0xa0 => {
                        self.fnum[channel] =
                            (self.data as u16 & 0xff) | ((self.reg_a4 as u16 & 0x07) << 8);
                        self.block[channel] = (self.reg_a4 >> 3) & 0x07;
                        self.kcode[channel] = (self.block[channel] << 2)
                            | FN_NOTE[(self.fnum[channel] >> 7) as usize] as u8;
                    }
                    0xa4 => {
                        self.reg_a4 = self.data;
                    }
                    0xa8 => {
                        self.fnum_3ch[channel] =
                            (self.data as u16 & 0xff) | ((self.reg_ac as u16 & 0x07) << 8);
                        self.block_3ch[channel] = (self.reg_ac >> 3) & 0x07;
                        self.kcode_3ch[channel] = (self.block_3ch[channel] << 2)
                            | FN_NOTE[(self.fnum_3ch[channel] >> 7) as usize] as u8;
                    }
                    0xac => {
                        self.reg_ac = self.data;
                    }
                    0xb0 => {
                        self.connect[channel] = self.data & 0x07;
                        self.fb[channel] = (self.data >> 3) & 0x07;
                    }
                    0xb4 => {
                        self.pms[channel] = self.data & 0x07;
                        self.ams[channel] = (self.data >> 4) & 0x03;
                        self.pan_l[channel] = (self.data >> 7) & 0x01;
                        self.pan_r[channel] = (self.data >> 6) & 0x01;
                    }
                    _ => {}
                }
            }
        }

        if self.write_a_en != 0 || self.write_d_en != 0 {
            /* Data */
            if self.write_a_en != 0 {
                self.write_fm_data = 0;
            }

            if self.write_fm_address != 0 && self.write_d_en != 0 {
                self.write_fm_data = 1;
            }

            /* Address */
            if self.write_a_en != 0 {
                if (self.write_data & 0xf0) != 0x00 {
                    /* FM Write */
                    self.address = self.write_data;
                    self.write_fm_address = 1;
                } else {
                    /* SSG write */
                    self.write_fm_address = 0;
                }
            }

            /* FM Mode */
            /* Data */
            if self.write_d_en != 0 && (self.write_data & 0x100) == 0 {
                match self.address {
                    0x21 => {
                        /* LSI test 1 */
                        for i in 0..8 {
                            self.mode_test_21[i] = ((self.write_data >> i) & 0x01) as u8;
                        }
                    }
                    0x22 => {
                        /* LFO control */
                        if (self.write_data >> 3) & 0x01 != 0 {
                            self.lfo_en = 0x7f;
                        } else {
                            self.lfo_en = 0;
                        }
                        self.lfo_freq = (self.write_data & 0x07) as u8;
                    }
                    0x24 => {
                        /* Timer A */
                        self.timer_a_reg &= 0x03;
                        self.timer_a_reg |= (self.write_data & 0xff) << 2;
                    }
                    0x25 => {
                        self.timer_a_reg &= 0x3fc;
                        self.timer_a_reg |= self.write_data & 0x03;
                    }
                    0x26 => {
                        /* Timer B */
                        self.timer_b_reg = self.write_data & 0xff;
                    }
                    0x27 => {
                        /* CSM, Timer control */
                        self.mode_ch3 = ((self.write_data & 0xc0) >> 6) as u8;
                        self.mode_csm = (self.mode_ch3 == 2) as u8;
                        self.timer_a_load = (self.write_data & 0x01) as u8;
                        self.timer_a_enable = ((self.write_data >> 2) & 0x01) as u8;
                        self.timer_a_reset = ((self.write_data >> 4) & 0x01) as u8;
                        self.timer_b_load = ((self.write_data >> 1) & 0x01) as u8;
                        self.timer_b_enable = ((self.write_data >> 3) & 0x01) as u8;
                        self.timer_b_reset = ((self.write_data >> 5) & 0x01) as u8;
                    }
                    0x28 => {
                        /* Key on/off */
                        for i in 0..4 {
                            self.mode_kon_operator[i] = ((self.write_data >> (4 + i)) & 0x01) as u8;
                        }
                        if (self.write_data & 0x03) == 0x03 {
                            /* Invalid address */
                            self.mode_kon_channel = 0xff;
                        } else {
                            self.mode_kon_channel =
                                ((self.write_data & 0x03) + ((self.write_data >> 2) & 1) * 3) as u8;
                        }
                    }
                    0x2a => {
                        /* DAC data */
                        self.dacdata &= 0x01;
                        self.dacdata |= ((self.write_data ^ 0x80) << 1) as i16;
                    }
                    0x2b => {
                        /* DAC enable */
                        self.dacen = (self.write_data >> 7) as u8;
                    }
                    0x2c => {
                        /* LSI test 2 */
                        for i in 0..8 {
                            self.mode_test_2c[i] = ((self.write_data >> i) & 0x01) as u8;
                        }
                        self.dacdata &= 0x1fe;
                        self.dacdata |= self.mode_test_2c[3] as i16;
                        self.eg_custom_timer =
                            (self.mode_test_2c[7] == 0 && self.mode_test_2c[6] != 0) as u8;
                    }
                    _ => {}
                }
            }

            /* Address */
            if self.write_a_en != 0 {
                self.write_fm_mode_a = (self.write_data & 0xff) as u8;
            }
        }

        if self.write_fm_data != 0 {
            self.data = (self.write_data & 0xff) as u8;
        }
    }

    /// C++ `OPN2_PhaseCalcIncrement`
    fn phase_calc_increment(&mut self) {
        let chan = self.channel as usize;
        let slot = self.cycles as usize;
        let mut fnum = self.pg_fnum as u32;
        let fnum_h = fnum >> 4;
        let lfo = self.lfo_pm;
        let mut lfo_l = lfo & 0x0f;
        let pms = self.pms[chan];
        let dt = self.dt[slot];
        let dt_l = dt & 0x03;
        let mut detune: u8 = 0;
        let mut kcode = self.pg_kcode;

        fnum <<= 1;
        /* Apply LFO */
        if lfo_l & 0x08 != 0 {
            lfo_l ^= 0x0f;
        }
        let mut fm = (fnum_h >> PG_LFO_SH1[pms as usize][lfo_l as usize])
            + (fnum_h >> PG_LFO_SH2[pms as usize][lfo_l as usize]);
        if pms > 5 {
            fm <<= pms - 5;
        }
        fm >>= 2;
        if lfo & 0x10 != 0 {
            fnum = fnum.wrapping_sub(fm);
        } else {
            fnum = fnum.wrapping_add(fm);
        }
        fnum &= 0xfff;

        let mut basefreq = (fnum << self.pg_block) >> 2;

        /* Apply detune */
        if dt_l != 0 {
            if kcode > 0x1c {
                kcode = 0x1c;
            }
            let block = kcode >> 2;
            let note = kcode & 0x03;
            let sum = block + 9 + (((dt_l == 3) as u8) | (dt_l & 0x02));
            let sum_h = sum >> 1;
            let sum_l = sum & 0x01;
            detune = (PG_DETUNE[((sum_l << 2) | note) as usize] >> (9 - sum_h)) as u8;
        }
        if dt & 0x04 != 0 {
            basefreq = basefreq.wrapping_sub(detune as u32);
        } else {
            basefreq = basefreq.wrapping_add(detune as u32);
        }
        basefreq &= 0x1ffff;
        self.pg_inc[slot] = (basefreq.wrapping_mul(self.multi[slot] as u32)) >> 1;
        self.pg_inc[slot] &= 0xfffff;
    }

    /// C++ `OPN2_PhaseGenerate`
    fn phase_generate(&mut self) {
        /* Mask increment */
        let mut slot = ((self.cycles + 20) % 24) as usize;
        if self.pg_reset[slot] != 0 {
            self.pg_inc[slot] = 0;
        }
        /* Phase step */
        slot = ((self.cycles + 19) % 24) as usize;
        self.pg_phase[slot] = self.pg_phase[slot].wrapping_add(self.pg_inc[slot]);
        self.pg_phase[slot] &= 0xfffff;
        if self.pg_reset[slot] != 0 || self.mode_test_21[3] != 0 {
            self.pg_phase[slot] = 0;
        }
    }

    /// C++ `OPN2_EnvelopeSSGEG`
    fn envelope_ssg_eg(&mut self) {
        let slot = self.cycles as usize;
        let mut direction: u8 = 0;
        self.eg_ssg_pgrst_latch[slot] = 0;
        self.eg_ssg_repeat_latch[slot] = 0;
        self.eg_ssg_hold_up_latch[slot] = 0;
        self.eg_ssg_inv[slot] = 0;
        if self.ssg_eg[slot] & 0x08 != 0 {
            direction = self.eg_ssg_dir[slot];
            if self.eg_level[slot] & 0x200 != 0 {
                /* Reset */
                if (self.ssg_eg[slot] & 0x03) == 0x00 {
                    self.eg_ssg_pgrst_latch[slot] = 1;
                }
                /* Repeat */
                if (self.ssg_eg[slot] & 0x01) == 0x00 {
                    self.eg_ssg_repeat_latch[slot] = 1;
                }
                /* Inverse */
                if (self.ssg_eg[slot] & 0x03) == 0x02 {
                    direction ^= 1;
                }
                if (self.ssg_eg[slot] & 0x03) == 0x03 {
                    direction = 1;
                }
            }
            /* Hold up */
            if self.eg_kon_latch[slot] != 0
                && ((self.ssg_eg[slot] & 0x07) == 0x05 || (self.ssg_eg[slot] & 0x07) == 0x03)
            {
                self.eg_ssg_hold_up_latch[slot] = 1;
            }
            direction &= self.eg_kon[slot];
            self.eg_ssg_inv[slot] =
                (self.eg_ssg_dir[slot] ^ ((self.ssg_eg[slot] >> 2) & 0x01)) & self.eg_kon[slot];
        }
        self.eg_ssg_dir[slot] = direction;
        self.eg_ssg_enable[slot] = (self.ssg_eg[slot] >> 3) & 0x01;
    }

    /// C++ `OPN2_EnvelopeADSR`
    fn envelope_adsr(&mut self) {
        let slot = ((self.cycles + 22) % 24) as usize;

        let nkon = self.eg_kon_latch[slot];
        let okon = self.eg_kon[slot];
        let mut nextstate = self.eg_state[slot];
        let mut inc: i16 = 0;
        self.eg_read[0] = self.eg_read_inc as u32;
        self.eg_read_inc = (self.eg_inc > 0) as u8;

        /* Reset phase generator */
        self.pg_reset[slot] =
            ((nkon != 0 && okon == 0) || self.eg_ssg_pgrst_latch[slot] != 0) as u8;

        /* KeyOn/Off */
        let kon_event =
            ((nkon != 0 && okon == 0) || (okon != 0 && self.eg_ssg_repeat_latch[slot] != 0)) as u8;
        let koff_event = (okon != 0 && nkon == 0) as u8;

        let mut level = self.eg_level[slot] as i16;
        let mut ssg_level = level;

        if self.eg_ssg_inv[slot] != 0 {
            /* Inverse */
            ssg_level = (512i32.wrapping_sub(level as i32)) as i16;
            ssg_level = (ssg_level as i32 & 0x3ff) as i16;
        }
        if koff_event != 0 {
            level = ssg_level;
        }
        let eg_off: u8 = if self.eg_ssg_enable[slot] != 0 {
            ((level as i32) >> 9) as u8
        } else {
            ((level as i32 & 0x3f0) == 0x3f0) as u8
        };
        let mut nextlevel = level;
        if kon_event != 0 {
            nextstate = EG_NUM_ATTACK;
            /* Instant attack */
            if self.eg_ratemax != 0 {
                nextlevel = 0;
            } else if self.eg_state[slot] == EG_NUM_ATTACK
                && level != 0
                && self.eg_inc != 0
                && nkon != 0
            {
                inc = (((!(level as i32)) << self.eg_inc) >> 5) as i16;
            }
        } else {
            match self.eg_state[slot] {
                EG_NUM_ATTACK => {
                    if level == 0 {
                        nextstate = EG_NUM_DECAY;
                    } else if self.eg_inc != 0 && self.eg_ratemax == 0 && nkon != 0 {
                        inc = (((!(level as i32)) << self.eg_inc) >> 5) as i16;
                    }
                }
                EG_NUM_DECAY => {
                    if (level as i32 >> 5) == self.eg_sl[1] as i32 {
                        nextstate = EG_NUM_SUSTAIN;
                    } else if eg_off == 0 && self.eg_inc != 0 {
                        inc = (1i32 << (self.eg_inc - 1)) as i16;
                        if self.eg_ssg_enable[slot] != 0 {
                            inc = ((inc as i32) << 2) as i16;
                        }
                    }
                }
                EG_NUM_SUSTAIN | EG_NUM_RELEASE if eg_off == 0 && self.eg_inc != 0 => {
                    inc = (1i32 << (self.eg_inc - 1)) as i16;
                    if self.eg_ssg_enable[slot] != 0 {
                        inc = ((inc as i32) << 2) as i16;
                    }
                }
                _ => {}
            }
            if nkon == 0 {
                nextstate = EG_NUM_RELEASE;
            }
        }
        if self.eg_kon_csm[slot] != 0 {
            nextlevel = (nextlevel as i32 | ((self.eg_tl[1] as i32) << 3)) as i16;
        }

        /* Envelope off */
        if kon_event == 0
            && self.eg_ssg_hold_up_latch[slot] == 0
            && self.eg_state[slot] != EG_NUM_ATTACK
            && eg_off != 0
        {
            nextstate = EG_NUM_RELEASE;
            nextlevel = 0x3ff;
        }

        nextlevel = (nextlevel as i32).wrapping_add(inc as i32) as i16;

        self.eg_kon[slot] = self.eg_kon_latch[slot];
        self.eg_level[slot] = (nextlevel as u16) & 0x3ff;
        self.eg_state[slot] = nextstate;
    }

    /// C++ `OPN2_EnvelopePrepare`
    fn envelope_prepare(&mut self) {
        let mut inc: u8 = 0;
        let slot = self.cycles as usize;

        /* Prepare increment */
        let mut rate = (((self.eg_rate as u32) << 1).wrapping_add(self.eg_ksv as u32)) as u8;

        if rate > 0x3f {
            rate = 0x3f;
        }

        let sum = ((rate >> 2).wrapping_add(self.eg_shift_lock)) & 0x0f;
        if self.eg_rate != 0 && self.eg_quotient == 2 {
            if rate < 48 {
                match sum {
                    12 => {
                        inc = 1;
                    }
                    13 => {
                        inc = (rate >> 1) & 0x01;
                    }
                    14 => {
                        inc = rate & 0x01;
                    }
                    _ => {}
                }
            } else {
                inc = (EG_STEPHI[(rate & 0x03) as usize][self.eg_timer_low_lock as usize] as i32
                    + (rate as i32 >> 2)
                    - 11) as u8;
                if inc > 4 {
                    inc = 4;
                }
            }
        }
        self.eg_inc = inc;
        self.eg_ratemax = ((rate >> 1) == 0x1f) as u8;

        /* Prepare rate & ksv */
        let mut rate_sel = self.eg_state[slot];
        if (self.eg_kon[slot] != 0 && self.eg_ssg_repeat_latch[slot] != 0)
            || (self.eg_kon[slot] == 0 && self.eg_kon_latch[slot] != 0)
        {
            rate_sel = EG_NUM_ATTACK;
        }
        match rate_sel {
            EG_NUM_ATTACK => {
                self.eg_rate = self.ar[slot];
            }
            EG_NUM_DECAY => {
                self.eg_rate = self.dr[slot];
            }
            EG_NUM_SUSTAIN => {
                self.eg_rate = self.sr[slot];
            }
            EG_NUM_RELEASE => {
                self.eg_rate = (self.rr[slot] << 1) | 0x01;
            }
            _ => {}
        }
        self.eg_ksv = self.pg_kcode >> (self.ks[slot] ^ 0x03);
        if self.am[slot] != 0 {
            self.eg_lfo_am = self.lfo_am >> EG_AM_SHIFT[self.ams[self.channel as usize] as usize];
        } else {
            self.eg_lfo_am = 0;
        }
        /* Delay TL & SL value */
        self.eg_tl[1] = self.eg_tl[0];
        self.eg_tl[0] = self.tl[slot];
        self.eg_sl[1] = self.eg_sl[0];
        self.eg_sl[0] = self.sl[slot];
    }

    /// C++ `OPN2_EnvelopeGenerate`
    fn envelope_generate(&mut self) {
        let slot = ((self.cycles + 23) % 24) as usize;

        let mut level: u16 = self.eg_level[slot];

        if self.eg_ssg_inv[slot] != 0 {
            /* Inverse */
            level = (512i32.wrapping_sub(level as i32)) as u16;
        }
        if self.mode_test_21[5] != 0 {
            level = 0;
        }
        level &= 0x3ff;

        /* Apply AM LFO */
        level = level.wrapping_add(self.eg_lfo_am as u16);

        /* Apply TL */
        if !(self.mode_csm != 0 && self.channel == 2 + 1) {
            level = level.wrapping_add((self.eg_tl[0] as u16) << 3);
        }
        if level > 0x3ff {
            level = 0x3ff;
        }
        self.eg_out[slot] = level;
    }

    /// C++ `OPN2_UpdateLFO`
    fn update_lfo(&mut self) {
        if (self.lfo_quotient as u32 & LFO_CYCLES[self.lfo_freq as usize])
            == LFO_CYCLES[self.lfo_freq as usize]
        {
            self.lfo_quotient = 0;
            self.lfo_cnt = self.lfo_cnt.wrapping_add(1);
        } else {
            self.lfo_quotient = self.lfo_quotient.wrapping_add(self.lfo_inc);
        }
        self.lfo_cnt &= self.lfo_en;
    }

    /// C++ `OPN2_FMPrepare`
    fn fm_prepare(&mut self) {
        let mut slot = ((self.cycles + 6) % 24) as usize;
        let channel = self.channel as usize;
        let op = slot / 6;
        let connect = self.connect[channel] as usize;
        let prevslot = ((self.cycles + 18) % 24) as usize;

        /* Calculate modulation */
        let mut mod1: i16 = 0;
        let mut mod2: i16 = 0;

        if FM_ALGORITHM[op][0][connect] != 0 {
            mod2 |= self.fm_op1[channel][0];
        }
        if FM_ALGORITHM[op][1][connect] != 0 {
            mod1 |= self.fm_op1[channel][1];
        }
        if FM_ALGORITHM[op][2][connect] != 0 {
            mod1 |= self.fm_op2[channel];
        }
        if FM_ALGORITHM[op][3][connect] != 0 {
            mod2 |= self.fm_out[prevslot];
        }
        if FM_ALGORITHM[op][4][connect] != 0 {
            mod1 |= self.fm_out[prevslot];
        }
        let mut fmod = (mod1 as i32).wrapping_add(mod2 as i32) as i16;
        if op == 0 {
            /* Feedback */
            fmod = (fmod as i32 >> (10 - self.fb[channel] as i32)) as i16;
            if self.fb[channel] == 0 {
                fmod = 0;
            }
        } else {
            fmod = (fmod as i32 >> 1) as i16;
        }
        self.fm_mod[slot] = fmod as u16;

        slot = ((self.cycles + 18) % 24) as usize;
        /* OP1 */
        if slot / 6 == 0 {
            self.fm_op1[channel][1] = self.fm_op1[channel][0];
            self.fm_op1[channel][0] = self.fm_out[slot];
        }
        /* OP2 */
        if slot / 6 == 2 {
            self.fm_op2[channel] = self.fm_out[slot];
        }
    }

    /// C++ `OPN2_ChGenerate`
    fn ch_generate(&mut self) {
        let slot = ((self.cycles + 18) % 24) as usize;
        let channel = self.channel as usize;
        let op = slot / 6;
        let test_dac = self.mode_test_2c[5] as u32;
        let mut acc = self.ch_acc[channel];
        let mut add = test_dac as i16;
        if op == 0 && test_dac == 0 {
            acc = 0;
        }
        if FM_ALGORITHM[op][5][self.connect[channel] as usize] != 0 && test_dac == 0 {
            add = (add as i32).wrapping_add(self.fm_out[slot] as i32 >> 5) as i16;
        }
        let mut sum = (acc as i32).wrapping_add(add as i32) as i16;
        /* Clamp */
        sum = sum.clamp(-256, 255);

        if op == 0 || test_dac != 0 {
            self.ch_out[channel] = self.ch_acc[channel];
        }
        self.ch_acc[channel] = sum;
    }

    /// C++ `OPN2_ChOutput`
    fn ch_output(&mut self) {
        let cycles = self.cycles;
        let slot = self.cycles;
        let mut channel = self.channel as usize;
        let test_dac = self.mode_test_2c[5] as u32;
        self.ch_read = self.ch_lock;
        if slot < 12 {
            /* Ch 4,5,6 */
            channel += 1;
        }
        if (cycles & 3) == 0 {
            if test_dac == 0 {
                /* Lock value */
                self.ch_lock = self.ch_out[channel];
            }
            self.ch_lock_l = self.pan_l[channel];
            self.ch_lock_r = self.pan_r[channel];
        }
        /* Ch 6 */
        let mut out: i16;
        if ((cycles >> 2) == 1 && self.dacen != 0) || test_dac != 0 {
            out = self.dacdata;
            out = ((out as i32) << 7) as i16;
            out >>= 7;
        } else {
            out = self.ch_lock;
        }
        self.mol = 0;
        self.mor = 0;

        if CHIP_TYPE == YM3438_TYPE_YM2612 {
            let out_en = ((cycles & 3) == 3) || test_dac != 0;
            /* YM2612 DAC emulation(not verified) */
            let mut sign = out >> 8;
            if out >= 0 {
                out += 1;
                sign += 1;
            }
            if self.ch_lock_l != 0 && out_en {
                self.mol = out;
            } else {
                self.mol = sign;
            }
            if self.ch_lock_r != 0 && out_en {
                self.mor = out;
            } else {
                self.mor = sign;
            }
            /* Amplify signal */
            self.mol = (self.mol as i32 * 3) as i16;
            self.mor = (self.mor as i32 * 3) as i16;
        } else {
            let out_en = ((cycles & 3) != 0) || test_dac != 0;
            /* Discrete YM3438 seems has the ladder effect too */
            if out >= 0 && CHIP_TYPE == YM3438_TYPE_DISCRETE {
                out += 1;
            }
            if self.ch_lock_l != 0 && out_en {
                self.mol = out;
            }
            if self.ch_lock_r != 0 && out_en {
                self.mor = out;
            }
        }
    }

    /// C++ `OPN2_FMGenerate`
    fn fm_generate(&mut self) {
        let slot = ((self.cycles + 19) % 24) as usize;
        /* Calculate phase */
        let phase: u16 =
            ((self.fm_mod[slot] as u32).wrapping_add(self.pg_phase[slot] >> 10) & 0x3ff) as u16;
        let quarter: u16 = if phase & 0x100 != 0 {
            (phase ^ 0xff) & 0xff
        } else {
            phase & 0xff
        };
        let mut level: u16 = LOGSINROM[quarter as usize];
        /* Apply envelope */
        level = level.wrapping_add(self.eg_out[slot] << 2);
        /* Transform */
        if level > 0x1fff {
            level = 0x1fff;
        }
        let mut output: i16 = ((((EXPROM[((level & 0xff) ^ 0xff) as usize] as i32) | 0x400) << 2)
            >> (level >> 8)) as i16;
        if phase & 0x200 != 0 {
            output = (((!(output as i32)) ^ ((self.mode_test_21[4] as i32) << 13)).wrapping_add(1))
                as i16;
        } else {
            output = ((output as i32) ^ ((self.mode_test_21[4] as i32) << 13)) as i16;
        }
        output = (((output as i32) << 2) as i16) >> 2;
        self.fm_out[slot] = output;
    }

    /// C++ `OPN2_DoTimerA`
    fn do_timer_a(&mut self) {
        let mut load = self.timer_a_overflow;
        if self.cycles == 2 {
            /* Lock load value */
            load |= (self.timer_a_load_lock == 0 && self.timer_a_load != 0) as u8;
            self.timer_a_load_lock = self.timer_a_load;
            if self.mode_csm != 0 {
                /* CSM KeyOn */
                self.mode_kon_csm = load;
            } else {
                self.mode_kon_csm = 0;
            }
        }
        /* Load counter */
        let mut time: u16 = if self.timer_a_load_latch != 0 {
            self.timer_a_reg
        } else {
            self.timer_a_cnt
        };
        self.timer_a_load_latch = load;
        /* Increase counter */
        if (self.cycles == 1 && self.timer_a_load_lock != 0) || self.mode_test_21[2] != 0 {
            time = time.wrapping_add(1);
        }
        /* Set overflow flag */
        if self.timer_a_reset != 0 {
            self.timer_a_reset = 0;
            self.timer_a_overflow_flag = 0;
        } else {
            self.timer_a_overflow_flag |= self.timer_a_overflow & self.timer_a_enable;
        }
        self.timer_a_overflow = (time >> 10) as u8;
        self.timer_a_cnt = time & 0x3ff;
    }

    /// C++ `OPN2_DoTimerB`
    fn do_timer_b(&mut self) {
        let mut load = self.timer_b_overflow;
        if self.cycles == 2 {
            /* Lock load value */
            load |= (self.timer_b_load_lock == 0 && self.timer_b_load != 0) as u8;
            self.timer_b_load_lock = self.timer_b_load;
        }
        /* Load counter */
        let mut time: u16 = if self.timer_b_load_latch != 0 {
            self.timer_b_reg
        } else {
            self.timer_b_cnt
        };
        self.timer_b_load_latch = load;
        /* Increase counter */
        if self.cycles == 1 {
            self.timer_b_subcnt = self.timer_b_subcnt.wrapping_add(1);
        }
        if (self.timer_b_subcnt == 0x10 && self.timer_b_load_lock != 0) || self.mode_test_21[2] != 0
        {
            time = time.wrapping_add(1);
        }
        self.timer_b_subcnt &= 0x0f;
        /* Set overflow flag */
        if self.timer_b_reset != 0 {
            self.timer_b_reset = 0;
            self.timer_b_overflow_flag = 0;
        } else {
            self.timer_b_overflow_flag |= self.timer_b_overflow & self.timer_b_enable;
        }
        self.timer_b_overflow = (time >> 8) as u8;
        self.timer_b_cnt = time & 0xff;
    }

    /// C++ `OPN2_KeyOn`
    fn key_on(&mut self) {
        let slot = self.cycles as usize;
        let chan = self.channel as usize;
        /* Key On */
        self.eg_kon_latch[slot] = self.mode_kon[slot];
        self.eg_kon_csm[slot] = 0;
        if self.channel == 2 && self.mode_kon_csm != 0 {
            /* CSM Key On */
            self.eg_kon_latch[slot] = 1;
            self.eg_kon_csm[slot] = 1;
        }
        if self.cycles == self.mode_kon_channel as u32 {
            /* OP1 */
            self.mode_kon[chan] = self.mode_kon_operator[0];
            /* OP2 */
            self.mode_kon[chan + 12] = self.mode_kon_operator[1];
            /* OP3 */
            self.mode_kon[chan + 6] = self.mode_kon_operator[2];
            /* OP4 */
            self.mode_kon[chan + 18] = self.mode_kon_operator[3];
        }
    }

    /// C++ `OPN2_Reset`
    fn reset(&mut self, rate: u32, clock: u32) {
        let rateratio = self.rateratio as u32;
        *self = Self::zeroed();
        for i in 0..24 {
            self.eg_out[i] = 0x3ff;
            self.eg_level[i] = 0x3ff;
            self.eg_state[i] = EG_NUM_RELEASE;
            self.multi[i] = 1;
        }
        for i in 0..6 {
            self.pan_l[i] = 1;
            self.pan_r[i] = 1;
        }

        if rate != 0 {
            self.rateratio = ((((144u64 * rate as u64) << RSM_FRAC) / clock as u64) as u32) as i32;
        } else {
            self.rateratio = rateratio as i32;
        }
    }

    /// C++ `OPN2_Clock`
    fn clock(&mut self, buffer: &mut [i16; 2]) {
        let slot = self.cycles;
        self.lfo_inc = self.mode_test_21[1];
        self.pg_read >>= 1;
        self.eg_read[1] >>= 1;
        self.eg_cycle = self.eg_cycle.wrapping_add(1);
        /* Lock envelope generator timer value */
        if self.cycles == 1 && self.eg_quotient == 2 {
            if self.eg_cycle_stop != 0 {
                self.eg_shift_lock = 0;
            } else {
                self.eg_shift_lock = self.eg_shift + 1;
            }
            self.eg_timer_low_lock = (self.eg_timer & 0x03) as u8;
        }
        /* Cycle specific functions */
        match self.cycles {
            0 => {
                self.lfo_pm = self.lfo_cnt >> 2;
                if self.lfo_cnt & 0x40 != 0 {
                    self.lfo_am = self.lfo_cnt & 0x3f;
                } else {
                    self.lfo_am = self.lfo_cnt ^ 0x3f;
                }
                self.lfo_am <<= 1;
            }
            1 => {
                self.eg_quotient = self.eg_quotient.wrapping_add(1);
                self.eg_quotient %= 3;
                self.eg_cycle = 0;
                self.eg_cycle_stop = 1;
                self.eg_shift = 0;
                self.eg_timer_inc |= (self.eg_quotient >> 1) as u8;
                self.eg_timer = self.eg_timer.wrapping_add(self.eg_timer_inc as u16);
                self.eg_timer_inc = (self.eg_timer >> 12) as u8;
                self.eg_timer &= 0xfff;
            }
            2 => {
                self.pg_read = self.pg_phase[21] & 0x3ff;
                self.eg_read[1] = self.eg_out[0] as u32;
            }
            13 => {
                self.eg_cycle = 0;
                self.eg_cycle_stop = 1;
                self.eg_shift = 0;
                self.eg_timer = self.eg_timer.wrapping_add(self.eg_timer_inc as u16);
                self.eg_timer_inc = (self.eg_timer >> 12) as u8;
                self.eg_timer &= 0xfff;
            }
            23 => {
                self.lfo_inc |= 1;
            }
            _ => {}
        }
        self.eg_timer =
            (self.eg_timer as u32 & !((self.mode_test_21[5] as u32) << self.eg_cycle)) as u16;
        if (((self.eg_timer as u32 >> self.eg_cycle)
            | (self.pin_test_in & self.eg_custom_timer) as u32)
            & self.eg_cycle_stop as u32)
            != 0
        {
            self.eg_shift = self.eg_cycle;
            self.eg_cycle_stop = 0;
        }

        self.do_io();

        self.do_timer_a();
        self.do_timer_b();
        self.key_on();

        self.ch_output();
        self.ch_generate();

        self.fm_prepare();
        self.fm_generate();

        self.phase_generate();
        self.phase_calc_increment();

        self.envelope_adsr();
        self.envelope_generate();
        self.envelope_ssg_eg();
        self.envelope_prepare();

        /* Prepare fnum & block */
        if self.mode_ch3 != 0 {
            /* Channel 3 special mode */
            match slot {
                1 => {
                    /* OP1 */
                    self.pg_fnum = self.fnum_3ch[1];
                    self.pg_block = self.block_3ch[1];
                    self.pg_kcode = self.kcode_3ch[1];
                }
                7 => {
                    /* OP3 */
                    self.pg_fnum = self.fnum_3ch[0];
                    self.pg_block = self.block_3ch[0];
                    self.pg_kcode = self.kcode_3ch[0];
                }
                13 => {
                    /* OP2 */
                    self.pg_fnum = self.fnum_3ch[2];
                    self.pg_block = self.block_3ch[2];
                    self.pg_kcode = self.kcode_3ch[2];
                }
                _ => {
                    /* OP4 (case 19) and default */
                    self.pg_fnum = self.fnum[((self.channel + 1) % 6) as usize];
                    self.pg_block = self.block[((self.channel + 1) % 6) as usize];
                    self.pg_kcode = self.kcode[((self.channel + 1) % 6) as usize];
                }
            }
        } else {
            self.pg_fnum = self.fnum[((self.channel + 1) % 6) as usize];
            self.pg_block = self.block[((self.channel + 1) % 6) as usize];
            self.pg_kcode = self.kcode[((self.channel + 1) % 6) as usize];
        }

        self.update_lfo();
        self.do_reg_write();
        self.cycles = (self.cycles + 1) % 24;
        self.channel = self.cycles % 6;

        buffer[0] = self.mol;
        buffer[1] = self.mor;
    }

    /// C++ `OPN2_Write`
    fn write(&mut self, port: u32, data: u8) {
        let port = port & 3;
        self.write_data = (((port << 7) & 0x100) | data as u32) as u16;
        if port & 1 != 0 {
            /* Data */
            self.write_d |= 1;
        } else {
            /* Address */
            self.write_a |= 1;
        }
    }

    /// C++ `OPN2_WriteBuffered`
    fn write_buffered(&mut self, port: u32, data: u8) {
        let mut buffer = [0i16; 2];

        let last = self.writebuf_last as usize;
        if self.writebuf[last].port & 0x04 != 0 {
            let (p, d) = (self.writebuf[last].port & 0x03, self.writebuf[last].data);
            self.write(p as u32, d);

            self.writebuf_cur = ((last + 1) % OPN_WRITEBUF_SIZE) as u32;
            let mut skip = self.writebuf[last]
                .time
                .wrapping_sub(self.writebuf_samplecnt);
            self.writebuf_samplecnt = self.writebuf[last].time;
            while skip > 0 {
                skip -= 1;
                self.clock(&mut buffer);
            }
        }

        let last = self.writebuf_last as usize;
        self.writebuf[last].port = ((port & 0x03) | 0x04) as u8;
        self.writebuf[last].data = data;
        let mut time1 = self.writebuf_lasttime + OPN_WRITEBUF_DELAY;
        let time2 = self.writebuf_samplecnt;

        if time1 < time2 {
            time1 = time2;
        }

        self.writebuf[last].time = time1;
        self.writebuf_lasttime = time1;
        self.writebuf_last = ((last + 1) % OPN_WRITEBUF_SIZE) as u32;
    }

    /// C++ `OPN2_Generate`
    fn generate(&mut self, buf: &mut [i16; 2]) {
        let mut buffer = [0i16; 2];

        buf[0] = 0;
        buf[1] = 0;

        for _ in 0..24 {
            let mute = match self.cycles >> 2 {
                0 => self.mute[1],                         /* Ch 2 */
                1 => self.mute[(5 + self.dacen) as usize], /* Ch 6, DAC */
                2 => self.mute[3],                         /* Ch 4 */
                3 => self.mute[0],                         /* Ch 1 */
                4 => self.mute[4],                         /* Ch 5 */
                5 => self.mute[2],                         /* Ch 3 */
                _ => 0,
            };
            self.clock(&mut buffer);
            if mute == 0 {
                buf[0] = buf[0].wrapping_add(buffer[0]);
                buf[1] = buf[1].wrapping_add(buffer[1]);
            }

            while self.writebuf[self.writebuf_cur as usize].time <= self.writebuf_samplecnt {
                if self.writebuf[self.writebuf_cur as usize].port & 0x04 == 0 {
                    break;
                }
                self.writebuf[self.writebuf_cur as usize].port &= 0x03;
                let (p, d) = (
                    self.writebuf[self.writebuf_cur as usize].port,
                    self.writebuf[self.writebuf_cur as usize].data,
                );
                self.write(p as u32, d);
                self.writebuf_cur = (self.writebuf_cur + 1) % OPN_WRITEBUF_SIZE as u32;
            }
            self.writebuf_samplecnt = self.writebuf_samplecnt.wrapping_add(1);
        }
    }

    /// C++ `OPN2_GenerateResampled`
    fn generate_resampled(&mut self, buf: &mut [i16; 2]) {
        let mut buffer = [0i16; 2];

        while self.samplecnt >= self.rateratio {
            self.oldsamples[0] = self.samples[0];
            self.oldsamples[1] = self.samples[1];
            self.generate(&mut buffer);
            self.samples[0] = buffer[0] as i32 * 11;
            self.samples[1] = buffer[1] as i32 * 11;
            self.samplecnt = self.samplecnt.wrapping_sub(self.rateratio);
        }
        buf[0] = ((self.oldsamples[0]
            .wrapping_mul(self.rateratio.wrapping_sub(self.samplecnt))
            .wrapping_add(self.samples[0].wrapping_mul(self.samplecnt))
            / self.rateratio)
            >> 1) as i16;
        buf[1] = ((self.oldsamples[1]
            .wrapping_mul(self.rateratio.wrapping_sub(self.samplecnt))
            .wrapping_add(self.samples[1].wrapping_mul(self.samplecnt))
            / self.rateratio)
            >> 1) as i16;
        self.samplecnt = self.samplecnt.wrapping_add(1 << RSM_FRAC);
    }

    /// C++ `OPN2_GenerateStreamMix`
    fn generate_stream_mix(&mut self, output: &mut [i16], numsamples: u32) {
        let mut buffer = [0i16; 2];

        for i in 0..numsamples as usize {
            self.generate_resampled(&mut buffer);
            output[i * 2] = output[i * 2].wrapping_add(buffer[0]);
            output[i * 2 + 1] = output[i * 2 + 1].wrapping_add(buffer[1]);
        }
    }

    /// C++ `OPN2_SetMute`
    fn set_mute(&mut self, mute: u32) {
        for (i, voice) in self.mute.iter_mut().enumerate() {
            *voice = (mute >> i) & 0x01;
        }
    }
}

/// C++ `Ym2612_Nuked_Emu`
pub struct Ym2612NukedEmu {
    chip: Box<Ym3438>,
    prev_sample_rate: f64,
    prev_clock_rate: f64,
}

impl Default for Ym2612NukedEmu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ym2612NukedEmu {
    pub const OUT_CHAN_COUNT: usize = 2; // stereo

    pub fn new() -> Self {
        // C++ ctor: OPN2_SetChipType(ym3438_type_asic) — see CHIP_TYPE above.
        Self {
            chip: Box::new(Ym3438::zeroed()),
            prev_sample_rate: 0.0,
            prev_clock_rate: 0.0,
        }
    }

    /// C++ `Ym2612_Nuked_Emu::set_rate`
    pub fn set_rate(&mut self, sample_rate: f64, clock_rate: f64) {
        self.prev_sample_rate = sample_rate;
        self.prev_clock_rate = clock_rate;
        self.chip.reset(sample_rate as u32, clock_rate as u32);
    }

    /// C++ `Ym2612_Nuked_Emu::reset`
    pub fn reset(&mut self) {
        self.chip
            .reset(self.prev_sample_rate as u32, self.prev_clock_rate as u32);
    }

    /// C++ `Ym2612_Nuked_Emu::mute_voices`
    pub fn mute_voices(&mut self, mask: i32) {
        self.chip.set_mute(mask as u32);
    }

    /// C++ `Ym2612_Nuked_Emu::write0`
    pub fn write0(&mut self, addr: i32, data: i32) {
        self.chip.write_buffered(0, addr as u8);
        self.chip.write_buffered(1, data as u8);
    }

    /// C++ `Ym2612_Nuked_Emu::write1`
    pub fn write1(&mut self, addr: i32, data: i32) {
        self.chip.write_buffered(2, addr as u8);
        self.chip.write_buffered(3, data as u8);
    }

    /// C++ `Ym2612_Nuked_Emu::run`: adds `pair_count` stereo sample pairs into
    /// the current contents of `out`.
    pub fn run(&mut self, pair_count: i32, out: &mut [i16]) {
        self.chip.generate_stream_mix(out, pair_count as u32);
    }
}
