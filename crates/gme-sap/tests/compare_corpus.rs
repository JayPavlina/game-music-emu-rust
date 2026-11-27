//! Corpus differential test. Set GME_SAP_FILE to diff one file in detail
//! (full mix + per-voice solos), or SAP_TEST_DIR to sweep a directory.

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_sap::sap_emu::SapEmu;

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 3;
const CHUNK_SIZE: usize = 4096;

fn render_rust(data: &[u8], track: i32, mute_mask: i32) -> Vec<i16> {
    let mut emu = SapEmu::new();
    emu.load_mem_impl(data).expect("rust load failed");
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.start_track_impl(track).expect("start_track failed");
    if mute_mask != 0 {
        emu.mute_voices_impl(mute_mask);
    }
    let total = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(CHUNK_SIZE);
        emu.play_impl(n as i32, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn render_cpp(data: &[u8], track: i32, mute_mask: i32) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_data(data, SAMPLE_RATE).unwrap();
    emu.start_track(track as usize).unwrap();
    if mute_mask != 0 {
        emu.mute_voices(mute_mask);
    }
    let total = (RENDER_SECS * SAMPLE_RATE) as usize * 2;
    let mut out = vec![0i16; total];
    let mut pos = 0;
    while pos < total {
        let n = (total - pos).min(CHUNK_SIZE);
        emu.play(n, &mut out[pos..pos + n]).unwrap();
        pos += n;
    }
    out
}

fn rms(s: &[i16]) -> f64 {
    (s.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / s.len() as f64).sqrt()
}

fn diff_stats(rust_out: &[i16], cpp_out: &[i16]) -> (Option<usize>, i32, usize) {
    let mut first = None;
    let mut max = 0i32;
    let mut count = 0usize;
    for i in 0..rust_out.len() {
        let d = (rust_out[i] as i32 - cpp_out[i] as i32).abs();
        if d != 0 {
            if first.is_none() {
                first = Some(i);
            }
            count += 1;
            max = max.max(d);
        }
    }
    (first, max, count)
}

#[test]
fn compare_sap_file_detailed() {
    let Ok(path) = std::env::var("GME_SAP_FILE") else {
        eprintln!("GME_SAP_FILE not set; skipping");
        return;
    };
    let data = std::fs::read(&path).expect("file not found");
    let track: i32 = std::env::var("GME_SAP_TRACK")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);

    let rust_out = render_rust(&data, track, 0);
    let cpp_out = render_cpp(&data, track, 0);
    let (first, max, count) = diff_stats(&rust_out, &cpp_out);
    eprintln!(
        "FULL MIX: rust_rms={:.1} cpp_rms={:.1} first_diff={:?} max_diff={} diff_count={}/{}",
        rms(&rust_out),
        rms(&cpp_out),
        first,
        max,
        count,
        rust_out.len()
    );
    if let Some(i) = first {
        let s = i.saturating_sub(4);
        for j in s..(s + 12).min(rust_out.len()) {
            eprintln!(
                "  [{j}] cpp={:>6} rust={:>6} diff={}",
                cpp_out[j],
                rust_out[j],
                rust_out[j] as i32 - cpp_out[j] as i32
            );
        }
    }

    if std::env::var("SAP_QUICK").is_ok() {
        assert_eq!(first, None, "full mix diverges");
        return;
    }
    // Per-voice solo comparison
    let voices = {
        let mut emu = SapEmu::new();
        emu.load_mem_impl(&data).unwrap();
        emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
        emu.voice_count() as usize
    };
    for v in 0..voices {
        let solo = ((1 << voices) - 1) & !(1 << v);
        let r = render_rust(&data, track, solo);
        let c = render_cpp(&data, track, solo);
        let (first, max, count) = diff_stats(&r, &c);
        eprintln!(
            "SOLO voice {v}: rust_rms={:.1} cpp_rms={:.1} first_diff={:?} max_diff={} count={}",
            rms(&r),
            rms(&c),
            first,
            max,
            count
        );
    }

    assert_eq!(first, None, "full mix diverges");
}

#[test]
fn compare_sap_corpus() {
    let Ok(dir) = std::env::var("SAP_TEST_DIR") else {
        eprintln!("SAP_TEST_DIR not set; skipping");
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("bad dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("sap"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();

    let mut failures = Vec::new();
    for f in &files {
        let data = std::fs::read(f).unwrap();
        // The C++ side may reject some files; skip those.
        let Ok(cpp_emu) = game_music_emu_wrapper::GameMusicEmu::from_data(&data, SAMPLE_RATE)
        else {
            eprintln!("SKIP (cpp rejects): {}", f.display());
            continue;
        };
        drop(cpp_emu);
        let rust_out = render_rust(&data, 0, 0);
        let cpp_out = render_cpp(&data, 0, 0);
        let (first, max, count) = diff_stats(&rust_out, &cpp_out);
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        if first.is_some() {
            eprintln!(
                "DIFF {name}: first={first:?} max={max} count={count} rust_rms={:.1} cpp_rms={:.1}",
                rms(&rust_out),
                rms(&cpp_out)
            );
            failures.push(name);
        } else {
            eprintln!("OK   {name}");
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} corpus files diverge: {:?}",
        failures.len(),
        files.len(),
        failures
    );
}
