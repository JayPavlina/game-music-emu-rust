//! VGM adapter for a YM2413 clocked independently of the PSG and host sample rate.
use gme_core::blip_buffer::{BLIP_MED_QUALITY, BlipBuffer, BlipEq, BlipSynth, BlipTime};
use gme_core::emu2413::Opll;

pub(super) struct Ym2413 {
    opll: Opll,
    clock: i64,
    tick_period: i64,
    next_tick: i64,
    last_amplitude: i32,
    synth: BlipSynth,
    is_vrc7: bool,
}

impl Ym2413 {
    pub fn new(clock: u32, psg_clock: u32, gain: f64, is_vrc7: bool) -> Self {
        let mut opll = Opll::new(clock, clock / 72);
        if is_vrc7 {
            opll.set_chip_type(1);
            opll.reset_patch(1);
        }
        let mut synth = BlipSynth::new(BLIP_MED_QUALITY);
        // A full-scale emu2413 mix is signed 16-bit PCM.
        synth.volume_unit(gain / 32768.0);
        Self {
            opll,
            clock: i64::from(clock),
            tick_period: 72 * i64::from(psg_clock),
            next_tick: 0,
            last_amplitude: 0,
            synth,
            is_vrc7,
        }
    }

    pub fn reset(&mut self) {
        self.opll.reset();
        self.next_tick = 0;
        self.last_amplitude = 0;
    }

    pub fn treble_eq(&mut self, eq: &BlipEq) {
        self.synth.treble_eq(eq);
    }

    pub fn write(&mut self, addr: u8, value: u8) {
        self.opll.write_reg(u32::from(addr), value);
    }

    pub fn run_until(&mut self, end: BlipTime, buffer: &mut BlipBuffer, mute: i32) {
        // Keep the fractional PSG clock across writes and frames. Each native
        // OPLL sample takes 72 chip clocks even when the two clocks differ.
        let end = i64::from(end) * self.clock;
        while self.next_tick < end {
            let channels = self.opll.clock_channels();
            let melodic = if self.is_vrc7 { 6 } else { 9 };
            let mut amplitude = 0;
            for (i, &sample) in channels[..melodic].iter().enumerate() {
                if mute & (1 << i.min(5)) == 0 {
                    amplitude += i32::from(sample);
                }
            }
            if !self.is_vrc7 && mute & 0x40 == 0 {
                amplitude += channels[9..].iter().copied().map(i32::from).sum::<i32>();
            }
            let delta = amplitude - self.last_amplitude;
            if delta != 0 {
                self.synth
                    .offset_at((self.next_tick / self.clock) as BlipTime, delta, buffer);
                self.last_amplitude = amplitude;
            }
            self.next_tick += self.tick_period;
        }
    }

    pub fn end_frame(&mut self, end: BlipTime, buffer: &mut BlipBuffer, mute: i32) {
        self.run_until(end, buffer, mute);
        self.next_tick -= i64::from(end) * self.clock;
    }
}
