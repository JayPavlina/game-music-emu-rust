//! Test-only bindings to the C++ `Spc_Dsp` (via `spc_dsp_shim.cpp`), so the Rust
//! port's DSP can be differentially tested in isolation from the SPC700 CPU.

use std::ffi::c_void;

#[repr(C)]
struct RawSpcDsp {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn spc_dsp_trace_reset();
    fn spc_dsp_trace_len() -> i32;
    fn spc_dsp_trace_get(i: i32, time: *mut i32, addr: *mut i32, data: *mut i32);
    fn spc_dsp_shim_new() -> *mut RawSpcDsp;
    fn spc_dsp_shim_delete(dsp: *mut RawSpcDsp);
    fn spc_dsp_shim_init(dsp: *mut RawSpcDsp, ram_64k: *mut c_void);
    fn spc_dsp_shim_load(dsp: *mut RawSpcDsp, regs: *const u8);
    fn spc_dsp_shim_set_output(dsp: *mut RawSpcDsp, out: *mut i16, out_size: i32);
    fn spc_dsp_shim_run(dsp: *mut RawSpcDsp, clock_count: i32);
    fn spc_dsp_shim_sample_count(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_read(dsp: *const RawSpcDsp, addr: i32) -> i32;
    fn spc_dsp_shim_write(dsp: *mut RawSpcDsp, addr: i32, data: i32);
    fn spc_dsp_shim_mute_voices(dsp: *mut RawSpcDsp, mask: i32);
    fn spc_dsp_shim_voice_env(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_voice_env_mode(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_voice_kon_delay(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_voice_interp_pos(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_voice_brr_addr(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_voice_brr_offset(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_voice_hidden_env(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_voice_buf_pos(dsp: *const RawSpcDsp, v: i32) -> i32;
    fn spc_dsp_shim_kon(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_new_kon(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_t_koff(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_noise(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_every_other_sample(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_echo_offset(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_phase(dsp: *const RawSpcDsp) -> i32;
    fn spc_dsp_shim_counter(dsp: *const RawSpcDsp, i: i32) -> u32;
}

/// Clears the global C++ DSP-register write trace (fed by SPC_DSP_WRITE_HOOK
/// inside Snes_Spc::dsp_write). Process-global; tests using it must not run
/// other SPC emulation concurrently.
pub fn dsp_write_trace_reset() {
    unsafe { spc_dsp_trace_reset() }
}

/// Returns the C++ DSP-register write trace as (time, addr, data) tuples.
/// addr 0x100 marks a dsp.run() event, with time = clock_count.
pub fn dsp_write_trace() -> Vec<(i32, u16, u8)> {
    let len = unsafe { spc_dsp_trace_len() };
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let (mut t, mut a, mut d) = (0i32, 0i32, 0i32);
        unsafe { spc_dsp_trace_get(i, &mut t, &mut a, &mut d) };
        out.push((t, a as u16, d as u8));
    }
    out
}

#[repr(C)]
struct RawFir24 {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn fir24_shim_new() -> *mut RawFir24;
    fn fir24_shim_delete(r: *mut RawFir24);
    fn fir24_shim_buffer_size(r: *mut RawFir24, size: i32);
    fn fir24_shim_time_ratio(r: *mut RawFir24, factor: f64, rolloff: f64, gain: f64) -> f64;
    fn fir24_shim_clear(r: *mut RawFir24);
    fn fir24_shim_max_write(r: *const RawFir24) -> i32;
    fn fir24_shim_buffer(r: *mut RawFir24) -> *mut i16;
    fn fir24_shim_write(r: *mut RawFir24, count: std::ffi::c_long);
    fn fir24_shim_read(r: *mut RawFir24, out: *mut i16, count: i32) -> i32;
    fn fir24_shim_skip_input(r: *mut RawFir24, count: std::ffi::c_long) -> i32;
}

/// Bindings to the C++ `Fir_Resampler<24>` used by Spc_Emu, for differential
/// testing of the Rust FirResampler port.
pub struct Fir24Shim {
    r: *mut RawFir24,
}

impl Fir24Shim {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            r: unsafe { fir24_shim_new() },
        }
    }

    pub fn buffer_size(&mut self, size: i32) {
        unsafe { fir24_shim_buffer_size(self.r, size) }
    }

    pub fn time_ratio(&mut self, factor: f64, rolloff: f64, gain: f64) -> f64 {
        unsafe { fir24_shim_time_ratio(self.r, factor, rolloff, gain) }
    }

    pub fn clear(&mut self) {
        unsafe { fir24_shim_clear(self.r) }
    }

    pub fn max_write(&self) -> i32 {
        unsafe { fir24_shim_max_write(self.r) }
    }

    /// Writes input samples into the resampler's buffer.
    pub fn write_input(&mut self, samples: &[i16]) {
        unsafe {
            let buf = fir24_shim_buffer(self.r);
            std::ptr::copy_nonoverlapping(samples.as_ptr(), buf, samples.len());
            fir24_shim_write(self.r, samples.len() as std::ffi::c_long);
        }
    }

    pub fn read(&mut self, out: &mut [i16]) -> i32 {
        unsafe { fir24_shim_read(self.r, out.as_mut_ptr(), out.len() as i32) }
    }

    pub fn skip_input(&mut self, count: i64) -> i32 {
        unsafe { fir24_shim_skip_input(self.r, count as std::ffi::c_long) }
    }
}

impl Drop for Fir24Shim {
    fn drop(&mut self) {
        unsafe { fir24_shim_delete(self.r) }
    }
}

/// Owns a C++ `Spc_Dsp` plus the 64K RAM it operates on. RAM lives in a `Box`
/// so its address stays stable even if this struct is moved.
pub struct SpcDspShim {
    dsp: *mut RawSpcDsp,
    ram: Box<[u8; 0x10000]>,
}

impl SpcDspShim {
    /// Creates a DSP initialized with the given 64K RAM contents.
    pub fn new(ram_64k: &[u8]) -> Self {
        assert_eq!(ram_64k.len(), 0x10000);
        let mut ram: Box<[u8; 0x10000]> = vec![0u8; 0x10000].into_boxed_slice().try_into().unwrap();
        ram.copy_from_slice(ram_64k);
        let dsp = unsafe { spc_dsp_shim_new() };
        unsafe { spc_dsp_shim_init(dsp, ram.as_mut_ptr() as *mut c_void) };
        Self { dsp, ram }
    }

    pub fn load(&mut self, regs: &[u8; 128]) {
        unsafe { spc_dsp_shim_load(self.dsp, regs.as_ptr()) }
    }

    /// Runs the DSP for `clock_count` clocks, writing stereo samples into `out`.
    /// Returns the number of i16 samples written.
    pub fn run(&mut self, clock_count: i32, out: &mut [i16]) -> usize {
        unsafe {
            spc_dsp_shim_set_output(self.dsp, out.as_mut_ptr(), out.len() as i32);
            spc_dsp_shim_run(self.dsp, clock_count);
            spc_dsp_shim_sample_count(self.dsp) as usize
        }
    }

    pub fn read(&self, addr: i32) -> u8 {
        unsafe { spc_dsp_shim_read(self.dsp, addr) as u8 }
    }

    pub fn write(&mut self, addr: i32, data: u8) {
        unsafe { spc_dsp_shim_write(self.dsp, addr, data as i32) }
    }

    pub fn mute_voices(&mut self, mask: i32) {
        unsafe { spc_dsp_shim_mute_voices(self.dsp, mask) }
    }

    pub fn ram(&self) -> &[u8; 0x10000] {
        &self.ram
    }

    pub fn voice_env(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_env(self.dsp, v as i32) }
    }

    pub fn voice_env_mode(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_env_mode(self.dsp, v as i32) }
    }

    pub fn voice_kon_delay(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_kon_delay(self.dsp, v as i32) }
    }

    pub fn voice_interp_pos(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_interp_pos(self.dsp, v as i32) }
    }

    pub fn voice_brr_addr(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_brr_addr(self.dsp, v as i32) }
    }

    pub fn voice_brr_offset(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_brr_offset(self.dsp, v as i32) }
    }

    pub fn voice_hidden_env(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_hidden_env(self.dsp, v as i32) }
    }

    pub fn voice_buf_pos(&self, v: usize) -> i32 {
        unsafe { spc_dsp_shim_voice_buf_pos(self.dsp, v as i32) }
    }

    pub fn kon(&self) -> i32 {
        unsafe { spc_dsp_shim_kon(self.dsp) }
    }

    pub fn new_kon(&self) -> i32 {
        unsafe { spc_dsp_shim_new_kon(self.dsp) }
    }

    pub fn t_koff(&self) -> i32 {
        unsafe { spc_dsp_shim_t_koff(self.dsp) }
    }

    pub fn noise(&self) -> i32 {
        unsafe { spc_dsp_shim_noise(self.dsp) }
    }

    pub fn every_other_sample(&self) -> i32 {
        unsafe { spc_dsp_shim_every_other_sample(self.dsp) }
    }

    pub fn echo_offset(&self) -> i32 {
        unsafe { spc_dsp_shim_echo_offset(self.dsp) }
    }

    pub fn phase(&self) -> i32 {
        unsafe { spc_dsp_shim_phase(self.dsp) }
    }

    pub fn counter(&self, i: usize) -> u32 {
        unsafe { spc_dsp_shim_counter(self.dsp, i as i32) }
    }
}

impl Drop for SpcDspShim {
    fn drop(&mut self) {
        unsafe { spc_dsp_shim_delete(self.dsp) }
    }
}
