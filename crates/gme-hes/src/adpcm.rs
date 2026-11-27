//! Turbo Grafx 16 (PC Engine) ADPCM sound chip emulator.
//! Port of C++ Hes_Apu_Adpcm.h / Hes_Apu_Adpcm.cpp.
//!
//! Note: Hes_Emu never calls set_output()/volume() on this chip, so it never
//! produces audio; only its register/timing state machine is emulated (reads
//! at $180A-$180D observe it). The synth/output plumbing is therefore omitted
//! and the amplitude path mirrors the C++ "output == NULL" behavior: last_amp
//! is never updated and no deltas are emitted.

use gme_core::blip_buffer::BlipTime;

pub const IO_ADDR: u32 = 0x1800;
pub const IO_SIZE: u32 = 0x400;

const STEPSIZE: [i16; 49] = [
    16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130,
    143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796,
    876, 963, 1060, 1166, 1282, 1411, 1552,
];

struct State {
    pcmbuf: Vec<u8>, // 0x10000
    port: [u8; 0x10],
    ad_sample: i32,
    ad_ref_index: i32,
    ad_low_nibble: bool,
    freq: i32,
    addr: u16,
    writeptr: u16,
    readptr: u16,
    playptr: u16,
    playflag: u8,
    repeatflag: u8,
    length: i32,
    playlength: i32,
    playedsamplecount: i32,
    volume: i32,
    fadetimer: i32,
    fadecount: i32,
}

impl State {
    fn new() -> Self {
        Self {
            pcmbuf: vec![0u8; 0x10000],
            port: [0; 0x10],
            ad_sample: 0,
            ad_ref_index: 0,
            ad_low_nibble: false,
            freq: 0,
            addr: 0,
            writeptr: 0,
            readptr: 0,
            playptr: 0,
            playflag: 0,
            repeatflag: 0,
            length: 0,
            playlength: 0,
            playedsamplecount: 0,
            volume: 0,
            fadetimer: 0,
            fadecount: 0,
        }
    }
}

pub struct HesAdpcm {
    state: State,
    last_time: BlipTime,
    next_timer: f64,
    #[allow(dead_code)]
    last_amp: i32, // never updated: output is always NULL in Hes_Emu
}

impl Default for HesAdpcm {
    fn default() -> Self {
        Self::new()
    }
}

impl HesAdpcm {
    pub fn new() -> Self {
        let mut adpcm = Self {
            state: State::new(),
            last_time: 0,
            next_timer: 0.0,
            last_amp: 0,
        };
        adpcm.reset();
        adpcm
    }

    pub fn reset(&mut self) {
        self.last_time = 0;
        self.next_timer = 0.0;
        self.last_amp = 0;

        self.state.pcmbuf.fill(0);
        self.state.port.fill(0);

        self.state.ad_sample = 0;
        self.state.ad_ref_index = 0;

        self.state.addr = 0;
        self.state.freq = 0;
        self.state.writeptr = 0;
        self.state.readptr = 0;
        self.state.playflag = 0;
        self.state.repeatflag = 0;
        self.state.length = 0;
        self.state.volume = 0xFF;
        self.state.fadetimer = 0;
        self.state.fadecount = 0;
    }

    fn run_until(&mut self, end_time: BlipTime) {
        let mut volume = self.state.volume;
        let fadetimer = self.state.fadetimer;
        let mut fadecount = self.state.fadecount;
        let mut last_time = self.last_time;
        let mut next_timer = self.next_timer;

        while self.state.playflag != 0 && last_time < end_time {
            while last_time as f64 >= next_timer {
                if fadetimer != 0 {
                    if fadecount > 0 {
                        fadecount -= 1;
                        volume = 0xFF * fadecount / fadetimer;
                    } else if fadecount < 0 {
                        fadecount += 1;
                        volume = 0xFF - (0xFF * fadecount / fadetimer);
                    }
                }
                next_timer += 7159.091;
            }

            let amp;
            if self.state.ad_low_nibble {
                amp = self
                    .adpcm_decode((self.state.pcmbuf[self.state.playptr as usize] & 0x0F) as i32);
                self.state.ad_low_nibble = false;
                self.state.playptr = self.state.playptr.wrapping_add(1);
                self.state.playedsamplecount += 1;
                if self.state.playedsamplecount == self.state.playlength {
                    self.state.playflag = 0;
                }
            } else {
                amp =
                    self.adpcm_decode((self.state.pcmbuf[self.state.playptr as usize] >> 4) as i32);
                self.state.ad_low_nibble = true;
            }

            let _amp = amp * volume / 0xFF;
            // output == NULL in Hes_Emu: no delta emitted, last_amp untouched.

            last_time += self.state.freq;
        }

        if self.state.playflag == 0 {
            while next_timer <= end_time as f64 {
                next_timer += 7159.091;
            }
            last_time = end_time;
        }

        self.last_time = last_time;
        self.next_timer = next_timer;
        self.state.volume = volume;
        self.state.fadetimer = fadetimer;
        self.state.fadecount = fadecount;
    }

