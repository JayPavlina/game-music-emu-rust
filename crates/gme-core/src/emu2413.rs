//! YM2413/VRC7 (OPLL) FM sound chip core.
//!
//! 1:1 transliteration of the vendored `ext/emu2413.c` v1.5.9
//! (<https://github.com/digital-sound-antiques/emu2413>,
//! Copyright (C) 2020 Mitsutaka Okazaki, MIT license).
//!
//! NSF uses the VRC7 presets. VGM also uses the YM2413 instrument ROM and
//! rhythm channels. Both callers do their output-rate conversion externally.

use std::sync::LazyLock;

pub const OPLL_TONE_NUM: usize = 3;

/* phase increment counter */
const DP_BITS: u32 = 19;
const DP_WIDTH: u32 = 1 << DP_BITS;
const DP_BASE_BITS: u32 = DP_BITS - PG_BITS;

/* dynamic range of envelope output */
const EG_STEP: f64 = 0.375;
const EG_BITS: u32 = 7;
const EG_MUTE: i32 = (1 << EG_BITS) - 1;
const EG_MAX: i32 = EG_MUTE - 4;

/* dynamic range of total level */
const TL_BITS: usize = 6;

/* damper speed before key-on. key-scale affects. */
const DAMPER_RATE: i32 = 12;

#[inline]
fn tl2eg(d: u32) -> u32 {
    d << 1
}

/* sine table */
const PG_BITS: u32 = 10; /* 2^10 = 1024 length sine table */
const PG_WIDTH: usize = 1 << PG_BITS;

/// C: `default_inst[OPLL_TONE_NUM][(16 + 3) * 8]` — ROM patch dumps for
/// YM2413 / VRC7 / YMF281B (copied verbatim from the vendored source).
#[rustfmt::skip]
static DEFAULT_INST: [[u8; (16 + 3) * 8]; OPLL_TONE_NUM] = [
    [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 0: User
        0x71, 0x61, 0x1e, 0x17, 0xd0, 0x78, 0x00, 0x17, // 1: Violin
        0x13, 0x41, 0x1a, 0x0d, 0xd8, 0xf7, 0x23, 0x13, // 2: Guitar
        0x13, 0x01, 0x99, 0x00, 0xf2, 0xc4, 0x11, 0x23, // 3: Piano
        0x31, 0x61, 0x0e, 0x07, 0xa8, 0x64, 0x70, 0x27, // 4: Flute
        0x32, 0x21, 0x1e, 0x06, 0xe0, 0x76, 0x00, 0x28, // 5: Clarinet
        0x31, 0x22, 0x16, 0x05, 0xe0, 0x71, 0x00, 0x18, // 6: Oboe
        0x21, 0x61, 0x1d, 0x07, 0x82, 0x81, 0x10, 0x07, // 7: Trumpet
        0x23, 0x21, 0x2d, 0x14, 0xa2, 0x72, 0x00, 0x07, // 8: Organ
        0x61, 0x61, 0x1b, 0x06, 0x64, 0x65, 0x10, 0x17, // 9: Horn
        0x41, 0x61, 0x0b, 0x18, 0x85, 0xf7, 0x71, 0x07, // A: Synthesizer
        0x13, 0x01, 0x83, 0x11, 0xfa, 0xe4, 0x10, 0x04, // B: Harpsichord
        0x17, 0xc1, 0x24, 0x07, 0xf8, 0xf8, 0x22, 0x12, // C: Vibraphone
        0x61, 0x50, 0x0c, 0x05, 0xc2, 0xf5, 0x20, 0x42, // D: Synthesizer Bass
        0x01, 0x01, 0x55, 0x03, 0xc9, 0x95, 0x03, 0x02, // E: Acoustic Bass
        0x61, 0x41, 0x89, 0x03, 0xf1, 0xe4, 0x40, 0x13, // F: Electric Guitar
        0x01, 0x01, 0x18, 0x0f, 0xdf, 0xf8, 0x6a, 0x6d, // R: Bass Drum
        0x01, 0x01, 0x00, 0x00, 0xc8, 0xd8, 0xa7, 0x48, // R: High-Hat(M) / Snare Drum(C)
        0x05, 0x01, 0x00, 0x00, 0xf8, 0xaa, 0x59, 0x55, // R: Tom-tom(M) / Top Cymbal(C)
    ],
    [
        /* VRC7 presets from Nuke.YKT */
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x03, 0x21, 0x05, 0x06, 0xe8, 0x81, 0x42, 0x27,
        0x13, 0x41, 0x14, 0x0d, 0xd8, 0xf6, 0x23, 0x12,
        0x11, 0x11, 0x08, 0x08, 0xfa, 0xb2, 0x20, 0x12,
        0x31, 0x61, 0x0c, 0x07, 0xa8, 0x64, 0x61, 0x27,
        0x32, 0x21, 0x1e, 0x06, 0xe1, 0x76, 0x01, 0x28,
        0x02, 0x01, 0x06, 0x00, 0xa3, 0xe2, 0xf4, 0xf4,
        0x21, 0x61, 0x1d, 0x07, 0x82, 0x81, 0x11, 0x07,
        0x23, 0x21, 0x22, 0x17, 0xa2, 0x72, 0x01, 0x17,
        0x35, 0x11, 0x25, 0x00, 0x40, 0x73, 0x72, 0x01,
        0xb5, 0x01, 0x0f, 0x0F, 0xa8, 0xa5, 0x51, 0x02,
        0x17, 0xc1, 0x24, 0x07, 0xf8, 0xf8, 0x22, 0x12,
        0x71, 0x23, 0x11, 0x06, 0x65, 0x74, 0x18, 0x16,
        0x01, 0x02, 0xd3, 0x05, 0xc9, 0x95, 0x03, 0x02,
        0x61, 0x63, 0x0c, 0x00, 0x94, 0xC0, 0x33, 0xf6,
        0x21, 0x72, 0x0d, 0x00, 0xc1, 0xd5, 0x56, 0x06,
        0x01, 0x01, 0x18, 0x0f, 0xdf, 0xf8, 0x6a, 0x6d,
        0x01, 0x01, 0x00, 0x00, 0xc8, 0xd8, 0xa7, 0x68,
        0x05, 0x01, 0x00, 0x00, 0xf8, 0xaa, 0x59, 0x55,
    ],
    [
        /* YMF281B presets */
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 0: User
        0x62, 0x21, 0x1a, 0x07, 0xf0, 0x6f, 0x00, 0x16, // 1: Electric Strings
        0x40, 0x10, 0x45, 0x00, 0xf6, 0x83, 0x73, 0x63, // 2: Bow Wow
        0x13, 0x01, 0x99, 0x00, 0xf2, 0xc3, 0x21, 0x23, // 3: Electric Guitar
        0x01, 0x61, 0x0b, 0x0f, 0xf9, 0x64, 0x70, 0x17, // 4: Organ
        0x32, 0x21, 0x1e, 0x06, 0xe1, 0x76, 0x01, 0x28, // 5: Clarinet
        0x60, 0x01, 0x82, 0x0e, 0xf9, 0x61, 0x20, 0x27, // 6: Saxophone
        0x21, 0x61, 0x1c, 0x07, 0x84, 0x81, 0x11, 0x07, // 7: Trumpet
        0x37, 0x32, 0xc9, 0x01, 0x66, 0x64, 0x40, 0x28, // 8: Street Organ
        0x01, 0x21, 0x07, 0x03, 0xa5, 0x71, 0x51, 0x07, // 9: Synth Brass
        0x06, 0x01, 0x5e, 0x07, 0xf3, 0xf3, 0xf6, 0x13, // A: Electric Piano
        0x00, 0x00, 0x18, 0x06, 0xf5, 0xf3, 0x20, 0x23, // B: Bass
        0x17, 0xc1, 0x24, 0x07, 0xf8, 0xf8, 0x22, 0x12, // C: Vibraphone
        0x35, 0x64, 0x00, 0x00, 0xff, 0xf3, 0x77, 0xf5, // D: Chimes
        0x11, 0x31, 0x00, 0x07, 0xdd, 0xf3, 0xff, 0xfb, // E: Tom Tom II
        0x3a, 0x21, 0x00, 0x07, 0x80, 0x84, 0x0f, 0xf5, // F: Noise
        0x01, 0x01, 0x18, 0x0f, 0xdf, 0xf8, 0x6a, 0x6d, // R: Bass Drum
        0x01, 0x01, 0x00, 0x00, 0xc8, 0xd8, 0xa7, 0x68, // R: High-Hat(M) / Snare Drum(C)
        0x05, 0x01, 0x00, 0x00, 0xf8, 0xaa, 0x59, 0x55, // R: Tom-tom(M) / Top Cymbal(C)
    ],
];

