use crate::native::{ULong, Long, Int, Double, Short};

// Quality level. Start with BLIP_GOOD_QUALITY.
const BLIP_MED_QUALITY: Int = 8;
const BLIP_GOOD_QUALITY: Int = 12;
const BLIP_HIGH_QUALITY: Int = 16;
const BLIP_SAMPLE_BITS:Int = 30;

pub struct BlipSynth {
    pub buf: BlipBuffer,
    pub last_amp: Int,
    pub delta_factor: Int,

    volume_unit: Double,
    impulses: Vec<Short>,
    width: Int,
    kernel_unit: Long,
}

pub struct BlipEq {
    treble: Double,
    rolloff_freq: Long,
    sample_rate: Long,
    cutoff_freq: Long
}

pub struct BlipBuffer {
    pub factor: ULong,
    pub offset: ULong,
    pub buffer: Vec<Long>,
    pub buffer_size: Long,
    pub reader_accum: Long,
    pub bass_shift: Int,

    sample_rate: Long,
    clock_rate: Long,
    bass_freq: Int,
    length: Int,
    modified: Int,
}