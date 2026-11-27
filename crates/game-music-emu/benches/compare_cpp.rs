//! Benchmarks comparing the pure Rust emulator vs the C++ wrapper for each format.
//!
//! Run with: `cargo bench --bench compare_cpp`

use criterion::{
    BatchSize, BenchmarkGroup, Criterion, criterion_group, criterion_main, measurement::WallTime,
};
use flate2::read::GzDecoder;
use game_music_emu::GameMusicEmu;
use game_music_emu_wrapper::GameMusicEmu as CppGameMusicEmu;
use std::hint::black_box;
use std::io::Read;

const SAMPLE_RATE: u32 = 44100;
const SAMPLES_PER_SEC: usize = SAMPLE_RATE as usize * 2;

fn bench_rust(group: &mut BenchmarkGroup<WallTime>, data: &[u8], track: i32) {
    group.bench_function("rust", |b| {
        b.iter_batched(
            || {
                let mut emu = GameMusicEmu::from_data(data, SAMPLE_RATE).unwrap();
                emu.start_track(track).unwrap();
                (emu, vec![0i16; SAMPLES_PER_SEC])
            },
            |(mut emu, mut buf)| {
                emu.play(&mut buf).unwrap();
                black_box(buf);
            },
            BatchSize::LargeInput,
        )
    });
}

fn bench_cpp(group: &mut BenchmarkGroup<WallTime>, data: &[u8], track: i32) {
    group.bench_function("cpp", |b| {
        b.iter_batched(
            || {
                let emu = CppGameMusicEmu::from_data(data, SAMPLE_RATE).unwrap();
                emu.start_track(track as usize).unwrap();
                (emu, vec![0i16; SAMPLES_PER_SEC])
            },
            |(emu, mut buf)| {
                emu.play(SAMPLES_PER_SEC, &mut buf).unwrap();
                black_box(buf);
            },
            BatchSize::LargeInput,
        )
    });
}

fn decompress_gz(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    GzDecoder::new(data).read_to_end(&mut out).unwrap();
    out
}

fn bench_format(c: &mut Criterion, name: &str, asset: &str, track: i32) {
    let path = format!("../../{asset}");
    let data = std::fs::read(&path)
        .or_else(|_| std::fs::read(asset))
        .unwrap_or_else(|_| panic!("{asset} not found"));

    let mut group = c.benchmark_group(name);
    bench_rust(&mut group, &data, track);

    // C++ wrapper doesn't handle gzip; pass decompressed data for VGZ.
    let cpp_data = if name == "vgz" {
        decompress_gz(&data)
    } else {
        data.clone()
    };
    bench_cpp(&mut group, &cpp_data, track);
    group.finish();
}

fn compare_benches(c: &mut Criterion) {
    bench_format(c, "nsf", "assets/test.nsf", 0);
    bench_format(c, "nsf_vrc7", "assets/expansion/vrc7.nsf", 0);
    bench_format(c, "spc", "assets/test.spc", 0);
    bench_format(c, "gbs", "assets/test.gbs", 0);
    bench_format(c, "vgm", "assets/test.vgm", 0);
    bench_format(c, "vgz", "assets/test.vgz", 0);
    bench_format(c, "gym", "assets/test.gym", 0);
    bench_format(c, "sap", "assets/test.sap", 0);
    bench_format(c, "kss", "assets/test.kss", 0);
    bench_format(c, "ay", "assets/test.ay", 0);
    bench_format(c, "hes", "assets/test.hes", 1);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30);
    targets = compare_benches
}
criterion_main!(benches);