/* exp_table[x] = round((exp2((double)x / 256.0) - 1) * 1024) */
#[rustfmt::skip]
static EXP_TABLE: [u16; 256] = [
    0,    3,    6,    8,    11,   14,   17,   20,   22,   25,   28,   31,   34,   37,   40,   42,
    45,   48,   51,   54,   57,   60,   63,   66,   69,   72,   75,   78,   81,   84,   87,   90,
    93,   96,   99,   102,  105,  108,  111,  114,  117,  120,  123,  126,  130,  133,  136,  139,
    142,  145,  148,  152,  155,  158,  161,  164,  168,  171,  174,  177,  181,  184,  187,  190,
    194,  197,  200,  204,  207,  210,  214,  217,  220,  224,  227,  231,  234,  237,  241,  244,
    248,  251,  255,  258,  262,  265,  268,  272,  276,  279,  283,  286,  290,  293,  297,  300,
    304,  308,  311,  315,  318,  322,  326,  329,  333,  337,  340,  344,  348,  352,  355,  359,
    363,  367,  370,  374,  378,  382,  385,  389,  393,  397,  401,  405,  409,  412,  416,  420,
    424,  428,  432,  436,  440,  444,  448,  452,  456,  460,  464,  468,  472,  476,  480,  484,
    488,  492,  496,  501,  505,  509,  513,  517,  521,  526,  530,  534,  538,  542,  547,  551,
    555,  560,  564,  568,  572,  577,  581,  585,  590,  594,  599,  603,  607,  612,  616,  621,
    625,  630,  634,  639,  643,  648,  652,  657,  661,  666,  670,  675,  680,  684,  689,  693,
    698,  703,  708,  712,  717,  722,  726,  731,  736,  741,  745,  750,  755,  760,  765,  770,
    774,  779,  784,  789,  794,  799,  804,  809,  814,  819,  824,  829,  834,  839,  844,  849,
    854,  859,  864,  869,  874,  880,  885,  890,  895,  900,  906,  911,  916,  921,  927,  932,
    937,  942,  948,  953,  959,  964,  969,  975,  980,  986,  991,  996,  1002, 1007, 1013, 1018,
];

/* fullsin_table[x] = round(-log2(sin((x + 0.5) * PI / (PG_WIDTH / 4) / 2)) * 256)
(first quarter; the rest is generated by make_sin_table, like the C code) */
#[rustfmt::skip]
static FULLSIN_BASE: [u16; PG_WIDTH / 4] = [
    2137, 1731, 1543, 1419, 1326, 1252, 1190, 1137, 1091, 1050, 1013, 979,  949,  920,  894,  869,
    846,  825,  804,  785,  767,  749,  732,  717,  701,  687,  672,  659,  646,  633,  621,  609,
    598,  587,  576,  566,  556,  546,  536,  527,  518,  509,  501,  492,  484,  476,  468,  461,
    453,  446,  439,  432,  425,  418,  411,  405,  399,  392,  386,  380,  375,  369,  363,  358,
    352,  347,  341,  336,  331,  326,  321,  316,  311,  307,  302,  297,  293,  289,  284,  280,
    276,  271,  267,  263,  259,  255,  251,  248,  244,  240,  236,  233,  229,  226,  222,  219,
    215,  212,  209,  205,  202,  199,  196,  193,  190,  187,  184,  181,  178,  175,  172,  169,
    167,  164,  161,  159,  156,  153,  151,  148,  146,  143,  141,  138,  136,  134,  131,  129,
    127,  125,  122,  120,  118,  116,  114,  112,  110,  108,  106,  104,  102,  100,  98,   96,
    94,   92,   91,   89,   87,   85,   83,   82,   80,   78,   77,   75,   74,   72,   70,   69,
    67,   66,   64,   63,   62,   60,   59,   57,   56,   55,   53,   52,   51,   49,   48,   47,
    46,   45,   43,   42,   41,   40,   39,   38,   37,   36,   35,   34,   33,   32,   31,   30,
    29,   28,   27,   26,   25,   24,   23,   23,   22,   21,   20,   20,   19,   18,   17,   17,
    16,   15,   15,   14,   13,   13,   12,   12,   11,   10,   10,   9,    9,    8,    8,    7,
    7,    7,    6,    6,    5,    5,    5,    4,    4,    4,    3,    3,    3,    2,    2,    2,
    2,    1,    1,    1,    1,    1,    1,    1,    0,    0,    0,    0,    0,    0,    0,    0,
];

/* pitch modulator: offset to fnum, rough approximation of 14 cents depth. */
#[rustfmt::skip]
static PM_TABLE: [[i8; 8]; 8] = [
    [0, 0, 0, 0, 0, 0, 0, 0],    // fnum = 000xxxxxx
    [0, 0, 1, 0, 0, 0, -1, 0],   // fnum = 001xxxxxx
    [0, 1, 2, 1, 0, -1, -2, -1], // fnum = 010xxxxxx
    [0, 1, 3, 1, 0, -1, -3, -1], // fnum = 011xxxxxx
    [0, 2, 4, 2, 0, -2, -4, -2], // fnum = 100xxxxxx
    [0, 2, 5, 2, 0, -2, -5, -2], // fnum = 101xxxxxx
    [0, 3, 6, 3, 0, -3, -6, -3], // fnum = 110xxxxxx
    [0, 3, 7, 3, 0, -3, -7, -3], // fnum = 111xxxxxx
];

/* amplitude lfo table */
#[rustfmt::skip]
static AM_TABLE: [u8; 210] = [
    0,  0,  0,  0,  0,  0,  0,  0,  1,  1,  1,  1,  1,  1,  1,  1,
    2,  2,  2,  2,  2,  2,  2,  2,  3,  3,  3,  3,  3,  3,  3,  3,
    4,  4,  4,  4,  4,  4,  4,  4,  5,  5,  5,  5,  5,  5,  5,  5,
    6,  6,  6,  6,  6,  6,  6,  6,  7,  7,  7,  7,  7,  7,  7,  7,
    8,  8,  8,  8,  8,  8,  8,  8,  9,  9,  9,  9,  9,  9,  9,  9,
    10, 10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 11, 11, 11,
    12, 12, 12, 12, 12, 12, 12, 12,
    13, 13, 13,
    12, 12, 12, 12, 12, 12, 12, 12,
    11, 11, 11, 11, 11, 11, 11, 11, 10, 10, 10, 10, 10, 10, 10, 10,
    9,  9,  9,  9,  9,  9,  9,  9,  8,  8,  8,  8,  8,  8,  8,  8,
    7,  7,  7,  7,  7,  7,  7,  7,  6,  6,  6,  6,  6,  6,  6,  6,
    5,  5,  5,  5,  5,  5,  5,  5,  4,  4,  4,  4,  4,  4,  4,  4,
    3,  3,  3,  3,  3,  3,  3,  3,  2,  2,  2,  2,  2,  2,  2,  2,
    1,  1,  1,  1,  1,  1,  1,  1,  0,  0,  0,  0,  0,  0,  0,
];

