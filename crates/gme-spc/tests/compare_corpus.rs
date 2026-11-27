//! Renders every .spc file under the directory in the SPC_TEST_DIR env var
//! through both the Rust port and the C++ reference, asserting bit-exact
//! output. Skips (with a message) when SPC_TEST_DIR is unset, since real SPC
//! rips are copyrighted and live outside the repo.
//!
//! Usage:
//!   SPC_TEST_DIR=/path/to/spc/dir cargo test -p gme-spc --test compare_corpus -- --nocapture

use gme_common::gme_file::GmeFile;
use gme_common::music_emu::MusicEmulator;
use gme_spc::spc_emu::SpcEmu;
use std::path::{Path, PathBuf};

const SAMPLE_RATE: u32 = 44100;
const RENDER_SECS: u32 = 5;
const CHUNK_SIZE: usize = 4096;

fn collect_spc_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_spc_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("spc"))
        {
            out.push(path);
        }
    }
}

fn render_cpp(path: &Path) -> Vec<i16> {
    let emu = game_music_emu_wrapper::GameMusicEmu::from_file(path, SAMPLE_RATE).unwrap();
    emu.start_track(0).unwrap();
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

fn render_rust(path: &Path) -> Vec<i16> {
    let data = std::fs::read(path).unwrap();
    let mut emu = SpcEmu::new();
    emu.set_sample_rate_impl(SAMPLE_RATE as i32).unwrap();
    emu.load_mem_impl(&data).unwrap();
    emu.start_track_impl(0).unwrap();
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

#[test]
fn compare_corpus() {
    let Some(dir) = std::env::var_os("SPC_TEST_DIR") else {
        println!("SPC_TEST_DIR not set — skipping corpus comparison");
        return;
    };
    let mut files = Vec::new();
    collect_spc_files(Path::new(&dir), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no .spc files found under SPC_TEST_DIR");
    println!(
        "comparing {} SPC files ({}s each)...",
        files.len(),
        RENDER_SECS
    );

    let mut failures = Vec::new();
    for path in &files {
        let cpp = render_cpp(path);
        let rust = render_rust(path);
        let diff = cpp.iter().zip(rust.iter()).position(|(a, b)| a != b);
        match diff {
            None => println!("OK  {}", path.display()),
            Some(d) => {
                let differing = cpp.iter().zip(rust.iter()).filter(|(a, b)| a != b).count();
                println!(
                    "DIFF {} — first at sample {} ({} of {} differ)",
                    path.display(),
                    d,
                    differing,
                    cpp.len()
                );
                failures.push(path.clone());
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} files differ from C++: {:?}",
        failures.len(),
        files.len(),
        failures
    );
}
