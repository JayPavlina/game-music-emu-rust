use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_core::multi_buffer::MultiBuffer;
use gme_nsf::nsf_emu::NsfEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;

fn render_rust(path: &str, mute_mask: i32) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    // Mute after start_track, matching the C++ helper, so both sides
    // pre-fill identically.
    emu.start_track_impl(0).expect("start_track failed");
    emu.mute_voices_impl(mute_mask);

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(4096);
        emu.play_impl(n as i32, &mut out[pos..pos + n])
            .expect("play failed");
        pos += n;
    }
    out
}

fn render_cpp(path: &str, mute_mask: i32) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();
    emu.mute_voices(mute_mask);

    let total_samples = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total_samples];
    let mut pos = 0;
    while pos < total_samples {
        let n = (total_samples - pos).min(4096);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn rms(out: &[i16]) -> f64 {
    let sum: f64 = out.iter().map(|&s| (s as f64).powi(2)).sum();
    (sum / out.len() as f64).sqrt()
}

fn max_abs(out: &[i16]) -> i32 {
    out.iter().map(|s| s.abs() as i32).max().unwrap_or(0)
}

#[test]
fn debug_buffer_growth() {
    let path = "../../assets/expansion/vrc6.nsf";
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.mute_voices_impl(0x1F);
    emu.start_track_impl(0).expect("start_track failed");

    let mut out = vec![0i16; 4096];
    for frame in 0..5 {
        emu.play_impl(2048, &mut out).expect("play failed");
        let buf = unsafe { &mut *emu.stereo_buf.channel(0, 0).center };
        let avail = buf.samples_avail();
        eprintln!(
            "Frame {}: buffer.len()={} samples_avail()={} offset={}",
            frame,
            buf.buffer.len(),
            avail,
            buf.offset
        );
    }
}

#[test]
fn debug_vrc6_in_buffer() {
    let path = "../../assets/expansion/vrc6.nsf";
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.mute_voices_impl(0x1F); // mute APU, keep VRC6
    emu.start_track_impl(0).expect("start_track failed");

    // Run one frame to generate VRC6 deltas
    let mut out = vec![0i16; 4096];
    emu.play_impl(2048, &mut out).expect("play failed");

    // Read the center buffer directly
    let buf = unsafe { &mut *emu.stereo_buf.channel(0, 0).center };
    eprintln!(
        "Buffer size: {}, samples_avail: {}",
        buf.buffer.len(),
        buf.samples_avail()
    );
    // Find first non-zero
    let first_nz = buf
        .buffer
        .iter()
        .position(|&v| v != 0)
        .unwrap_or(buf.buffer.len());
    eprintln!("First non-zero at position: {}", first_nz);
    eprintln!(
        "First 20 buffer values (VRC6 only): {:?}",
        &buf.buffer[first_nz..first_nz + 20.min(buf.buffer.len() - first_nz)]
    );
    eprintln!(
        "Sum of first 100 buffer values: {}",
        buf.buffer[..100.min(buf.buffer.len())].iter().sum::<i32>()
    );
    eprintln!(
        "Non-zero count in first 200: {}",
        buf.buffer[..200.min(buf.buffer.len())]
            .iter()
            .filter(|&&v| v != 0)
            .count()
    );
    eprintln!(
        "Max abs in first 500: {}",
        buf.buffer[..500.min(buf.buffer.len())]
            .iter()
            .map(|v| v.abs())
            .max()
            .unwrap_or(0)
    );
}

#[test]
fn debug_apu_in_buffer() {
    let path = "../../assets/expansion/vrc6.nsf";
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.load_mem_impl(&data).expect("load failed");
    emu.state.set_track_count(emu.track_count);
    emu.set_sample_rate_impl(SAMPLE_RATE as i32)
        .expect("set_sample_rate failed");
    emu.mute_voices_impl(0xE0); // mute VRC6, keep APU
    emu.start_track_impl(0).expect("start_track failed");

    // Run one frame to generate APU deltas
    let mut out = vec![0i16; 4096];
    emu.play_impl(2048, &mut out).expect("play failed");

    // Read the center buffer directly
    let buf = unsafe { &mut *emu.stereo_buf.channel(0, 0).center };
    eprintln!(
        "Buffer size: {}, samples_avail: {}",
        buf.buffer.len(),
        buf.samples_avail()
    );
    eprintln!(
        "First 20 buffer values (APU only): {:?}",
        &buf.buffer[..20.min(buf.buffer.len())]
    );
    eprintln!(
        "Sum of first 100 buffer values: {}",
        buf.buffer[..100.min(buf.buffer.len())].iter().sum::<i32>()
    );
    eprintln!(
        "Non-zero count in first 200: {}",
        buf.buffer[..200.min(buf.buffer.len())]
            .iter()
            .filter(|&&v| v != 0)
            .count()
    );
}

#[test]
fn vrc6_full_mix_comparison() {
    let path = "../../assets/expansion/vrc6.nsf";

    let rust_vrc6 = render_rust(path, 0x1F);
    let rust_apu = render_rust(path, 0xE0);
    let rust_full = render_rust(path, 0);

    let cpp_vrc6 = render_cpp(path, 0x1F);
    let cpp_apu = render_cpp(path, 0xE0);
    let cpp_full = render_cpp(path, 0);

    eprintln!("=== Rust ===");
    eprintln!(
        "VRC6 only:  rms={:.1} max={}",
        rms(&rust_vrc6),
        max_abs(&rust_vrc6)
    );
    eprintln!(
        "APU only:   rms={:.1} max={}",
        rms(&rust_apu),
        max_abs(&rust_apu)
    );
    eprintln!(
        "Full mix:   rms={:.1} max={}",
        rms(&rust_full),
        max_abs(&rust_full)
    );

    eprintln!("=== C++ ===");
    eprintln!(
        "VRC6 only:  rms={:.1} max={}",
        rms(&cpp_vrc6),
        max_abs(&cpp_vrc6)
    );
    eprintln!(
        "APU only:   rms={:.1} max={}",
        rms(&cpp_apu),
        max_abs(&cpp_apu)
    );
    eprintln!(
        "Full mix:   rms={:.1} max={}",
        rms(&cpp_full),
        max_abs(&cpp_full)
    );

    // The full mix should be roughly the sum of VRC6 + APU
    // If the APU produces sound, the full mix should be louder than VRC6 only
    eprintln!("\n=== Analysis ===");
    eprintln!(
        "Rust full/VRC6 ratio: {:.3}",
        rms(&rust_full) / rms(&rust_vrc6)
    );
    eprintln!(
        "C++ full/VRC6 ratio:  {:.3}",
        rms(&cpp_full) / rms(&cpp_vrc6)
    );
}

#[test]
fn fds_comparison() {
    let path = "../../assets/expansion/fds.nsf";

    let rust_fds = render_rust(path, 0x1F);
    let rust_apu = render_rust(path, 0xE0);
    let rust_full = render_rust(path, 0);

    let cpp_fds = render_cpp(path, 0x1F);
    let cpp_apu = render_cpp(path, 0xE0);
    let cpp_full = render_cpp(path, 0);

    eprintln!("=== FDS ===");
    eprintln!(
        "Rust FDS only: rms={:.1} max={}",
        rms(&rust_fds),
        max_abs(&rust_fds)
    );
    eprintln!(
        "Rust APU only: rms={:.1} max={}",
        rms(&rust_apu),
        max_abs(&rust_apu)
    );
    eprintln!(
        "Rust full mix: rms={:.1} max={}",
        rms(&rust_full),
        max_abs(&rust_full)
    );
    eprintln!(
        "C++ FDS only:  rms={:.1} max={}",
        rms(&cpp_fds),
        max_abs(&cpp_fds)
    );
    eprintln!(
        "C++ APU only:  rms={:.1} max={}",
        rms(&cpp_apu),
        max_abs(&cpp_apu)
    );
    eprintln!(
        "C++ full mix:  rms={:.1} max={}",
        rms(&cpp_full),
        max_abs(&cpp_full)
    );
}

#[test]
fn fme7_comparison() {
    let path = "../../assets/expansion/fme7.nsfe";

    let rust_fme7 = render_rust(path, 0x1F);
    let rust_apu = render_rust(path, 0xE0);
    let rust_full = render_rust(path, 0);

    let cpp_fme7 = render_cpp(path, 0x1F);
    let cpp_apu = render_cpp(path, 0xE0);
    let cpp_full = render_cpp(path, 0);

    eprintln!("=== FME7 ===");
    eprintln!(
        "Rust FME7 only: rms={:.1} max={}",
        rms(&rust_fme7),
        max_abs(&rust_fme7)
    );
    eprintln!(
        "Rust APU only:  rms={:.1} max={}",
        rms(&rust_apu),
        max_abs(&rust_apu)
    );
    eprintln!(
        "Rust full mix:  rms={:.1} max={}",
        rms(&rust_full),
        max_abs(&rust_full)
    );
    eprintln!(
        "C++ FME7 only:  rms={:.1} max={}",
        rms(&cpp_fme7),
        max_abs(&cpp_fme7)
    );
    eprintln!(
        "C++ APU only:   rms={:.1} max={}",
        rms(&cpp_apu),
        max_abs(&cpp_apu)
    );
    eprintln!(
        "C++ full mix:   rms={:.1} max={}",
        rms(&cpp_full),
        max_abs(&cpp_full)
    );
}

#[test]
fn per_channel_mute_test() {
    // Test each bit of mute_mask individually to find missing channels
    let path = "../../assets/expansion/vrc6.nsf";

    eprintln!("=== Per-channel mute (VRC6 file) ===");
    for bit in 0..8 {
        let mask = 1 << bit;
        let rust = render_rust(path, mask);
        let cpp = render_cpp(path, mask);
        let label = match bit {
            0 => "bit0=square1",
            1 => "bit1=square2",
            2 => "bit2=triangle",
            3 => "bit3=noise",
            4 => "bit4=dmc",
            // C++ voice order puts the VRC6 saw first
            5 => "bit5=vrc6_saw",
            6 => "bit6=vrc6_p1",
            7 => "bit7=vrc6_p2",
            _ => unreachable!(),
        };
        eprintln!(
            "Mute {}: rust_rms={:.1} cpp_rms={:.1} diff={:.1}",
            label,
            rms(&rust),
            rms(&cpp),
            rms(&rust) - rms(&cpp)
        );
    }
}