/* envelope decay increment step table (based on andete's research) */
static EG_STEP_TABLES: [[u8; 8]; 4] = [
    [0, 1, 0, 1, 0, 1, 0, 1],
    [0, 1, 0, 1, 1, 1, 0, 1],
    [0, 1, 1, 1, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 1],
];

/* __OPLL_EG_STATE */
const ATTACK: u8 = 0;
const DECAY: u8 = 1;
const SUSTAIN: u8 = 2;
const RELEASE: u8 = 3;
const DAMP: u8 = 4;

static ML_TABLE: [u32; 16] = [
    1,
    2,
    2 * 2,
    3 * 2,
    4 * 2,
    5 * 2,
    6 * 2,
    7 * 2,
    8 * 2,
    9 * 2,
    10 * 2,
    10 * 2,
    12 * 2,
    12 * 2,
    15 * 2,
    15 * 2,
];

#[inline]
fn db2(x: f64) -> f64 {
    x * 2.0
}

static KL_TABLE: LazyLock<[f64; 16]> = LazyLock::new(|| {
    [
        db2(0.000),
        db2(9.000),
        db2(12.000),
        db2(13.875),
        db2(15.000),
        db2(16.125),
        db2(16.875),
        db2(17.625),
        db2(18.000),
        db2(18.750),
        db2(19.125),
        db2(19.500),
        db2(19.875),
        db2(20.250),
        db2(20.625),
        db2(21.000),
    ]
});

/// Voice data (`OPLL_PATCH`).
#[derive(Clone, Copy, Default, Debug)]
pub struct OpllPatch {
    pub tl: u32,
    pub fb: u32,
    pub eg: u32,
    pub ml: u32,
    pub ar: u32,
    pub dr: u32,
    pub sl: u32,
    pub rr: u32,
    pub kr: u32,
    pub kl: u32,
    pub am: u32,
    pub pm: u32,
    pub ws: u32,
}

/// Index of the all-zero "null" patch (C: `null_patch`); patch slot 38 in the
/// instance patch array is never written, so it stays zeroed.
const NULL_PATCH_IDX: usize = 38;

/// Slot (`OPLL_SLOT`). The C struct holds `OPLL_PATCH *patch` and
/// `uint16_t *wave_table` pointers; here they are indices (`patch` into
/// `Opll::patch`, `wave_table` into the wave table map) so the struct stays
/// movable.
#[derive(Clone, Copy)]
struct OpllSlot {
    number: u8,
    /// type flags: bit 0 = carrier, bit 1 = single slot mode
    type_: u8,
    patch: usize,
    /// slot output, latest and previous
    output: [i32; 2],
    /// 0 = fullsin, 1 = halfsin
    wave_table: u8,
    pg_phase: u32,
    pg_out: u32,
    pg_keep: u8,
    blk_fnum: u16,
    fnum: u16,
    blk: u8,
    eg_state: u8,
    volume: i32,
    key_flag: u8,
    sus_flag: u8,
    tll: u16,
    rks: u8,
    eg_rate_h: u8,
    eg_rate_l: u8,
    eg_shift: u32,
    eg_out: u32,
    update_requests: u32,
}

impl Default for OpllSlot {
    fn default() -> Self {
        Self {
            number: 0,
            type_: 0,
            patch: NULL_PATCH_IDX,
            output: [0; 2],
            wave_table: 0,
            pg_phase: 0,
            pg_out: 0,
            pg_keep: 0,
            blk_fnum: 0,
            fnum: 0,
            blk: 0,
            eg_state: 0,
            volume: 0,
            key_flag: 0,
            sus_flag: 0,
            tll: 0,
            rks: 0,
            eg_rate_h: 0,
            eg_rate_l: 0,
            eg_shift: 0,
            eg_out: 0,
            update_requests: 0,
        }
    }
}

const SLOT_BD1: usize = 12;
#[allow(dead_code)] // kept for parity with the C source (covered by `3 << SLOT_BD1`)
const SLOT_BD2: usize = 13;
const SLOT_HH: usize = 14;
const SLOT_SD: usize = 15;
const SLOT_TOM: usize = 16;
const SLOT_CYM: usize = 17;

#[inline]
fn bit(s: u32, b: u32) -> u8 {
    ((s >> b) & 1) as u8
}

/* SLOT_UPDATE_FLAG */
const UPDATE_WS: u32 = 1;
const UPDATE_TLL: u32 = 2;
const UPDATE_RKS: u32 = 4;
const UPDATE_EG: u32 = 8;
const UPDATE_ALL: u32 = 255;

/// Tables generated by the C `initializeTables()` (same double-precision
/// generation code, computed once lazily).
struct Tables {
    fullsin: [u16; PG_WIDTH],
    halfsin: [u16; PG_WIDTH],
    /// `tll_table[8 * 16][1 << TL_BITS][4]`
    tll: Box<[[[u32; 4]; 1 << TL_BITS]; 8 * 16]>,
    /// `rks_table[8 * 2][2]`
    rks: [[i32; 2]; 16],
    /// `default_patch[OPLL_TONE_NUM][(16 + 3) * 2]`
    default_patch: [[OpllPatch; 19 * 2]; OPLL_TONE_NUM],
}

static TABLES: LazyLock<Tables> = LazyLock::new(Tables::new);

impl Tables {
    fn new() -> Self {
        /* makeSinTable */
        let mut fullsin = [0u16; PG_WIDTH];
        fullsin[..PG_WIDTH / 4].copy_from_slice(&FULLSIN_BASE);
        for x in 0..PG_WIDTH / 4 {
            fullsin[PG_WIDTH / 4 + x] = fullsin[PG_WIDTH / 4 - x - 1];
        }
        for x in 0..PG_WIDTH / 2 {
            fullsin[PG_WIDTH / 2 + x] = 0x8000 | fullsin[x];
        }
        let mut halfsin = [0u16; PG_WIDTH];
        halfsin[..PG_WIDTH / 2].copy_from_slice(&fullsin[..PG_WIDTH / 2]);
        for h in halfsin.iter_mut().take(PG_WIDTH).skip(PG_WIDTH / 2) {
            *h = 0xfff;
        }

        /* makeTllTable */
        let mut tll = Box::new([[[0u32; 4]; 1 << TL_BITS]; 8 * 16]);
        for fnum in 0..16usize {
            for block in 0..8usize {
                for tl in 0..64u32 {
                    for (kl, level) in tll[(block << 4) | fnum][tl as usize].iter_mut().enumerate()
                    {
                        if kl == 0 {
                            *level = tl2eg(tl);
                        } else {
                            let tmp = (KL_TABLE[fnum] - db2(3.000) * (7 - block) as f64) as i32;
                            if tmp <= 0 {
                                *level = tl2eg(tl);
                            } else {
                                *level = ((tmp >> (3 - kl)) as f64 / EG_STEP) as u32 + tl2eg(tl);
                            }
                        }
                    }
                }
            }
        }

        /* makeRksTable */
        let mut rks = [[0i32; 2]; 16];
        for fnum8 in 0..2usize {
            for block in 0..8usize {
                rks[(block << 1) | fnum8][1] = ((block << 1) + fnum8) as i32;
                rks[(block << 1) | fnum8][0] = (block >> 1) as i32;
            }
        }

        /* makeDefaultPatch */
        let mut default_patch = [[OpllPatch::default(); 19 * 2]; OPLL_TONE_NUM];
        for (i, patches) in default_patch.iter_mut().enumerate() {
            for j in 0..19usize {
                let (p0, p1) = dump_to_patch(&DEFAULT_INST[i][j * 8..j * 8 + 8]);
                patches[j * 2] = p0;
                patches[j * 2 + 1] = p1;
            }
        }

        Self {
            fullsin,
            halfsin,
            tll,
            rks,
            default_patch,
        }
    }

