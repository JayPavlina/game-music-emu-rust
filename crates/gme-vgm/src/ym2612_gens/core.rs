// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2002 Stéphane Dallongeville and 2004–2006 Shay Green.
// Adapted in Rust from Game Music Emu's Ym2612_GENS.cpp on November 27, 2026.
// Distributed without warranty. See COPYING.LESSER.
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
#[derive(Copy, Clone)]
#[repr(C)]
pub struct slot_t {
    pub DT: *const i32,
    pub MUL: i32,
    pub TL: i32,
    pub TLL: i32,
    pub SLL: i32,
    pub KSR_S: i32,
    pub KSR: i32,
    pub SEG: i32,
    pub env_xor: i32,
    pub env_max: i32,
    pub AR: *const i32,
    pub DR: *const i32,
    pub SR: *const i32,
    pub RR: *const i32,
    pub Fcnt: i32,
    pub Finc: i32,
    pub Ecurp: i32,
    pub Ecnt: i32,
    pub Einc: i32,
    pub Ecmp: i32,
    pub EincA: i32,
    pub EincD: i32,
    pub EincS: i32,
    pub EincR: i32,
    pub OUTp: *mut i32,
    pub INd: i32,
    pub ChgEnM: i32,
    pub AMS: i32,
    pub AMSon: i32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct channel_t {
    pub S0_OUT: [i32; 4],
    pub LEFT: i32,
    pub RIGHT: i32,
    pub ALGO: i32,
    pub FB: i32,
    pub FMS: i32,
    pub AMS: i32,
    pub FNUM: [i32; 4],
    pub FOCT: [i32; 4],
    pub KC: [i32; 4],
    pub SLOT: [slot_t; 4],
    pub FFlag: i32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct state_t {
    pub TimerBase: i32,
    pub Status: i32,
    pub TimerA: i32,
    pub TimerAL: i32,
    pub TimerAcnt: i32,
    pub TimerB: i32,
    pub TimerBL: i32,
    pub TimerBcnt: i32,
    pub Mode: i32,
    pub DAC: i32,
    pub CHANNEL: [channel_t; 6],
    pub REG: [[i32; 256]; 2],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct tables_t {
    pub SIN_TAB: [i16; 4096],
    pub LFOcnt: i32,
    pub LFOinc: i32,
    pub AR_TAB: [u32; 128],
    pub DR_TAB: [u32; 96],
    pub DT_TAB: [[u32; 32]; 8],
    pub SL_TAB: [u32; 16],
    pub NULL_RATE: [u32; 32],
    pub LFO_INC_TAB: [i32; 8],
    pub ENV_TAB: [i16; 8200],
    pub LFO_ENV_TAB: [i16; 1024],
    pub LFO_FREQ_TAB: [i16; 1024],
    pub TL_TAB: [i32; 24576],
    pub DECAY_TO_ATTACK: [u32; 4096],
    pub FINC_TAB: [u32; 2048],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct Ym2612_GENS_Impl {
    pub YM2612: state_t,
    pub mute_mask: i32,
    pub g: tables_t,
}
pub type Chip = Ym2612_GENS_Impl;
pub const INT_MAX: i32 = 2147483647_i32;
pub const channel_count: i32 = 6_i32;
const output_bits: i32 = 14_i32;
pub const PI: f64 = 3.14159265358979323846f64;
pub const ATTACK: i32 = 0_i32;
pub const DECAY: i32 = 1_i32;
pub const SUBSTAIN: i32 = 2_i32;
pub const RELEASE: i32 = 3_i32;
pub const SIN_HBITS: i32 = 12_i32;
pub const SIN_LBITS: i32 = 26_i32 - SIN_HBITS;
pub const ENV_HBITS: i32 = 12_i32;
pub const ENV_LBITS: i32 = 28_i32 - ENV_HBITS;
pub const LFO_HBITS: i32 = 10_i32;
pub const LFO_LBITS: i32 = 28_i32 - LFO_HBITS;
pub const SIN_LENGHT: i32 = (1_i32) << SIN_HBITS;
pub const ENV_LENGHT: i32 = (1_i32) << ENV_HBITS;
pub const LFO_LENGHT: i32 = (1_i32) << LFO_HBITS;
pub const TL_LENGHT: i32 = ENV_LENGHT * 3_i32;
pub const ENV_MASK: i32 = ENV_LENGHT - 1_i32;
pub const LFO_MASK: i32 = LFO_LENGHT - 1_i32;
pub const ENV_STEP: f64 = 96.0f64 / ENV_LENGHT as f64;
pub const ENV_ATTACK: i32 = (ENV_LENGHT * 0_i32) << ENV_LBITS;
pub const ENV_DECAY: i32 = (ENV_LENGHT * 1_i32) << ENV_LBITS;
pub const ENV_END: i32 = (ENV_LENGHT * 2_i32) << ENV_LBITS;
pub const MAX_OUT_BITS: i32 = SIN_HBITS + SIN_LBITS + 2_i32;
pub const MAX_OUT: i32 = ((1_i32) << MAX_OUT_BITS) - 1_i32;
pub const PG_CUT_OFF: i32 = (78.0f64 / ENV_STEP) as i32;
pub const AR_RATE: i32 = 399128_i32;
pub const DR_RATE: i32 = 5514396_i32;
pub const LFO_FMS_LBITS: i32 = 9_i32;
pub const LFO_FMS_BASE: i32 =
    (0.05946309436f64 * 0.0338f64 * ((1_i32) << LFO_FMS_LBITS) as f64) as i32;
pub const S0: i32 = 0_i32;
pub const S1: i32 = 2_i32;
pub const S2: i32 = 1_i32;
pub const S3: i32 = 3_i32;
#[inline]
unsafe fn set_seg(mut s: *mut slot_t, mut seg: i32) {
    (*s).env_xor = 0_i32;
    (*s).env_max = INT_MAX;
    (*s).SEG = seg;
    if seg & 4_i32 != 0 {
        (*s).env_xor = ENV_MASK;
        (*s).env_max = ENV_MASK;
    }
}
const DT_DEF_TAB: [u8; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 8, 8, 8, 8,
    1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 8, 8, 9, 10, 11, 12, 13, 14, 16,
    16, 16, 16, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6, 7, 8, 8, 9, 10, 11, 12, 13, 14, 16,
    17, 19, 20, 22, 22, 22, 22,
];
const FKEY_TAB: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 3, 3, 3, 3, 3, 3];
const LFO_AMS_TAB: [u8; 4] = [31_i32 as u8, 4_i32 as u8, 1_i32 as u8, 0_i32 as u8];
const LFO_FMS_TAB: [u8; 8] = [
    (LFO_FMS_BASE * 0_i32) as u8,
    (LFO_FMS_BASE * 1_i32) as u8,
    (LFO_FMS_BASE * 2_i32) as u8,
    (LFO_FMS_BASE * 3_i32) as u8,
    (LFO_FMS_BASE * 4_i32) as u8,
    (LFO_FMS_BASE * 6_i32) as u8,
    (LFO_FMS_BASE * 12_i32) as u8,
    (LFO_FMS_BASE * 24_i32) as u8,
];
#[inline]
unsafe fn YM2612_Special_Update() {}
pub unsafe fn gens_KEY_ON(mut chip: *mut Chip, mut ch: *mut channel_t, mut nsl: i32) {
    let mut SL: *mut slot_t =
        (&raw mut (*ch).SLOT as *mut slot_t).offset(nsl as isize) as *mut slot_t;
    if (*SL).Ecurp == RELEASE {
        (*SL).Fcnt = 0_i32;
        (*SL).Ecnt = ((*chip).g.DECAY_TO_ATTACK
            [(*chip).g.ENV_TAB[((*SL).Ecnt >> ENV_LBITS) as usize] as usize]
            .wrapping_add(ENV_ATTACK as u32)
            & (*SL).ChgEnM as u32) as i32;
        (*SL).ChgEnM = !(0_i32);
        (*SL).Einc = (*SL).EincA;
        (*SL).Ecmp = ENV_DECAY;
        (*SL).Ecurp = ATTACK;
    }
}
pub unsafe fn gens_KEY_OFF(mut chip: *mut Chip, mut ch: *mut channel_t, mut nsl: i32) {
    let mut SL: *mut slot_t =
        (&raw mut (*ch).SLOT as *mut slot_t).offset(nsl as isize) as *mut slot_t;
    if (*SL).Ecurp != RELEASE {
        if (*SL).Ecnt < ENV_DECAY {
            (*SL).Ecnt = (((*chip).g.ENV_TAB[((*SL).Ecnt >> ENV_LBITS) as usize] as i32)
                << ENV_LBITS)
                + ENV_DECAY;
        }
        (*SL).Einc = (*SL).EincR;
        (*SL).Ecmp = ENV_END;
        (*SL).Ecurp = RELEASE;
    }
}
pub unsafe fn gens_SLOT_SET(mut chip: *mut Chip, mut Adr: i32, mut data: i32) -> i32 {
    let mut nch = Adr & 3_i32;
    if nch == 3_i32 {
        return 1_i32;
    }
    let mut ch: *mut channel_t = (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t)
        .offset((nch + (if Adr & 0x100_i32 != 0 { 3_i32 } else { 0_i32 })) as isize)
        as *mut channel_t;
    let mut sl: *mut slot_t =
        (&raw mut (*ch).SLOT as *mut slot_t).offset((Adr >> 2_i32 & 3_i32) as isize) as *mut slot_t;
    match Adr & 0xf0_i32 {
        48 => {
            (*sl).MUL = data & 0xf_i32;
            if (*sl).MUL != 0_i32 {
                (*sl).MUL <<= 1_i32;
            } else {
                (*sl).MUL = 1_i32;
            }
            (*sl).DT = &raw mut *(&raw mut (*chip).g.DT_TAB as *mut [u32; 32])
                .offset((data >> 4_i32 & 7_i32) as isize) as *mut u32
                as *mut i32;
            (*ch).SLOT[0].Finc = -(1_i32);
        }
        64 => {
            (*sl).TL = data & 0x7f_i32;
            YM2612_Special_Update();
            (*sl).TLL = (*sl).TL << ENV_HBITS - 7_i32;
        }
        80 => {
            (*sl).KSR_S = 3_i32 - (data >> 6_i32);
            (*ch).SLOT[0].Finc = -(1_i32);
            data &= 0x1f_i32;
            if data != 0 {
                (*sl).AR = (&raw mut (*chip).g.AR_TAB as *mut u32).offset((data << 1_i32) as isize)
                    as *mut u32 as *mut i32;
            } else {
                (*sl).AR =
                    (&raw mut (*chip).g.NULL_RATE as *mut u32).add(0) as *mut u32 as *mut i32;
            }
            (*sl).EincA = *(*sl).AR.offset((*sl).KSR as isize);
            if (*sl).Ecurp == ATTACK {
                (*sl).Einc = (*sl).EincA;
            }
        }
        96 => {
            (*sl).AMSon = data & 0x80_i32;
            if (*sl).AMSon != 0_i32 {
                (*sl).AMS = (*ch).AMS;
            } else {
                (*sl).AMS = 31_i32;
            }
            data &= 0x1f_i32;
            if data != 0 {
                (*sl).DR = (&raw mut (*chip).g.DR_TAB as *mut u32).offset((data << 1_i32) as isize)
                    as *mut u32 as *mut i32;
            } else {
                (*sl).DR =
                    (&raw mut (*chip).g.NULL_RATE as *mut u32).add(0) as *mut u32 as *mut i32;
            }
            (*sl).EincD = *(*sl).DR.offset((*sl).KSR as isize);
            if (*sl).Ecurp == DECAY {
                (*sl).Einc = (*sl).EincD;
            }
        }
        112 => {
            data &= 0x1f_i32;
            if data != 0 {
                (*sl).SR = (&raw mut (*chip).g.DR_TAB as *mut u32).offset((data << 1_i32) as isize)
                    as *mut u32 as *mut i32;
            } else {
                (*sl).SR =
                    (&raw mut (*chip).g.NULL_RATE as *mut u32).add(0) as *mut u32 as *mut i32;
            }
            (*sl).EincS = *(*sl).SR.offset((*sl).KSR as isize);
            if (*sl).Ecurp == SUBSTAIN && (*sl).Ecnt < ENV_END {
                (*sl).Einc = (*sl).EincS;
            }
        }
        128 => {
            (*sl).SLL = (*chip).g.SL_TAB[(data >> 4_i32) as usize] as i32;
            (*sl).RR = (&raw mut (*chip).g.DR_TAB as *mut u32)
                .offset((((data & 0xf_i32) << 2_i32) + 2_i32) as isize)
                as *mut u32 as *mut i32;
            (*sl).EincR = *(*sl).RR.offset((*sl).KSR as isize);
            if (*sl).Ecurp == RELEASE && (*sl).Ecnt < ENV_END {
                (*sl).Einc = (*sl).EincR;
            }
        }
        144 => {
            set_seg(
                sl,
                if data & 8_i32 != 0 {
                    data & 0xf_i32
                } else {
                    0_i32
                },
            );
        }
        _ => {}
    }
    return 0_i32;
}
pub unsafe fn gens_CHANNEL_SET(mut chip: *mut Chip, mut Adr: i32, mut data: i32) -> i32 {
    let mut num = Adr & 3_i32;
    if num == 3_i32 {
        return 1_i32;
    }
    let mut ch: *mut channel_t = (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t)
        .offset((num + (if Adr & 0x100_i32 != 0 { 3_i32 } else { 0_i32 })) as isize)
        as *mut channel_t;
    match Adr & 0xfc_i32 {
        160 => {
            YM2612_Special_Update();
            (*ch).FNUM[0] = ((*ch).FNUM[0] & 0x700_i32) + data;
            (*ch).KC[0] =
                (*ch).FOCT[0] << 2_i32 | FKEY_TAB[((*ch).FNUM[0] >> 7_i32) as usize] as i32;
            (*ch).SLOT[0].Finc = -(1_i32);
        }
        164 => {
            YM2612_Special_Update();
            (*ch).FNUM[0] = ((*ch).FNUM[0] & 0xff_i32) + ((data & 0x7_i32) << 8_i32);
            (*ch).FOCT[0] = (data & 0x38_i32) >> 3_i32;
            (*ch).KC[0] =
                (*ch).FOCT[0] << 2_i32 | FKEY_TAB[((*ch).FNUM[0] >> 7_i32) as usize] as i32;
            (*ch).SLOT[0].Finc = -(1_i32);
        }
        168 => {
            if Adr < 0x100_i32 {
                num += 1;
                YM2612_Special_Update();
                (*chip).YM2612.CHANNEL[2].FNUM[num as usize] =
                    ((*chip).YM2612.CHANNEL[2].FNUM[num as usize] & 0x700_i32) + data;
                (*chip).YM2612.CHANNEL[2].KC[num as usize] =
                    (*chip).YM2612.CHANNEL[2].FOCT[num as usize] << 2_i32
                        | FKEY_TAB[((*chip).YM2612.CHANNEL[2].FNUM[num as usize] >> 7_i32) as usize]
                            as i32;
                (*chip).YM2612.CHANNEL[2].SLOT[0].Finc = -(1_i32);
            }
        }
        172 => {
            if Adr < 0x100_i32 {
                num += 1;
                YM2612_Special_Update();
                (*chip).YM2612.CHANNEL[2].FNUM[num as usize] =
                    ((*chip).YM2612.CHANNEL[2].FNUM[num as usize] & 0xff_i32)
                        + ((data & 0x7_i32) << 8_i32);
                (*chip).YM2612.CHANNEL[2].FOCT[num as usize] = (data & 0x38_i32) >> 3_i32;
                (*chip).YM2612.CHANNEL[2].KC[num as usize] =
                    (*chip).YM2612.CHANNEL[2].FOCT[num as usize] << 2_i32
                        | FKEY_TAB[((*chip).YM2612.CHANNEL[2].FNUM[num as usize] >> 7_i32) as usize]
                            as i32;
                (*chip).YM2612.CHANNEL[2].SLOT[0].Finc = -(1_i32);
            }
        }
        176 => {
            if (*ch).ALGO != data & 7_i32 {
                YM2612_Special_Update();
                (*ch).ALGO = data & 7_i32;
                (*ch).SLOT[0].ChgEnM = 0_i32;
                (*ch).SLOT[1].ChgEnM = 0_i32;
                (*ch).SLOT[2].ChgEnM = 0_i32;
                (*ch).SLOT[3].ChgEnM = 0_i32;
            }
            (*ch).FB = 9_i32 - (data >> 3_i32 & 7_i32);
        }
        180 => {
            YM2612_Special_Update();
            (*ch).LEFT = 0_i32 - (data >> 7_i32 & 1_i32);
            (*ch).RIGHT = 0_i32 - (data >> 6_i32 & 1_i32);
            (*ch).AMS = LFO_AMS_TAB[(data >> 4_i32 & 3_i32) as usize] as i32;
            (*ch).FMS = LFO_FMS_TAB[(data & 7_i32) as usize] as i32;
            let mut i = 0_i32;
            while i < 4_i32 {
                let mut sl: *mut slot_t =
                    (&raw mut (*ch).SLOT as *mut slot_t).offset(i as isize) as *mut slot_t;
                (*sl).AMS = if (*sl).AMSon != 0 { (*ch).AMS } else { 31_i32 };
                i += 1;
            }
        }
        _ => {}
    }
    return 0_i32;
}
pub unsafe fn gens_YM_SET(mut chip: *mut Chip, mut Adr: i32, mut data: i32) -> i32 {
    match Adr {
        34 => {
            if data & 8_i32 != 0 {
                (*chip).g.LFOinc = (*chip).g.LFO_INC_TAB[(data & 7_i32) as usize];
            } else {
                (*chip).g.LFOcnt = 0_i32;
                (*chip).g.LFOinc = (*chip).g.LFOcnt;
            }
        }
        36 => {
            (*chip).YM2612.TimerA = (*chip).YM2612.TimerA & 0x3_i32 | data << 2_i32;
            if (*chip).YM2612.TimerAL != 1024_i32 - (*chip).YM2612.TimerA << 12_i32 {
                (*chip).YM2612.TimerAL = 1024_i32 - (*chip).YM2612.TimerA << 12_i32;
                (*chip).YM2612.TimerAcnt = (*chip).YM2612.TimerAL;
            }
        }
        37 => {
            (*chip).YM2612.TimerA = (*chip).YM2612.TimerA & 0x3fc_i32 | data & 3_i32;
            if (*chip).YM2612.TimerAL != 1024_i32 - (*chip).YM2612.TimerA << 12_i32 {
                (*chip).YM2612.TimerAL = 1024_i32 - (*chip).YM2612.TimerA << 12_i32;
                (*chip).YM2612.TimerAcnt = (*chip).YM2612.TimerAL;
            }
        }
        38 => {
            (*chip).YM2612.TimerB = data;
            if (*chip).YM2612.TimerBL != 256_i32 - (*chip).YM2612.TimerB << 4_i32 + 12_i32 {
                (*chip).YM2612.TimerBL = 256_i32 - (*chip).YM2612.TimerB << 4_i32 + 12_i32;
                (*chip).YM2612.TimerBcnt = (*chip).YM2612.TimerBL;
            }
        }
        39 => {
            if (data ^ (*chip).YM2612.Mode) & 0x40_i32 != 0 {
                YM2612_Special_Update();
                (*chip).YM2612.CHANNEL[2].SLOT[0].Finc = -(1_i32);
            }
            (*chip).YM2612.Status &= !data >> 4_i32 & data >> 2_i32;
            (*chip).YM2612.Mode = data;
        }
        40 => {
            let mut nch = data & 3_i32;
            if nch == 3_i32 {
                return 1_i32;
            }
            if data & 4_i32 != 0 {
                nch += 3_i32;
            }
            let mut ch: *mut channel_t = (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t)
                .offset(nch as isize) as *mut channel_t;
            YM2612_Special_Update();
            if data & 0x10_i32 != 0 {
                gens_KEY_ON(chip, ch, S0);
            } else {
                gens_KEY_OFF(chip, ch, S0);
            }
            if data & 0x20_i32 != 0 {
                gens_KEY_ON(chip, ch, S1);
            } else {
                gens_KEY_OFF(chip, ch, S1);
            }
            if data & 0x40_i32 != 0 {
                gens_KEY_ON(chip, ch, S2);
            } else {
                gens_KEY_OFF(chip, ch, S2);
            }
            if data & 0x80_i32 != 0 {
                gens_KEY_ON(chip, ch, S3);
            } else {
                gens_KEY_OFF(chip, ch, S3);
            }
        }
        43 => {
            if (*chip).YM2612.DAC ^ data & 0x80_i32 != 0 {
                YM2612_Special_Update();
            }
            (*chip).YM2612.DAC = data & 0x80_i32;
        }
        _ => {}
    }
    return 0_i32;
}
pub unsafe fn gens_set_rate(mut chip: *mut Chip, mut sample_rate: f64, mut clock_rate: f64) {
    let mut i: i32 = 0;
    let mut Frequence = clock_rate / sample_rate / 144.0f64;
    if f64::abs(Frequence - 1.0f64) < 0.0000001f64 {
        Frequence = 1.0f64;
    }
    (*chip).YM2612.TimerBase = (Frequence * 4096.0f64) as i32;
    i = 0_i32;
    while i < TL_LENGHT {
        if i >= PG_CUT_OFF {
            (*chip).g.TL_TAB[i as usize] = 0_i32;
            (*chip).g.TL_TAB[(TL_LENGHT + i) as usize] = (*chip).g.TL_TAB[i as usize];
        } else {
            let mut x = MAX_OUT as f64;
            x /= f64::powf(10.0f64, ENV_STEP * i as f64 / 20.0f64);
            (*chip).g.TL_TAB[i as usize] = x as i32;
            (*chip).g.TL_TAB[(TL_LENGHT + i) as usize] = -(*chip).g.TL_TAB[i as usize];
        }
        i += 1;
    }
    (*chip).g.SIN_TAB[(SIN_LENGHT / 2_i32) as usize] = PG_CUT_OFF as i16;
    (*chip).g.SIN_TAB[0] = (*chip).g.SIN_TAB[(SIN_LENGHT / 2_i32) as usize];
    i = 1_i32;
    while i <= SIN_LENGHT / 4_i32 {
        let mut x_0 = f64::sin(2.0f64 * PI * i as f64 / ((1_i32) << SIN_HBITS) as f64);
        x_0 = 20_i32 as f64 * f64::log10(1_i32 as f64 / x_0);
        let mut j = (x_0 / ENV_STEP) as i32;
        if j > PG_CUT_OFF {
            j = PG_CUT_OFF;
        }
        (*chip).g.SIN_TAB[(SIN_LENGHT / 2_i32 - i) as usize] = j as i16;
        (*chip).g.SIN_TAB[i as usize] = (*chip).g.SIN_TAB[(SIN_LENGHT / 2_i32 - i) as usize];
        (*chip).g.SIN_TAB[(SIN_LENGHT - i) as usize] = (TL_LENGHT + j) as i16;
        (*chip).g.SIN_TAB[(SIN_LENGHT / 2_i32 + i) as usize] =
            (*chip).g.SIN_TAB[(SIN_LENGHT - i) as usize];
        i += 1;
    }
    i = 0_i32;
    while i < LFO_LENGHT {
        let mut x_1 = f64::sin(2.0f64 * PI * i as f64 / ((1_i32) << LFO_HBITS) as f64);
        x_1 += 1.0f64;
        x_1 /= 2.0f64;
        x_1 *= 11.8f64 / ENV_STEP;
        (*chip).g.LFO_ENV_TAB[i as usize] = x_1 as i32 as i16;
        x_1 = f64::sin(2.0f64 * PI * i as f64 / ((1_i32) << LFO_HBITS) as f64);
        x_1 *= (((1_i32) << LFO_HBITS - 1_i32) - 1_i32) as f64;
        (*chip).g.LFO_FREQ_TAB[i as usize] = x_1 as i32 as i16;
        i += 1;
    }
    i = 0_i32;
    while i < ENV_LENGHT {
        let mut x_2 = f64::powf(
            (ENV_LENGHT - 1_i32 - i) as f64 / ((1_i32) << ENV_HBITS) as f64,
            8_i32 as f64,
        );
        x_2 *= ENV_LENGHT as f64;
        (*chip).g.ENV_TAB[i as usize] = x_2 as i32 as i16;
        x_2 = f64::powf(i as f64 / ((1_i32) << ENV_HBITS) as f64, 1_i32 as f64);
        x_2 *= ENV_LENGHT as f64;
        (*chip).g.ENV_TAB[(ENV_LENGHT + i) as usize] = x_2 as i32 as i16;
        i += 1;
    }
    i = 0_i32;
    while i < 8_i32 {
        (*chip).g.ENV_TAB[(i + ENV_LENGHT * 2_i32) as usize] = 0_i16;
        i += 1;
    }
    (*chip).g.ENV_TAB[(ENV_END >> ENV_LBITS) as usize] = (ENV_LENGHT - 1_i32) as i16;
    let mut j_0 = ENV_LENGHT - 1_i32;
    i = 0_i32;
    while i < ENV_LENGHT {
        while j_0 != 0 && ((*chip).g.ENV_TAB[j_0 as usize] as i32) < i {
            j_0 -= 1;
        }
        (*chip).g.DECAY_TO_ATTACK[i as usize] = (j_0 << ENV_LBITS) as u32;
        i += 1;
    }
    i = 0_i32;
    while i < 15_i32 {
        let mut x_3 = (i * 3_i32) as f64;
        x_3 /= ENV_STEP;
        (*chip).g.SL_TAB[i as usize] = (((x_3 as i32) << ENV_LBITS) + ENV_DECAY) as u32;
        i += 1;
    }
    (*chip).g.SL_TAB[15] = (((ENV_LENGHT - 1_i32) << ENV_LBITS) + ENV_DECAY) as u32;
    i = 0_i32;
    while i < 2048_i32 {
        let mut x_4 = i as f64 * Frequence;
        x_4 *= ((1_i32) << SIN_LBITS + SIN_HBITS - (21_i32 - 7_i32)) as f64;
        x_4 /= 2.0f64;
        (*chip).g.FINC_TAB[i as usize] = x_4 as u32;
        i += 1;
    }
    i = 0_i32;
    while i < 4_i32 {
        (*chip).g.AR_TAB[i as usize] = 0_u32;
        (*chip).g.DR_TAB[i as usize] = 0_u32;
        i += 1;
    }
    i = 0_i32;
    while i < 60_i32 {
        let mut x_5 = Frequence;
        x_5 *= 1.0f64 + (i & 3_i32) as f64 * 0.25f64;
        x_5 *= ((1_i32) << (i >> 2_i32)) as f64;
        x_5 *= (ENV_LENGHT << ENV_LBITS) as f64;
        (*chip).g.AR_TAB[(i + 4_i32) as usize] = (x_5 / AR_RATE as f64) as u32;
        (*chip).g.DR_TAB[(i + 4_i32) as usize] = (x_5 / DR_RATE as f64) as u32;
        i += 1;
    }
    i = 64_i32;
    while i < 96_i32 {
        (*chip).g.AR_TAB[i as usize] = (*chip).g.AR_TAB[63];
        (*chip).g.DR_TAB[i as usize] = (*chip).g.DR_TAB[63];
        (*chip).g.NULL_RATE[(i - 64_i32) as usize] = 0_u32;
        i += 1;
    }
    i = 96_i32;
    while i < 128_i32 {
        (*chip).g.AR_TAB[i as usize] = 0_u32;
        i += 1;
    }
    i = 0_i32;
    while i < 4_i32 {
        let mut j_1 = 0_i32;
        while j_1 < 32_i32 {
            let mut y = DT_DEF_TAB[((i << 5_i32) + j_1) as usize] as f64
                * Frequence
                * ((1_i32) << SIN_LBITS + SIN_HBITS - 21_i32) as f64;
            (*chip).g.DT_TAB[(i + 0_i32) as usize][j_1 as usize] = y as i32 as u32;
            (*chip).g.DT_TAB[(i + 4_i32) as usize][j_1 as usize] = -y as i32 as u32;
            j_1 += 1;
        }
        i += 1;
    }
    (*chip).g.LFO_INC_TAB[0] =
        (3.98f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    (*chip).g.LFO_INC_TAB[1] =
        (5.56f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    (*chip).g.LFO_INC_TAB[2] =
        (6.02f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    (*chip).g.LFO_INC_TAB[3] =
        (6.37f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    (*chip).g.LFO_INC_TAB[4] =
        (6.88f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    (*chip).g.LFO_INC_TAB[5] =
        (9.63f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    (*chip).g.LFO_INC_TAB[6] =
        (48.1f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    (*chip).g.LFO_INC_TAB[7] =
        (72.2f64 * ((1_i32) << LFO_HBITS + LFO_LBITS) as f64 / sample_rate) as u32 as i32;
    gens_reset(chip);
}
pub unsafe fn gens_write0(mut chip: *mut Chip, mut opn_addr: i32, mut data: i32) {
    if opn_addr < 0x30_i32 {
        (*chip).YM2612.REG[0][opn_addr as usize] = data;
        gens_YM_SET(chip, opn_addr, data);
    } else if (*chip).YM2612.REG[0][opn_addr as usize] != data {
        (*chip).YM2612.REG[0][opn_addr as usize] = data;
        if opn_addr < 0xa0_i32 {
            gens_SLOT_SET(chip, opn_addr, data);
        } else {
            gens_CHANNEL_SET(chip, opn_addr, data);
        }
    }
}
pub unsafe fn gens_write1(mut chip: *mut Chip, mut opn_addr: i32, mut data: i32) {
    if opn_addr >= 0x30_i32 && (*chip).YM2612.REG[1][opn_addr as usize] != data {
        (*chip).YM2612.REG[1][opn_addr as usize] = data;
        if opn_addr < 0xa0_i32 {
            gens_SLOT_SET(chip, opn_addr + 0x100_i32, data);
        } else {
            gens_CHANNEL_SET(chip, opn_addr + 0x100_i32, data);
        }
    }
}
pub unsafe fn gens_reset(mut chip: *mut Chip) {
    (*chip).g.LFOcnt = 0_i32;
    (*chip).YM2612.TimerA = 0_i32;
    (*chip).YM2612.TimerAL = 0_i32;
    (*chip).YM2612.TimerAcnt = 0_i32;
    (*chip).YM2612.TimerB = 0_i32;
    (*chip).YM2612.TimerBL = 0_i32;
    (*chip).YM2612.TimerBcnt = 0_i32;
    (*chip).YM2612.DAC = 0_i32;
    (*chip).YM2612.Status = 0_i32;
    let mut i: i32 = 0;
    i = 0_i32;
    while i < channel_count {
        let mut ch: *mut channel_t = (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t)
            .offset(i as isize) as *mut channel_t;
        (*ch).LEFT = !(0_i32);
        (*ch).RIGHT = !(0_i32);
        (*ch).ALGO = 0_i32;
        (*ch).FB = 31_i32;
        (*ch).FMS = 0_i32;
        (*ch).AMS = 0_i32;
        let mut j = 0_i32;
        while j < 4_i32 {
            (*ch).S0_OUT[j as usize] = 0_i32;
            (*ch).FNUM[j as usize] = 0_i32;
            (*ch).FOCT[j as usize] = 0_i32;
            (*ch).KC[j as usize] = 0_i32;
            (*ch).SLOT[j as usize].Fcnt = 0_i32;
            (*ch).SLOT[j as usize].Finc = 0_i32;
            (*ch).SLOT[j as usize].Ecnt = ENV_END;
            (*ch).SLOT[j as usize].Einc = 0_i32;
            (*ch).SLOT[j as usize].Ecmp = 0_i32;
            (*ch).SLOT[j as usize].Ecurp = RELEASE;
            (*ch).SLOT[j as usize].ChgEnM = 0_i32;
            j += 1;
        }
        i += 1;
    }
    i = 0_i32;
    while i < 0x100_i32 {
        (*chip).YM2612.REG[0][i as usize] = -(1_i32);
        (*chip).YM2612.REG[1][i as usize] = -(1_i32);
        i += 1;
    }
    i = 0xb6_i32;
    while i >= 0xb4_i32 {
        gens_write0(chip, i, 0xc0_i32);
        gens_write1(chip, i, 0xc0_i32);
        i -= 1;
    }
    i = 0xb2_i32;
    while i >= 0x22_i32 {
        gens_write0(chip, i, 0_i32);
        gens_write1(chip, i, 0_i32);
        i -= 1;
    }
    gens_write0(chip, 0x2a_i32, 0x80_i32);
}
fn update_envelope(slot: &mut slot_t) {
    slot.Ecnt = slot.Ecnt.saturating_add(slot.Einc);
    if slot.Ecnt < slot.Ecmp {
        return;
    }
    match slot.Ecurp {
        ATTACK => {
            slot.Ecnt = ENV_DECAY;
            slot.Einc = slot.EincD;
            slot.Ecmp = slot.SLL;
            slot.Ecurp = DECAY;
        }
        DECAY => {
            slot.Ecnt = slot.SLL;
            slot.Einc = slot.EincS;
            slot.Ecmp = ENV_END;
            slot.Ecurp = SUBSTAIN;
        }
        SUBSTAIN | RELEASE => {
            if slot.Ecurp == SUBSTAIN && slot.SEG & 8 != 0 {
                let repeat = slot.SEG & 1 == 0;
                unsafe { set_seg(slot, (slot.SEG << 1) & 4) };
                if repeat {
                    slot.Ecnt = 0;
                    slot.Einc = slot.EincA;
                    slot.Ecmp = ENV_DECAY;
                    slot.Ecurp = ATTACK;
                    return;
                }
            }
            slot.Ecnt = ENV_END;
            slot.Einc = 0;
            slot.Ecmp = ENV_END + 1;
        }
        _ => {}
    }
}
// Hardware operator order differs from the register slot order.
const OPERATORS: [usize; 4] = [0, 2, 1, 3];

unsafe fn update_chan(
    algorithm: i32,
    tables: *mut tables_t,
    channel: *mut channel_t,
    output: *mut i16,
    frames: i32,
) {
    let g = &*tables;
    let ch = &mut *channel;
    let samples = std::slice::from_raw_parts_mut(output, frames as usize * 2);
    let active = |op: usize| ch.SLOT[OPERATORS[op]].Ecnt != ENV_END;
    if !(active(3)
        || algorithm >= 4 && active(1)
        || algorithm >= 5 && active(2)
        || algorithm == 7 && active(0))
    {
        return;
    }

    let mut lfo = g.LFOcnt.wrapping_add(g.LFOinc);
    for pair in samples.as_chunks_mut::<2>().0 {
        let lfo_index = ((lfo >> LFO_LBITS) & LFO_MASK) as usize;
        let amplitude = i32::from(g.LFO_ENV_TAB[lfo_index]);
        let operator = |number: usize, modulation: i32| {
            let slot = &ch.SLOT[OPERATORS[number]];
            let level = i32::from(g.ENV_TAB[(slot.Ecnt >> ENV_LBITS) as usize]) + slot.TLL;
            let envelope =
                ((level ^ slot.env_xor) + (amplitude >> slot.AMS)) & ((level - slot.env_max) >> 31);
            let phase = (slot.Fcnt.wrapping_add(modulation) >> SIN_LBITS) & (SIN_LENGHT - 1);
            g.TL_TAB[(i32::from(g.SIN_TAB[phase as usize]) + envelope) as usize]
        };

        let previous = ch.S0_OUT[0];
        let feedback = previous.wrapping_add(ch.S0_OUT[1]) >> ch.FB;
        let first = operator(0, feedback);
        let mixed = match algorithm {
            0 => operator(3, operator(2, operator(1, previous))),
            1 => operator(3, operator(2, previous + operator(1, 0))),
            2 => operator(3, previous + operator(2, operator(1, 0))),
            3 => operator(3, operator(1, previous) + operator(2, 0)),
            4 => operator(3, operator(2, 0)) + operator(1, previous),
            5 => operator(3, previous) + operator(1, previous) + operator(2, previous),
            6 => operator(3, 0) + operator(1, previous) + operator(2, 0),
            _ => operator(3, 0) + operator(1, 0) + operator(2, 0) + previous,
        } >> (MAX_OUT_BITS - output_bits + 2);
        ch.S0_OUT[0] = first;
        ch.S0_OUT[1] = previous;
        let frequency = ((i32::from(g.LFO_FREQ_TAB[lfo_index]) * ch.FMS >> LFO_HBITS)
            + (1 << (LFO_FMS_LBITS - 1))) as u32;
        for slot in &mut ch.SLOT {
            let step = (slot.Finc as u32).wrapping_mul(frequency) >> (LFO_FMS_LBITS - 1);
            slot.Fcnt = slot.Fcnt.wrapping_add(step as i32);
            update_envelope(slot);
        }
        pair[0] = pair[0].wrapping_add((mixed & ch.LEFT) as i16);
        pair[1] = pair[1].wrapping_add((mixed & ch.RIGHT) as i16);
        lfo = lfo.wrapping_add(g.LFOinc);
    }
}

pub unsafe fn gens_run_timer(mut chip: *mut Chip, mut length: i32) {
    let step = 6_i32;
    let mut remain = length;
    loop {
        let mut n = step;
        if n > remain {
            n = remain;
        }
        remain -= n;
        let mut i = (n * (*chip).YM2612.TimerBase) as i64;
        if (*chip).YM2612.Mode & 1_i32 != 0 {
            (*chip).YM2612.TimerAcnt = ((*chip).YM2612.TimerAcnt as i64 - i) as i32;
            if (*chip).YM2612.TimerAcnt <= 0_i32 {
                (*chip).YM2612.Status |= ((*chip).YM2612.Mode & 0x4_i32) >> 2_i32;
                (*chip).YM2612.TimerAcnt += (*chip).YM2612.TimerAL;
                if (*chip).YM2612.Mode & 0x80_i32 != 0 {
                    gens_KEY_ON(
                        chip,
                        (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t).add(2)
                            as *mut channel_t,
                        0_i32,
                    );
                    gens_KEY_ON(
                        chip,
                        (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t).add(2)
                            as *mut channel_t,
                        1_i32,
                    );
                    gens_KEY_ON(
                        chip,
                        (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t).add(2)
                            as *mut channel_t,
                        2_i32,
                    );
                    gens_KEY_ON(
                        chip,
                        (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t).add(2)
                            as *mut channel_t,
                        3_i32,
                    );
                }
            }
        }
        if (*chip).YM2612.Mode & 2_i32 != 0 {
            (*chip).YM2612.TimerBcnt = ((*chip).YM2612.TimerBcnt as i64 - i) as i32;
            if (*chip).YM2612.TimerBcnt <= 0_i32 {
                (*chip).YM2612.Status |= ((*chip).YM2612.Mode & 0x8_i32) >> 2_i32;
                (*chip).YM2612.TimerBcnt += (*chip).YM2612.TimerBL;
            }
        }
        if !(remain > 0_i32) {
            break;
        }
    }
}
pub unsafe fn gens_run(mut chip: *mut Chip, mut pair_count: i32, mut out: *mut i16) {
    if pair_count <= 0_i32 {
        return;
    }
    if (*chip).YM2612.Mode & 3_i32 != 0 {
        gens_run_timer(chip, pair_count);
    }
    let mut chi = 0_i32;
    while chi < channel_count {
        let mut ch: *mut channel_t = (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t)
            .offset(chi as isize) as *mut channel_t;
        if (*ch).SLOT[0].Finc == -1 {
            let mut i2 = 0_i32;
            if chi == 2_i32 && (*chip).YM2612.Mode & 0x40_i32 != 0 {
                i2 = 2_i32;
            }
            let mut i = 0_i32;
            while i < 4_i32 {
                let mut sl: *mut slot_t =
                    (&raw mut (*ch).SLOT as *mut slot_t).offset(i as isize) as *mut slot_t;
                let mut finc = ((*chip).g.FINC_TAB[(*ch).FNUM[i2 as usize] as usize]
                    >> 7_i32 - (*ch).FOCT[i2 as usize]) as i32;
                let mut ksr = (*ch).KC[i2 as usize] >> (*sl).KSR_S;
                (*sl).Finc = finc
                    .wrapping_add(*(*sl).DT.offset((*ch).KC[i2 as usize] as isize))
                    .wrapping_mul((*sl).MUL);
                if (*sl).KSR != ksr {
                    (*sl).KSR = ksr;
                    (*sl).EincA = *(*sl).AR.offset(ksr as isize);
                    (*sl).EincD = *(*sl).DR.offset(ksr as isize);
                    (*sl).EincS = *(*sl).SR.offset(ksr as isize);
                    (*sl).EincR = *(*sl).RR.offset(ksr as isize);
                    if (*sl).Ecurp == ATTACK {
                        (*sl).Einc = (*sl).EincA;
                    } else if (*sl).Ecurp == DECAY {
                        (*sl).Einc = (*sl).EincD;
                    } else if (*sl).Ecnt < ENV_END {
                        if (*sl).Ecurp == SUBSTAIN {
                            (*sl).Einc = (*sl).EincS;
                        } else if (*sl).Ecurp == RELEASE {
                            (*sl).Einc = (*sl).EincR;
                        }
                    }
                }
                if i2 != 0 {
                    i2 = i2 ^ 2_i32 ^ i2 >> 1_i32;
                }
                i += 1;
            }
        }
        chi += 1;
    }
    let mut i_0 = 0_i32;
    while i_0 < channel_count {
        if (*chip).mute_mask & (1_i32) << i_0 == 0 && (i_0 != 5_i32 || (*chip).YM2612.DAC == 0) {
            update_chan(
                (*chip).YM2612.CHANNEL[i_0 as usize].ALGO,
                &raw mut (*chip).g,
                (&raw mut (*chip).YM2612.CHANNEL as *mut channel_t).offset(i_0 as isize)
                    as *mut channel_t,
                out,
                pair_count,
            );
        }
        i_0 += 1;
    }
    (*chip).g.LFOcnt = (*chip)
        .g
        .LFOcnt
        .wrapping_add((*chip).g.LFOinc.wrapping_mul(pair_count));
}
