#[derive(Clone, Default, Debug)]
pub struct EmuEqualizer {
    pub treble: f64,
    pub bass: f64,
}

impl EmuEqualizer {
    pub fn new(treble: f64, bass: f64) -> Self {
        Self { treble, bass }
    }
}