    #[inline]
    fn wave(&self, table: u8, idx: usize) -> u16 {
        if table == 0 {
            self.fullsin[idx]
        } else {
            self.halfsin[idx]
        }
    }
}

/// C: `OPLL_dumpToPatch` — convert an 8-byte register dump to a
/// modulator/carrier patch pair.
fn dump_to_patch(dump: &[u8]) -> (OpllPatch, OpllPatch) {
    let mut p0 = OpllPatch::default();
    let mut p1 = OpllPatch::default();
    p0.am = ((dump[0] >> 7) & 1) as u32;
    p1.am = ((dump[1] >> 7) & 1) as u32;
    p0.pm = ((dump[0] >> 6) & 1) as u32;
    p1.pm = ((dump[1] >> 6) & 1) as u32;
    p0.eg = ((dump[0] >> 5) & 1) as u32;
    p1.eg = ((dump[1] >> 5) & 1) as u32;
    p0.kr = ((dump[0] >> 4) & 1) as u32;
    p1.kr = ((dump[1] >> 4) & 1) as u32;
    p0.ml = (dump[0] & 15) as u32;
    p1.ml = (dump[1] & 15) as u32;
    p0.kl = ((dump[2] >> 6) & 3) as u32;
    p1.kl = ((dump[3] >> 6) & 3) as u32;
    p0.tl = (dump[2] & 63) as u32;
    p1.tl = 0;
    p0.fb = (dump[3] & 7) as u32;
    p1.fb = 0;
    p0.ws = ((dump[3] >> 3) & 1) as u32;
    p1.ws = ((dump[3] >> 4) & 1) as u32;
    p0.ar = ((dump[4] >> 4) & 15) as u32;
    p1.ar = ((dump[5] >> 4) & 15) as u32;
    p0.dr = (dump[4] & 15) as u32;
    p1.dr = (dump[5] & 15) as u32;
    p0.sl = ((dump[6] >> 4) & 15) as u32;
    p1.sl = ((dump[7] >> 4) & 15) as u32;
    p0.rr = (dump[6] & 15) as u32;
    p1.rr = (dump[7] & 15) as u32;
    (p0, p1)
}

/// C: `lookup_attack_step`.
fn lookup_attack_step(slot: &OpllSlot, counter: u32) -> u8 {
    match slot.eg_rate_h {
        12 => {
            let index = ((counter & 0xc) >> 1) as usize;
            4 - EG_STEP_TABLES[slot.eg_rate_l as usize][index]
        }
        13 => {
            let index = ((counter & 0xc) >> 1) as usize;
            3 - EG_STEP_TABLES[slot.eg_rate_l as usize][index]
        }
        14 => {
            let index = ((counter & 0xc) >> 1) as usize;
            2 - EG_STEP_TABLES[slot.eg_rate_l as usize][index]
        }
        0 | 15 => 0,
        _ => {
            let index = counter >> slot.eg_shift;
            if EG_STEP_TABLES[slot.eg_rate_l as usize][(index & 7) as usize] != 0 {
                4
            } else {
                0
            }
        }
    }
}

/// C: `lookup_decay_step`.
fn lookup_decay_step(slot: &OpllSlot, counter: u32) -> u8 {
    match slot.eg_rate_h {
        0 => 0,
        13 => {
            let index = (((counter & 0xc) >> 1) | (counter & 1)) as usize;
            EG_STEP_TABLES[slot.eg_rate_l as usize][index]
        }
        14 => {
            let index = ((counter & 0xc) >> 1) as usize;
            EG_STEP_TABLES[slot.eg_rate_l as usize][index] + 1
        }
        15 => 2,
        _ => {
            let index = counter >> slot.eg_shift;
            EG_STEP_TABLES[slot.eg_rate_l as usize][(index & 7) as usize]
        }
    }
}

/// C: `lookup_exp_table` — output: -4095...4095.
#[inline]
fn lookup_exp_table(i: u16) -> i16 {
    /* from andete's expression */
    let t: i16 = (EXP_TABLE[((i & 0xff) ^ 0xff) as usize] + 1024) as i16;
    let res: i16 = ((t as i32) >> ((i as i32 & 0x7f00) >> 8)) as i16;
    (if i & 0x8000 != 0 { !res } else { res }) << 1
}

/// C: `to_linear`.
#[inline]
fn to_linear(h: u16, slot: &OpllSlot, am: i16) -> i16 {
    if slot.eg_out as i32 > EG_MAX {
        return 0;
    }
    let att = ((EG_MUTE.min(slot.eg_out as i32 + slot.tll as i32 + am as i32)) << 4) as u16;
    lookup_exp_table(h.wrapping_add(att))
}

#[inline]
fn mo(x: i16) -> i16 {
    ((-(x as i32)) >> 1) as i16
}

#[inline]
fn ro(x: i16) -> i16 {
    x
}

/// The OPLL chip (`struct __OPLL`). The internal rate converter (`conv`) is
/// omitted: the VRC7 wrapper always sets rate = clk / 72, which disables it
/// (asserted in `reset_rate_conversion_params`).
pub struct Opll {
    clk: u32,
    rate: u32,
    chip_type: u8,
    adr: u32,
    inp_step: f64,
    out_step: f64,
    out_time: f64,
    reg: [u8; 0x40],
    test_flag: u8,
    slot_key_status: u32,
    rhythm_mode: u8,
    eg_counter: u32,
    pm_phase: u32,
    am_phase: i32,
    lfo_am: u8,
    noise: u32,
    short_noise: u8,
    patch_number: [i32; 9],
    slot: [OpllSlot; 18],
    /// 19*2 patches plus the never-written null patch at [38].
    patch: [OpllPatch; 19 * 2 + 1],
    pan: [u8; 16],
    pan_fine: [[f32; 2]; 16],
    mask: u32,
    /// channel output: 0..8 tone, 9 bd, 10 hh, 11 sd, 12 tom, 13 cym
    ch_out: [i16; 14],
    mix_out: [i16; 2],
}

#[inline]
fn mod_idx(ch: usize) -> usize {
    ch << 1
}

#[inline]
fn car_idx(ch: usize) -> usize {
    (ch << 1) | 1
}

