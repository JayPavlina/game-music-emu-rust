use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_core::multi_buffer::MultiBuffer;
use gme_gbs::gbs_emu::GbsEmu;

fn main() {
    let data = std::fs::read("../../assets/test.gbs")
        .or_else(|_| std::fs::read("assets/test.gbs"))
        .expect("test.gbs not found");

    let mut emu = GbsEmu::new();
    emu.load_mem_impl(&data).expect("Failed to load");
    emu.set_sample_rate_impl(44100).unwrap();
    emu.start_track_impl(0).unwrap();

    // Print initial APU register state (from SOUND_DATA)
    println!("After start_track_impl:");
    println!("  APU regs after SOUND_DATA init:");
    for (i, reg) in emu.apu.regs[..48].iter().enumerate() {
        print!("{reg:02X} ");
        if (i + 1) % 16 == 0 {
            println!();
        }
    }
    println!();

    // Manually call run_clocks via play_impl
    let mut out = vec![0i16; 44100]; // 500ms stereo
    emu.play_impl(44100, &mut out).unwrap();

    // Save raw samples to file
    let bytes: Vec<u8> = out.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write("/tmp/rust_gbs_samples.bin", &bytes).unwrap();

    // Check stereo buf
    println!("\nStereo buffer state:");
    println!("  Samples available: {}", emu.stereo_buf.samples_avail());
    println!("  Length (msec): {}", emu.stereo_buf.length());

    println!("\nAfter 500ms of playback:");
    println!("  APU regs:");
    for (i, reg) in emu.apu.regs[..48].iter().enumerate() {
        print!("{reg:02X} ");
        if (i + 1) % 16 == 0 {
            println!();
        }
    }
    println!();

    // Check RAM
    println!("\nRAM around APU area (0xFF00-0xFF3F):");
    for addr in (0xFF00u16..0xFF40).step_by(16) {
        let off = (addr - 0xA000) as usize;
        print!("  ${:04X}:", addr);
        for i in 0..16 {
            print!(" {:02X}", emu.ram[off + i]);
        }
        println!();
    }

    // Check first 32 samples
    println!("\nFirst 32 samples (L,R,L,R,...):");
    for s in out.iter().take(32) {
        print!("{} ", s);
    }
    println!();

    // Find first non-zero sample
    let first_nz = out.iter().position(|&s| s != 0).unwrap_or(0);
    println!("\nFirst non-zero sample at index: {}", first_nz);

    // Check non-zero count
    let nz = out.iter().filter(|&&s| s != 0).count();
    let max_abs = out.iter().map(|&s| s.abs()).max().unwrap_or(0);
    println!("Non-zero samples: {}/{}", nz, out.len());
    println!("Max abs sample: {}", max_abs);

    // Dump RAM around play routine state
    println!("\nRAM around play routine state (0xC7C0-0xC7E8):");
    let base = 0xC7C0u16.wrapping_sub(0xA000) as usize;
    for i in (0..0x28).step_by(16) {
        print!("  ${:04X}:", 0xC7C0 + i as u16);
        for j in 0..16 {
            if base + i + j < emu.ram.len() {
                print!(" {:02X}", emu.ram[base + i + j]);
            }
        }
        println!();
    }
}