    pub fn write_data(&mut self, time: BlipTime, addr: i32, data: i32) {
        if time > self.last_time {
            self.run_until(time);
        }

        let data = data & 0xFF;
        self.state.port[(addr & 15) as usize] = data as u8;
        match addr & 15 {
            8 => {
                self.state.addr &= 0xFF00;
                self.state.addr |= data as u16;
            }
            9 => {
                self.state.addr &= 0xFF;
                self.state.addr |= (data as u16) << 8;
            }
            10 => {
                self.state.pcmbuf[self.state.writeptr as usize] = data as u8;
                self.state.writeptr = self.state.writeptr.wrapping_add(1);
                self.state.playlength += 1;
            }
            11 => {
                // ADPCM DMA (debug print only in C++)
            }
            13 => {
                if data & 0x80 != 0 {
                    self.state.addr = 0;
                    self.state.freq = 0;
                    self.state.writeptr = 0;
                    self.state.readptr = 0;
                    self.state.playflag = 0;
                    self.state.repeatflag = 0;
                    self.state.length = 0;
                    self.state.volume = 0xFF;
                }

                if (data & 3) == 3 {
                    self.state.writeptr = self.state.addr;
                }

                if data & 8 != 0 {
                    self.state.readptr = if self.state.addr != 0 {
                        self.state.addr - 1
                    } else {
                        self.state.addr
                    };
                }

                if data & 0x10 != 0 {
                    self.state.length = self.state.addr as i32;
                }

                self.state.repeatflag = (data & 0x20) as u8;
                self.state.playflag = (data & 0x40) as u8;

                if self.state.playflag != 0 {
                    self.state.playptr = self.state.readptr;
                    self.state.playlength = self.state.length + 1;
                    self.state.playedsamplecount = 0;
                    self.state.ad_sample = 0;
                    self.state.ad_low_nibble = false;
                }
            }
            14 => {
                self.state.freq = 7159091 / (32000 / (16 - (data & 15)));
            }
            15 => match data & 15 {
                0 | 8 | 12 => {
                    self.state.fadetimer = -100;
                    self.state.fadecount = self.state.fadetimer;
                }
                10 => {
                    self.state.fadetimer = 5000;
                    self.state.fadecount = self.state.fadetimer;
                }
                14 => {
                    self.state.fadetimer = 1500;
                    self.state.fadecount = self.state.fadetimer;
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn read_data(&mut self, time: BlipTime, addr: i32) -> i32 {
        if time > self.last_time {
            self.run_until(time);
        }

        match addr & 15 {
            10 => {
                let v = self.state.pcmbuf[self.state.readptr as usize];
                self.state.readptr = self.state.readptr.wrapping_add(1);
                v as i32
            }
            11 => (self.state.port[11] & !1) as i32,
            12 => {
                if self.state.playflag == 0 {
                    self.state.port[12] |= 1;
                    self.state.port[12] &= !8;
                } else {
                    self.state.port[12] &= !1;
                    self.state.port[12] |= 8;
                }
                self.state.port[12] as i32
            }
            13 => self.state.port[13] as i32,
            _ => 0xFF,
        }
    }

    pub fn end_frame(&mut self, end_time: BlipTime) {
        self.run_until(end_time);
        self.last_time -= end_time;
        self.next_timer -= end_time as f64;
        debug_assert!(self.last_time >= 0);
        // output == NULL: no set_modified
    }

    fn adpcm_decode(&mut self, code: i32) -> i32 {
        let mut step = STEPSIZE[self.state.ad_ref_index as usize] as i32;
        let c = code & 7;

        let mut delta = 0;
        if c & 4 != 0 {
            delta += step;
        }
        step >>= 1;
        if c & 2 != 0 {
            delta += step;
        }
        step >>= 1;
        if c & 1 != 0 {
            delta += step;
        }
        step >>= 1;
        delta += step;

        if c != code {
            self.state.ad_sample -= delta;
            if self.state.ad_sample < -2048 {
                self.state.ad_sample = -2048;
            }
        } else {
            self.state.ad_sample += delta;
            if self.state.ad_sample > 2047 {
                self.state.ad_sample = 2047;
            }
        }

        const STEPS: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];
        self.state.ad_ref_index += STEPS[c as usize];
        self.state.ad_ref_index = self.state.ad_ref_index.clamp(0, 48);

        self.state.ad_sample
    }
}