impl Opll {
    /// C: `OPLL_new` (calloc'd zero state, then reset / chip type 0 / patch 0).
    pub fn new(clk: u32, rate: u32) -> Self {
        let mut opll = Self {
            clk,
            rate,
            chip_type: 0,
            adr: 0,
            inp_step: 0.0,
            out_step: 0.0,
            out_time: 0.0,
            reg: [0; 0x40],
            test_flag: 0,
            slot_key_status: 0,
            rhythm_mode: 0,
            eg_counter: 0,
            pm_phase: 0,
            am_phase: 0,
            lfo_am: 0,
            noise: 0,
            short_noise: 0,
            patch_number: [0; 9],
            slot: [OpllSlot::default(); 18],
            patch: [OpllPatch::default(); 19 * 2 + 1],
            pan: [0; 16],
            pan_fine: [[0.0; 2]; 16],
            mask: 0,
            ch_out: [0; 14],
            mix_out: [0; 2],
        };
        opll.reset();
        opll.set_chip_type(0);
        opll.reset_patch(0);
        opll
    }

    /// C: `OPLL_setChipType`.
    pub fn set_chip_type(&mut self, chip_type: u8) {
        self.chip_type = chip_type;
    }

    /// C: `OPLL_resetPatch`.
    pub fn reset_patch(&mut self, patch_type: u8) {
        let t = (patch_type as usize) % OPLL_TONE_NUM;
        for i in 0..19 * 2 {
            self.patch[i] = TABLES.default_patch[t][i];
        }
    }

    /// C: `reset_rate_conversion_params`.
    fn reset_rate_conversion_params(&mut self) {
        let f_out = self.rate as f64;
        let f_inp = self.clk as f64 / 72.0;

        self.out_time = 0.0;
        self.out_step = f_inp;
        self.inp_step = f_out;

        // C creates an OPLL_RateConv when floor(f_inp) != f_out and
        // floor(f_inp + 0.5) != f_out. The NSF VRC7 wrapper always passes
        // rate = clk / 72, so the converter is never needed.
        debug_assert!(
            f_inp.floor() == f_out || (f_inp + 0.5).floor() == f_out,
            "OPLL rate converter not supported (rate must be clk / 72)"
        );
    }

    /// C: `OPLL_reset`.
    pub fn reset(&mut self) {
        self.adr = 0;
        self.pm_phase = 0;
        self.am_phase = 0;
        self.noise = 0x1;
        self.mask = 0;
        self.rhythm_mode = 0;
        self.slot_key_status = 0;
        self.eg_counter = 0;

        self.reset_rate_conversion_params();

        for i in 0..18 {
            self.reset_slot(i);
        }
        for i in 0..9 {
            self.set_patch(i, 0);
        }
        for i in 0..0x40u32 {
            self.write_reg(i, 0);
        }
        for i in 0..15 {
            self.pan[i] = 3;
            self.pan_fine[i] = [1.0, 1.0];
        }
        self.ch_out = [0; 14];
    }

    /// C: `reset_slot`. Note: `update_requests` is deliberately not cleared,
    /// matching the C code.
    fn reset_slot(&mut self, number: usize) {
        let slot = &mut self.slot[number];
        slot.number = number as u8;
        slot.type_ = (number % 2) as u8;
        slot.pg_keep = 0;
        slot.wave_table = 0;
        slot.pg_phase = 0;
        slot.output[0] = 0;
        slot.output[1] = 0;
        slot.eg_state = RELEASE;
        slot.eg_shift = 0;
        slot.rks = 0;
        slot.tll = 0;
        slot.key_flag = 0;
        slot.sus_flag = 0;
        slot.blk_fnum = 0;
        slot.blk = 0;
        slot.fnum = 0;
        slot.volume = 0;
        slot.pg_out = 0;
        slot.eg_out = EG_MUTE as u32;
        slot.patch = NULL_PATCH_IDX;
    }

    #[inline]
    fn patch_of(&self, slot_idx: usize) -> &OpllPatch {
        &self.patch[self.slot[slot_idx].patch]
    }

    #[inline]
    fn request_update(&mut self, slot_idx: usize, flag: u32) {
        self.slot[slot_idx].update_requests |= flag;
    }

    /// C: `get_parameter_rate`.
    fn get_parameter_rate(&self, i: usize) -> i32 {
        let slot = &self.slot[i];
        let patch = self.patch_of(i);

        if (slot.type_ & 1) == 0 && slot.key_flag == 0 {
            return 0;
        }

        match slot.eg_state {
            ATTACK => patch.ar as i32,
            DECAY => patch.dr as i32,
            SUSTAIN => {
                if patch.eg != 0 {
                    0
                } else {
                    patch.rr as i32
                }
            }
            RELEASE => {
                if slot.sus_flag != 0 {
                    5
                } else if patch.eg != 0 {
                    patch.rr as i32
                } else {
                    7
                }
            }
            DAMP => DAMPER_RATE,
            _ => 0,
        }
    }

    /// C: `commit_slot_update`.
    fn commit_slot_update(&mut self, i: usize) {
        let patch = *self.patch_of(i);

        if self.slot[i].update_requests & UPDATE_WS != 0 {
            self.slot[i].wave_table = patch.ws as u8;
        }

        if self.slot[i].update_requests & UPDATE_TLL != 0 {
            let slot = &self.slot[i];
            let tll = if (slot.type_ & 1) == 0 {
                TABLES.tll[(slot.blk_fnum >> 5) as usize][patch.tl as usize][patch.kl as usize]
            } else {
                TABLES.tll[(slot.blk_fnum >> 5) as usize][slot.volume as usize][patch.kl as usize]
            };
            self.slot[i].tll = tll as u16;
        }

        if self.slot[i].update_requests & UPDATE_RKS != 0 {
            let blk_fnum = self.slot[i].blk_fnum;
            self.slot[i].rks = TABLES.rks[(blk_fnum >> 8) as usize][patch.kr as usize] as u8;
        }

        if self.slot[i].update_requests & (UPDATE_RKS | UPDATE_EG) != 0 {
            let p_rate = self.get_parameter_rate(i);
            let slot = &mut self.slot[i];

            if p_rate == 0 {
                slot.eg_shift = 0;
                slot.eg_rate_h = 0;
                slot.eg_rate_l = 0;
                // C returns here without clearing update_requests.
                return;
            }

            slot.eg_rate_h = 15.min(p_rate + (slot.rks as i32 >> 2)) as u8;
            slot.eg_rate_l = slot.rks & 3;
            if slot.eg_state == ATTACK {
                slot.eg_shift = if 0 < slot.eg_rate_h && slot.eg_rate_h < 12 {
                    (13 - slot.eg_rate_h) as u32
                } else {
                    0
                };
            } else {
                slot.eg_shift = if slot.eg_rate_h < 13 {
                    (13 - slot.eg_rate_h) as u32
                } else {
                    0
                };
            }
        }

        self.slot[i].update_requests = 0;
    }

    /// C: `slotOn`.
    fn slot_on(&mut self, i: usize) {
        self.slot[i].key_flag = 1;
        self.slot[i].eg_state = DAMP;
        self.request_update(i, UPDATE_EG);
    }

    /// C: `slotOff`.
    fn slot_off(&mut self, i: usize) {
        self.slot[i].key_flag = 0;
        if self.slot[i].type_ & 1 != 0 {
            self.slot[i].eg_state = RELEASE;
            self.request_update(i, UPDATE_EG);
        }
    }

