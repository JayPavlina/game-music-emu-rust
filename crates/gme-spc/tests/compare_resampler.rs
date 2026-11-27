//! Differential test of the Rust FirResampler<24> against the C++
//! Fir_Resampler<24> (via the wrapper shim), using the SPC emu's exact
//! configuration (32000 -> 44100, rolloff 0.9965) and a deterministic input.

use game_music_emu_wrapper::spc_dsp_shim::Fir24Shim;
use gme_core::fir_resampler::FirResampler;

const NATIVE: f64 = 32000.0;
const OUT_RATE: f64 = 44100.0;
const BUF_SIZE: usize = 3200; // native_sample_rate / 20 * 2

/// Deterministic pseudo-audio: a couple of sine-ish integer waves plus
/// silence at the start (mimics SPC output shape).
fn test_signal(len: usize) -> Vec<i16> {
    let mut v = vec![0i16; len];
    for (i, pair) in v.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        if i < 500 {
            continue; // leading silence
        }
        let t = i as f64;
        let s = (t * 0.13).sin() * 9000.0 + (t * 0.031).sin() * 5000.0;
        pair[0] = s as i16;
        pair[1] = (s * 0.8) as i16;
    }
    v
}

#[test]
fn compare_fir_resampler() {
    let input = test_signal(64000);

    let ratio = NATIVE / OUT_RATE;

    let mut cpp = Fir24Shim::new();
    cpp.buffer_size(BUF_SIZE as i32);
    let cpp_ratio = cpp.time_ratio(ratio, 0.9965, 1.0);

    let mut rust = FirResampler::<24>::new();
    rust.buffer_size(BUF_SIZE);
    let rust_ratio = rust.time_ratio(ratio, 0.9965, 1.0);

    println!("ratio: cpp={} rust={}", cpp_ratio, rust_ratio);
    assert_eq!(cpp_ratio, rust_ratio, "time_ratio results differ");

    // Drive both with identical chunked writes/reads like Spc_Emu::play_
    let out_total = 80000;
    let mut cpp_out = vec![0i16; out_total];
    let mut rust_out = vec![0i16; out_total];

    let in_pos;
    {
        let mut pos = 0usize;
        let count = out_total;
        let mut remain = count;
        while remain > 0 {
            let got = cpp.read(&mut cpp_out[count - remain..]) as usize;
            remain -= got;
            if remain > 0 {
                let n = cpp.max_write() as usize;
                let n = n.min(input.len() - pos);
                if n == 0 {
                    break;
                }
                cpp.write_input(&input[pos..pos + n]);
                pos += n;
            }
        }
        in_pos = pos;
    }
    {
        let mut pos = 0usize;
        let count = out_total;
        let mut remain = count;
        while remain > 0 {
            let got = rust.read(&mut rust_out[count - remain..]) as usize;
            remain -= got;
            if remain > 0 {
                let n = rust.max_write();
                let n = n.min(input.len() - pos);
                if n == 0 {
                    break;
                }
                rust.buffer_ptr()[..n].copy_from_slice(&input[pos..pos + n]);
                rust.write(n);
                pos += n;
            }
        }
        assert_eq!(in_pos, pos, "input consumption differs");
    }

    let diff = cpp_out
        .iter()
        .zip(rust_out.iter())
        .position(|(a, b)| a != b);
    if let Some(d) = diff {
        let lo = d.saturating_sub(4);
        for i in lo..(d + 8).min(out_total) {
            println!(
                "{} {:6}: cpp={:6} rust={:6}",
                if i == d { ">" } else { " " },
                i,
                cpp_out[i],
                rust_out[i]
            );
        }
        panic!("resampler outputs differ at sample {}", d);
    }
    println!("OK: {} resampled samples bit-exact", out_total);
}
