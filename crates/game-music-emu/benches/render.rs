//! Renders one second of audio per format through the public API.
//!
//! Each measurement excludes file loading and `start_track` (done in the
//! batch setup), so the numbers are pure decode cost per second of audio —
//! divide 1 s by the reported time to get the realtime multiple.

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use game_music_emu::GameMusicEmu;
use std::hint::black_box;

const SAMPLE_RATE: u32 = 44100;
/// One second of stereo i16 output.
const SAMPLES_PER_SEC: usize = SAMPLE_RATE as usize * 2;

fn bench_format(c: &mut Criterion, name: &str, asset: &str, track: i32) {
    let path = format!("../../{asset}");
    let data = std::fs::read(&path)
        .or_else(|_| std::fs::read(asset))
        .unwrap_or_else(|_| panic!("{asset} not found"));

    c.bench_function(name, |b| {
        b.iter_batched(
            || {
                let mut emu = GameMusicEmu::from_data(&data, SAMPLE_RATE).unwrap();
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

fn render_benches(c: &mut Criterion) {
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
    // test.hes track 0 is genuinely silent; track 1 has audio.
    bench_format(c, "hes", "assets/test.hes", 1);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30);
    targets = render_benches
}
criterion_main!(benches);