    /// C: `update_key_status`.
    fn update_key_status(&mut self) {
        let r14 = self.reg[0x0e];
        let rhythm_mode = bit(r14 as u32, 5);
        let mut new_slot_key_status: u32 = 0;

        for ch in 0..9 {
            if self.reg[0x20 + ch] & 0x10 != 0 {
                new_slot_key_status |= 3 << (ch * 2);
            }
        }

        if rhythm_mode != 0 {
            if r14 & 0x10 != 0 {
                new_slot_key_status |= 3 << SLOT_BD1;
            }
            if r14 & 0x01 != 0 {
                new_slot_key_status |= 1 << SLOT_HH;
            }
            if r14 & 0x08 != 0 {
                new_slot_key_status |= 1 << SLOT_SD;
            }
            if r14 & 0x04 != 0 {
                new_slot_key_status |= 1 << SLOT_TOM;
            }
            if r14 & 0x02 != 0 {
                new_slot_key_status |= 1 << SLOT_CYM;
            }
        }

        let updated_status = self.slot_key_status ^ new_slot_key_status;

        if updated_status != 0 {
            for i in 0..18 {
                if bit(updated_status, i as u32) != 0 {
                    if bit(new_slot_key_status, i as u32) != 0 {
                        self.slot_on(i);
                    } else {
                        self.slot_off(i);
                    }
                }
            }
        }

        self.slot_key_status = new_slot_key_status;
    }

    /// C: `set_patch`.
    fn set_patch(&mut self, ch: usize, num: i32) {
        self.patch_number[ch] = num;
        self.slot[mod_idx(ch)].patch = (num * 2) as usize;
        self.slot[car_idx(ch)].patch = (num * 2 + 1) as usize;
        self.request_update(mod_idx(ch), UPDATE_ALL);
        self.request_update(car_idx(ch), UPDATE_ALL);
    }

    /// C: `set_sus_flag`.
    fn set_sus_flag(&mut self, ch: usize, flag: u8) {
        self.slot[car_idx(ch)].sus_flag = flag;
        self.request_update(car_idx(ch), UPDATE_EG);
        if self.slot[mod_idx(ch)].type_ & 1 != 0 {
            self.slot[mod_idx(ch)].sus_flag = flag;
            self.request_update(mod_idx(ch), UPDATE_EG);
        }
    }

    /// C: `set_volume` (volume: 6 bits, register value << 2).
    fn set_volume(&mut self, ch: usize, volume: i32) {
        self.slot[car_idx(ch)].volume = volume;
        self.request_update(car_idx(ch), UPDATE_TLL);
    }

    /// C: `set_slot_volume`.
    fn set_slot_volume(&mut self, slot_idx: usize, volume: i32) {
        self.slot[slot_idx].volume = volume;
        self.request_update(slot_idx, UPDATE_TLL);
    }

    /// C: `set_fnumber` (fnum: 9 bits).
    fn set_fnumber(&mut self, ch: usize, fnum: i32) {
        for idx in [car_idx(ch), mod_idx(ch)] {
            let slot = &mut self.slot[idx];
            slot.fnum = fnum as u16;
            slot.blk_fnum = (slot.blk_fnum & 0xe00) | (fnum as u16 & 0x1ff);
        }
        self.request_update(car_idx(ch), UPDATE_EG | UPDATE_RKS | UPDATE_TLL);
        self.request_update(mod_idx(ch), UPDATE_EG | UPDATE_RKS | UPDATE_TLL);
    }

    /// C: `set_block` (blk: 3 bits).
    fn set_block(&mut self, ch: usize, blk: i32) {
        for idx in [car_idx(ch), mod_idx(ch)] {
            let slot = &mut self.slot[idx];
            slot.blk = blk as u8;
            slot.blk_fnum = (((blk as u16) & 7) << 9) | (slot.blk_fnum & 0x1ff);
        }
        self.request_update(car_idx(ch), UPDATE_EG | UPDATE_RKS | UPDATE_TLL);
        self.request_update(mod_idx(ch), UPDATE_EG | UPDATE_RKS | UPDATE_TLL);
    }

    /// C: `update_rhythm_mode`.
    fn update_rhythm_mode(&mut self) {
        let new_rhythm_mode = (self.reg[0x0e] >> 5) & 1;

        if self.rhythm_mode != new_rhythm_mode {
            if new_rhythm_mode != 0 {
                self.slot[SLOT_HH].type_ = 3;
                self.slot[SLOT_HH].pg_keep = 1;
                self.slot[SLOT_SD].type_ = 3;
                self.slot[SLOT_TOM].type_ = 3;
                self.slot[SLOT_CYM].type_ = 3;
                self.slot[SLOT_CYM].pg_keep = 1;
                self.set_patch(6, 16);
                self.set_patch(7, 17);
                self.set_patch(8, 18);
                let v_hh = (((self.reg[0x37] >> 4) & 15) as i32) << 2;
                self.set_slot_volume(SLOT_HH, v_hh);
                let v_tom = (((self.reg[0x38] >> 4) & 15) as i32) << 2;
                self.set_slot_volume(SLOT_TOM, v_tom);
            } else {
                self.slot[SLOT_HH].type_ = 0;
                self.slot[SLOT_HH].pg_keep = 0;
                self.slot[SLOT_SD].type_ = 1;
                self.slot[SLOT_TOM].type_ = 0;
                self.slot[SLOT_CYM].type_ = 1;
                self.slot[SLOT_CYM].pg_keep = 0;
                self.set_patch(6, (self.reg[0x36] >> 4) as i32);
                self.set_patch(7, (self.reg[0x37] >> 4) as i32);
                self.set_patch(8, (self.reg[0x38] >> 4) as i32);
            }
        }

        self.rhythm_mode = new_rhythm_mode;
    }

    /// C: `update_ampm`.
    fn update_ampm(&mut self) {
        if self.test_flag & 2 != 0 {
            self.pm_phase = 0;
            self.am_phase = 0;
        } else {
            self.pm_phase =
                self.pm_phase
                    .wrapping_add(if self.test_flag & 8 != 0 { 1024 } else { 1 });
            self.am_phase =
                self.am_phase
                    .wrapping_add(if self.test_flag & 8 != 0 { 64 } else { 1 });
        }
        self.lfo_am = AM_TABLE[((self.am_phase >> 6) % AM_TABLE.len() as i32) as usize];
    }

    /// C: `update_noise`.
    fn update_noise(&mut self, cycle: i32) {
        for _ in 0..cycle {
            if self.noise & 1 != 0 {
                self.noise ^= 0x800200;
            }
            self.noise >>= 1;
        }
    }

    /// C: `update_short_noise`.
    fn update_short_noise(&mut self) {
        let pg_hh = self.slot[SLOT_HH].pg_out;
        let pg_cym = self.slot[SLOT_CYM].pg_out;

        let h_bit2 = bit(pg_hh, PG_BITS - 8);
        let h_bit7 = bit(pg_hh, PG_BITS - 3);
        let h_bit3 = bit(pg_hh, PG_BITS - 7);

        let c_bit3 = bit(pg_cym, PG_BITS - 7);
        let c_bit5 = bit(pg_cym, PG_BITS - 5);

        self.short_noise = (h_bit2 ^ h_bit7) | (h_bit3 ^ c_bit5) | (c_bit3 ^ c_bit5);
    }

    /// C: `calc_phase`.
    fn calc_phase(&mut self, i: usize, pm_phase: i32, reset: bool) {
        let patch = self.patch_of(i);
        let pm: i8 = if patch.pm != 0 {
            PM_TABLE[((self.slot[i].fnum >> 6) & 7) as usize][((pm_phase >> 10) & 7) as usize]
        } else {
            0
        };
        let ml = ML_TABLE[patch.ml as usize];
        let slot = &mut self.slot[i];
        if reset {
            slot.pg_phase = 0;
        }
        let base = (((slot.fnum as i32 & 0x1ff) * 2 + pm as i32) as u32).wrapping_mul(ml);
        slot.pg_phase = slot.pg_phase.wrapping_add((base << slot.blk) >> 2);
        slot.pg_phase &= DP_WIDTH - 1;
        slot.pg_out = slot.pg_phase >> DP_BASE_BITS;
    }

