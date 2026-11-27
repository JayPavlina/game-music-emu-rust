use std::io::Write;

const SAMPLE_RATE: u32 = 44100;

fn write_wav(path: &str, samples: &[i16]) {
    let mut f = std::fs::File::create(std::env::temp_dir().join(path)).unwrap();
    let data_len = (samples.len() * 2) as u32;
    let file_len = data_len + 36;
    // RIFF header
    f.write_all(b"RIFF").unwrap();
    f.write_all(&file_len.to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    // fmt chunk
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&2u16.to_le_bytes()).unwrap(); // stereo
    f.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
    let byte_rate = SAMPLE_RATE * 2 * 2;
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    let block_align = 2u16 * 2;
    f.write_all(&block_align.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    // data chunk
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for s in samples {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
}

fn render_rust(path: &str, secs: u32) -> Vec<i16> {
    use gme_common::gme_file::GmeFile;
    use gme_common::music_emu::MusicEmulator;
    use gme_nsf::nsf_emu::NsfEmu;
    let data = std::fs::read(path).unwrap();
    let mut emu = NsfEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.load_mem_impl(&data).unwrap();
    emu.start_track_impl(0).unwrap();
    let total = (SAMPLE_RATE * secs * 2) as usize;
    let mut out = vec![0i16; total];
    let chunk = SAMPLE_RATE as usize * 2;
    let mut pos = 0;
    while pos < total {
        let n = chunk.min(total - pos);
        emu.play_impl(n as i32, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn render_cpp(path: &str, secs: u32) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();
    let total = (SAMPLE_RATE * secs * 2) as usize;
    let mut out = vec![0i16; total];
    let chunk = SAMPLE_RATE as usize * 2;
    let mut pos = 0;
    while pos < total {
        let n = chunk.min(total - pos);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

/// Listen-check renders for the generated expansion-chip tones.
#[test]
fn write_expansion_wavs() {
    for (path, name) in [
        ("../../assets/expansion/vrc6.nsf", "vrc6"),
        ("../../assets/expansion/fme7.nsfe", "fme7"),
        ("../../assets/expansion/fds.nsf", "fds"),
    ] {
        let rust_out = render_rust(path, 10);
        let cpp_out = render_cpp(path, 10);
        let rust_path = format!("rust_{}.wav", name);
        let cpp_path = format!("cpp_{}.wav", name);
        write_wav(&rust_path, &rust_out);
        write_wav(&cpp_path, &cpp_out);
        eprintln!("Written {} and {}", rust_path, cpp_path);
    }
}

/// Castlevania 3 — DPCM drum listen check.
#[test]
fn write_cv3_wavs() {
    let path = "../../assets/cv3.nsf";
    if !std::path::Path::new(path).exists() {
        eprintln!("cv3.nsf not present (gitignored asset); skipping");
        return;
    }
    let rust_out = render_rust(path, 10);
    let cpp_out = render_cpp(path, 10);
    write_wav("rust_cv3.wav", &rust_out);
    write_wav("cpp_cv3.wav", &cpp_out);
    eprintln!("Written rust_cv3.wav and cpp_cv3.wav");
}

#[test]
fn write_zelda1_wavs() {
    let path = "../../assets/zelda1.nsf";
    if !std::path::Path::new(path).exists() {
        eprintln!("zelda1.nsf not present (gitignored asset); skipping");
        return;
    }
    let rust_out = render_rust(path, 10);
    let cpp_out = render_cpp(path, 10);
    write_wav("rust_zelda1.wav", &rust_out);
    write_wav("cpp_zelda1.wav", &cpp_out);
    eprintln!("Written rust_zelda1.wav ({} samples)", rust_out.len());
    eprintln!("Written cpp_zelda1.wav ({} samples)", cpp_out.len());
}

#[test]
fn write_vrc6_wavs() {
    let path = "../../assets/expansion/vrc6.nsf";
    let rust_out = render_rust(path, 6);
    let cpp_out = render_cpp(path, 6);
    write_wav("rust_vrc6.wav", &rust_out);
    write_wav("cpp_vrc6.wav", &cpp_out);
    eprintln!("Written rust_vrc6.wav ({} samples)", rust_out.len());
    eprintln!("Written cpp_vrc6.wav ({} samples)", cpp_out.len());

    // Check first non-zero
    let rust_first = rust_out.iter().position(|&s| s != 0);
    let cpp_first = cpp_out.iter().position(|&s| s != 0);
    eprintln!("First non-zero: rust={:?} cpp={:?}", rust_first, cpp_first);

    // Check rms at t=0, t=1, t=2
    for s in 0..3 {
        let start = s as usize * SAMPLE_RATE as usize * 2;
        let end = start + SAMPLE_RATE as usize * 2;
        let rust_rms = (rust_out[start..end]
            .iter()
            .map(|&x| (x as f64).powi(2))
            .sum::<f64>()
            / (end - start) as f64)
            .sqrt();
        let cpp_rms = (cpp_out[start..end]
            .iter()
            .map(|&x| (x as f64).powi(2))
            .sum::<f64>()
            / (end - start) as f64)
            .sqrt();
        eprintln!("t={}s: rust rms={:.1} cpp rms={:.1}", s, rust_rms, cpp_rms);
    }

    // Show first 40 samples of both
    eprintln!("\nFirst 40 samples (stereo, L/R interleaved):");
    for i in (0..40).step_by(2) {
        eprintln!("  rust[{}]={} cpp[{}]={}", i, rust_out[i], i, cpp_out[i]);
    }
    eprintln!("\nSamples at t=1s (stereo index 44100):");
    for i in (44100..44120).step_by(2) {
        eprintln!("  rust[{}]={} cpp[{}]={}", i, rust_out[i], i, cpp_out[i]);
    }
}

#[test]
fn write_vrc6_apu_only() {
    use gme_common::gme_file::GmeFile;
    use gme_common::music_emu::MusicEmulator;
    use gme_nsf::nsf_emu::NsfEmu;
    let path = "../../assets/expansion/vrc6.nsf";

    // Rust: mute VRC6 (bits 5-7), keep APU
    let rust_out = {
        let data = std::fs::read(path).unwrap();
        let mut emu = NsfEmu::new();
        emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
        emu.load_mem_impl(&data).unwrap();
        emu.start_track_impl(0).unwrap();
        emu.mute_voices_impl(0xE0); // mute VRC6 (bits 5-7)
        let total = (SAMPLE_RATE * 6 * 2) as usize;
        let mut out = vec![0i16; total];
        let chunk = SAMPLE_RATE as usize * 2;
        let mut pos = 0;
        while pos < total {
            let n = chunk.min(total - pos);
            emu.play_impl(n as i32, &mut out[pos..pos + n]).unwrap();
            pos += n;
        }
        out
    };

    // C++: mute VRC6 (bits 5-7), keep APU
    let cpp_out = {
        let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
        emu.start_track(0).unwrap();
        emu.mute_voices(0xE0); // mute VRC6
        let total = (SAMPLE_RATE * 6 * 2) as usize;
        let mut out = vec![0i16; total];
        let chunk = SAMPLE_RATE as usize * 2;
        let mut pos = 0;
        while pos < total {
            let n = chunk.min(total - pos);
            emu.play(n, &mut out[pos..pos + n]).unwrap();
            pos += n;
        }
        out
    };

    write_wav("rust_vrc6_apu_only.wav", &rust_out);
    write_wav("cpp_vrc6_apu_only.wav", &cpp_out);

    let rust_first = rust_out.iter().position(|&s| s != 0);
    let cpp_first = cpp_out.iter().position(|&s| s != 0);
    eprintln!(
        "APU-only first non-zero: rust={:?} cpp={:?}",
        rust_first, cpp_first
    );

    for s in 0..3 {
        let start = s as usize * SAMPLE_RATE as usize * 2;
        let end = start + SAMPLE_RATE as usize * 2;
        let rust_rms = (rust_out[start..end]
            .iter()
            .map(|&x| (x as f64).powi(2))
            .sum::<f64>()
            / (end - start) as f64)
            .sqrt();
        let cpp_rms = (cpp_out[start..end]
            .iter()
            .map(|&x| (x as f64).powi(2))
            .sum::<f64>()
            / (end - start) as f64)
            .sqrt();
        eprintln!(
            "APU-only t={}s: rust rms={:.1} cpp rms={:.1}",
            s, rust_rms, cpp_rms
        );
    }
}
