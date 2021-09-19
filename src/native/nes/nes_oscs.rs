use crate::native::core::BlipBuffer;
use crate::native::Int;

struct NesOsc {
    regs: [u8; 4],
    reg_written: [bool; 4],
    output: BlipBuffer,
    /// length counter (0 if unused by oscillator)
    length_counter: Int,
    /// delay until next (potential) transition
    delay: Int,
    last_amp: Int
}

struct NesEnvelope {
    osc: NesOsc,
    envelope: Int,
    env_delay: Int
}

pub struct NesSquare {
    envelope: NesEnvelope,
    phase: Int,
    sweep_delay: Int
}