    /// C: `start_envelope`.
    fn start_envelope(&mut self, i: usize) {
        let ar = self.patch_of(i).ar as i32;
        let slot = &mut self.slot[i];
        if 15.min(ar + (slot.rks as i32 >> 2)) == 15 {
            slot.eg_state = DECAY;
            slot.eg_out = 0;
        } else {
            slot.eg_state = ATTACK;
        }
        self.request_update(i, UPDATE_EG);
    }

    /// C: `calc_envelope`. `eg_counter` is the 16-bit-truncated global
    /// envelope counter (truncated at the call site in C).
    fn calc_envelope(&mut self, i: usize, buddy: Option<usize>, eg_counter: u16, test: bool) {
        let mask: u32 = (1u32 << self.slot[i].eg_shift) - 1;

        {
            let slot = &self.slot[i];
            if slot.eg_state == ATTACK {
                if 0 < slot.eg_out && 0 < slot.eg_rate_h && (eg_counter as u32 & mask & !3) == 0 {
                    let s = lookup_attack_step(slot, eg_counter as u32);
                    if 0 < s {
                        let eg_out = slot.eg_out as i32;
                        self.slot[i].eg_out = 0.max(eg_out - (eg_out >> s) - 1) as u32;
                    }
                }
            } else if slot.eg_rate_h > 0 && (eg_counter as u32 & mask) == 0 {
                let step = lookup_decay_step(slot, eg_counter as u32) as i32;
                self.slot[i].eg_out = EG_MUTE.min(slot.eg_out as i32 + step) as u32;
            }
        }

        match self.slot[i].eg_state {
            // DAMP to ATTACK transition occurs when the envelope reaches
            // EG_MAX (max attenuation but not mute), synchronized with the
            // envelope progress via (eg_counter & mask) == 0.
            DAMP if self.slot[i].eg_out as i32 >= EG_MAX && (eg_counter as u32 & mask) == 0 => {
                self.start_envelope(i);
                if self.slot[i].type_ & 1 != 0 {
                    if self.slot[i].pg_keep == 0 {
                        self.slot[i].pg_phase = 0;
                    }
                    if let Some(b) = buddy
                        && self.slot[b].pg_keep == 0
                    {
                        self.slot[b].pg_phase = 0;
                    }
                }
            }
            ATTACK if self.slot[i].eg_out == 0 => {
                self.slot[i].eg_state = DECAY;
                self.request_update(i, UPDATE_EG);
            }
            // DECAY to SUSTAIN transition is checked at every cycle,
            // regardless of envelope rate/counter conditions.
            DECAY if (self.slot[i].eg_out >> 3) == self.patch_of(i).sl => {
                self.slot[i].eg_state = SUSTAIN;
                self.request_update(i, UPDATE_EG);
            }
            _ => {}
        }

        if test {
            self.slot[i].eg_out = 0;
        }
    }

    /// C: `update_slots`.
    fn update_slots(&mut self) {
        self.eg_counter = self.eg_counter.wrapping_add(1);

        for i in 0..18 {
            let buddy = match self.slot[i].type_ {
                0 => Some(i + 1),
                1 => Some(i - 1),
                _ => None,
            };
            if self.slot[i].update_requests != 0 {
                self.commit_slot_update(i);
            }
            self.calc_envelope(i, buddy, self.eg_counter as u16, self.test_flag & 1 != 0);
            self.calc_phase(i, self.pm_phase as i32, self.test_flag & 4 != 0);
        }
    }

    /// C: `calc_slot_car`.
    fn calc_slot_car(&mut self, ch: usize, fm: i16) -> i16 {
        let i = car_idx(ch);
        let am: i16 = if self.patch_of(i).am != 0 {
            self.lfo_am as i16
        } else {
            0
        };
        let slot = &self.slot[i];
        let idx = (slot.pg_out.wrapping_add((2 * ((fm as i32) >> 1)) as u32)
            & (PG_WIDTH as u32 - 1)) as usize;
        let value = to_linear(TABLES.wave(slot.wave_table, idx), slot, am);
        let slot = &mut self.slot[i];
        slot.output[1] = slot.output[0];
        slot.output[0] = value as i32;
        slot.output[0] as i16
    }

    /// C: `calc_slot_mod`.
    fn calc_slot_mod(&mut self, ch: usize) -> i16 {
        let i = mod_idx(ch);
        let patch = self.patch_of(i);
        let fb = patch.fb;
        let am: i16 = if patch.am != 0 { self.lfo_am as i16 } else { 0 };
        let slot = &self.slot[i];
        let fm: i16 = if fb > 0 {
            ((slot.output[1] + slot.output[0]) >> (9 - fb)) as i16
        } else {
            0
        };
        let idx = (slot.pg_out.wrapping_add(fm as i32 as u32) & (PG_WIDTH as u32 - 1)) as usize;
        let value = to_linear(TABLES.wave(slot.wave_table, idx), slot, am);
        let slot = &mut self.slot[i];
        slot.output[1] = slot.output[0];
        slot.output[0] = value as i32;
        slot.output[0] as i16
    }

    /// C: `calc_slot_tom`.
    fn calc_slot_tom(&mut self) -> i16 {
        let slot = &self.slot[mod_idx(8)];
        to_linear(TABLES.wave(slot.wave_table, slot.pg_out as usize), slot, 0)
    }

    /// C: `calc_slot_snare`.
    fn calc_slot_snare(&mut self) -> i16 {
        let slot = &self.slot[car_idx(7)];

        let phase: usize = if bit(slot.pg_out, PG_BITS - 2) != 0 {
            if self.noise & 1 != 0 { 0x300 } else { 0x200 }
        } else if self.noise & 1 != 0 {
            0x0
        } else {
            0x100
        };

        to_linear(TABLES.wave(slot.wave_table, phase), slot, 0)
    }

    /// C: `calc_slot_cym`.
    fn calc_slot_cym(&mut self) -> i16 {
        let slot = &self.slot[car_idx(8)];
        let phase: usize = if self.short_noise != 0 { 0x300 } else { 0x100 };
        to_linear(TABLES.wave(slot.wave_table, phase), slot, 0)
    }

    /// C: `calc_slot_hat`.
    fn calc_slot_hat(&mut self) -> i16 {
        let slot = &self.slot[mod_idx(7)];

        let phase: usize = if self.short_noise != 0 {
            if self.noise & 1 != 0 { 0x2d0 } else { 0x234 }
        } else if self.noise & 1 != 0 {
            0x34
        } else {
            0xd0
        };

        to_linear(TABLES.wave(slot.wave_table, phase), slot, 0)
    }

