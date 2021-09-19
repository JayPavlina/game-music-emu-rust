use crate::native::{Int, Long};

#[derive(Default)]
pub struct MultiBuffer {
    // unsigned channels_changed_count_;
    sample_rate: Long,
    length: Int,
    //TODO: I think this is a const?
    samples_per_frame: Int,
}