    /// C: `update_output`.
    fn update_output(&mut self) {
        self.update_ampm();
        self.update_short_noise();
        self.update_slots();

        /* CH1-6 */
        for i in 0..6 {
            if self.mask & (1 << i) == 0 {
                let fm = self.calc_slot_mod(i);
                self.ch_out[i] = mo(self.calc_slot_car(i, fm));
            }
        }

        /* CH7 */
        if self.rhythm_mode == 0 {
            if self.mask & (1 << 6) == 0 {
                let fm = self.calc_slot_mod(6);
                self.ch_out[6] = mo(self.calc_slot_car(6, fm));
            }
        } else if self.mask & (1 << 13) == 0 {
            // OPLL_MASK_BD
            let fm = self.calc_slot_mod(6);
            self.ch_out[9] = ro(self.calc_slot_car(6, fm));
        }
        self.update_noise(14);

        /* CH8 */
        if self.rhythm_mode == 0 {
            if self.mask & (1 << 7) == 0 {
                let fm = self.calc_slot_mod(7);
                self.ch_out[7] = mo(self.calc_slot_car(7, fm));
            }
        } else {
            if self.mask & (1 << 9) == 0 {
                // OPLL_MASK_HH
                self.ch_out[10] = ro(self.calc_slot_hat());
            }
            if self.mask & (1 << 12) == 0 {
                // OPLL_MASK_SD
                self.ch_out[11] = ro(self.calc_slot_snare());
            }
        }
        self.update_noise(2);

        /* CH9 */
        if self.rhythm_mode == 0 {
            if self.mask & (1 << 8) == 0 {
                let fm = self.calc_slot_mod(8);
                self.ch_out[8] = mo(self.calc_slot_car(8, fm));
            }
        } else {
            if self.mask & (1 << 11) == 0 {
                // OPLL_MASK_TOM
                self.ch_out[12] = ro(self.calc_slot_tom());
            }
            if self.mask & (1 << 10) == 0 {
                // OPLL_MASK_CYM
                self.ch_out[13] = ro(self.calc_slot_cym());
            }
        }
        self.update_noise(2);
    }

    /// C: `mix_output_stereo` (the rate converter branch is never taken).
    fn mix_output_stereo(&mut self) {
        self.mix_out[0] = 0;
        self.mix_out[1] = 0;
        for i in 0..14 {
            if self.pan[i] & 2 != 0 {
                let v = (self.ch_out[i] as f32 * self.pan_fine[i][0]) as i16;
                self.mix_out[0] = self.mix_out[0].wrapping_add(v);
            }
            if self.pan[i] & 1 != 0 {
                let v = (self.ch_out[i] as f32 * self.pan_fine[i][1]) as i16;
                self.mix_out[1] = self.mix_out[1].wrapping_add(v);
            }
        }
    }

    /// C: `OPLL_writeReg`.
    pub fn write_reg(&mut self, reg: u32, data: u8) {
        let mut reg = reg;

        /* mirror registers */
        if (0x19..=0x1f).contains(&reg)
            || (0x29..=0x2f).contains(&reg)
            || (0x39..=0x3f).contains(&reg)
        {
            reg -= 9;
        }

        if reg >= 0x40 {
            return;
        }

        self.reg[reg as usize] = data;
        let data = data as u32;

        match reg {
            0x00 => {
                self.patch[0].am = (data >> 7) & 1;
                self.patch[0].pm = (data >> 6) & 1;
                self.patch[0].eg = (data >> 5) & 1;
                self.patch[0].kr = (data >> 4) & 1;
                self.patch[0].ml = data & 15;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(mod_idx(i), UPDATE_RKS | UPDATE_EG);
                    }
                }
            }
            0x01 => {
                self.patch[1].am = (data >> 7) & 1;
                self.patch[1].pm = (data >> 6) & 1;
                self.patch[1].eg = (data >> 5) & 1;
                self.patch[1].kr = (data >> 4) & 1;
                self.patch[1].ml = data & 15;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(car_idx(i), UPDATE_RKS | UPDATE_EG);
                    }
                }
            }
            0x02 => {
                self.patch[0].kl = (data >> 6) & 3;
                self.patch[0].tl = data & 63;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(mod_idx(i), UPDATE_TLL);
                    }
                }
            }
            0x03 => {
                self.patch[1].kl = (data >> 6) & 3;
                self.patch[1].ws = (data >> 4) & 1;
                self.patch[0].ws = (data >> 3) & 1;
                self.patch[0].fb = data & 7;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(mod_idx(i), UPDATE_WS);
                        self.request_update(car_idx(i), UPDATE_WS | UPDATE_TLL);
                    }
                }
            }
            0x04 => {
                self.patch[0].ar = (data >> 4) & 15;
                self.patch[0].dr = data & 15;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(mod_idx(i), UPDATE_EG);
                    }
                }
            }
            0x05 => {
                self.patch[1].ar = (data >> 4) & 15;
                self.patch[1].dr = data & 15;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(car_idx(i), UPDATE_EG);
                    }
                }
            }
            0x06 => {
                self.patch[0].sl = (data >> 4) & 15;
                self.patch[0].rr = data & 15;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(mod_idx(i), UPDATE_EG);
                    }
                }
            }
            0x07 => {
                self.patch[1].sl = (data >> 4) & 15;
                self.patch[1].rr = data & 15;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.request_update(car_idx(i), UPDATE_EG);
                    }
                }
            }
            0x0e => {
                if self.chip_type == 1 {
                    return;
                }
                self.update_rhythm_mode();
                self.update_key_status();
            }
            0x0f => {
                self.test_flag = data as u8;
            }
            0x10..=0x18 => {
                let ch = (reg - 0x10) as usize;
                let fnum = data as i32 + (((self.reg[0x20 + ch] & 1) as i32) << 8);
                self.set_fnumber(ch, fnum);
            }
            0x20..=0x28 => {
                let ch = (reg - 0x20) as usize;
                let fnum = (((data & 1) as i32) << 8) + self.reg[0x10 + ch] as i32;
                self.set_fnumber(ch, fnum);
                self.set_block(ch, ((data >> 1) & 7) as i32);
                self.set_sus_flag(ch, ((data >> 5) & 1) as u8);
                self.update_key_status();
            }
            0x30..=0x38 => {
                if (self.reg[0x0e] & 32) != 0 && reg >= 0x36 {
                    match reg {
                        0x37 => {
                            let v = (((data >> 4) & 15) as i32) << 2;
                            self.set_slot_volume(mod_idx(7), v);
                        }
                        0x38 => {
                            let v = (((data >> 4) & 15) as i32) << 2;
                            self.set_slot_volume(mod_idx(8), v);
                        }
                        _ => {}
                    }
                } else {
                    self.set_patch((reg - 0x30) as usize, ((data >> 4) & 15) as i32);
                }
                self.set_volume((reg - 0x30) as usize, ((data & 15) as i32) << 2);
            }
            _ => {}
        }
    }

    /// C: `OPLL_writeIO`.
    pub fn write_io(&mut self, adr: u32, val: u8) {
        if adr & 1 != 0 {
            self.write_reg(self.adr, val);
        } else {
            self.adr = val as u32;
        }
    }

    /// Advance the chip by 72 master clocks and return its native channel outputs.
    pub fn clock_channels(&mut self) -> &[i16; 14] {
        self.update_output();
        &self.ch_out
    }

    /// C: `OPLL_calcStereo` (rate converter disabled).
    pub fn calc_stereo(&mut self, out: &mut [i32; 2]) {
        while self.out_step > self.out_time {
            self.out_time += self.inp_step;
            self.update_output();
            self.mix_output_stereo();
        }
        self.out_time -= self.out_step;
        out[0] = self.mix_out[0] as i32;
        out[1] = self.mix_out[1] as i32;
    }

    /// Channel output, as read by the per-oscillator path of the VRC7 wrapper
    /// (`((OPLL *) opll)->ch_out[i]`).
    #[inline]
    pub fn ch_out(&self, i: usize) -> i16 {
        self.ch_out[i]
    }
}
