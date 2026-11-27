use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use game_music_emu::GameMusicEmu;
use std::sync::{Arc, Mutex};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        eprintln!("Usage: play_nsf <file> [track]");
        eprintln!("  Supports NSF, SPC, GBS (depending on enabled features)");
        eprintln!("  Playing default: assets/test.nsf");
        "assets/test.nsf".to_string()
    });
    let track: i32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let err_fn = |err| eprintln!("audio stream error: {}", err);
    let host = cpal::default_host();
    let device = host.default_output_device().expect("no output device");
    println!("Output device: {}", device);

    let config = device.default_output_config().unwrap();
    let sample_rate = config.sample_rate();
    println!("Sample rate: {} Hz", sample_rate);

    let mut emu = GameMusicEmu::from_file(&path, sample_rate).unwrap_or_else(|e| {
        eprintln!("Failed to load {}: {}", path, e);
        std::process::exit(1);
    });

    let track_count = emu.track_count();
    println!("Loaded: {} ({} tracks)", path, track_count);

    if let Ok(info) = emu.track_info(track) {
        if !info.game.is_empty() {
            println!("Game: {}", info.game);
        }
        if !info.song.is_empty() {
            println!("Song: {}", info.song);
        }
        if !info.author.is_empty() {
            println!("Author: {}", info.author);
        }
    }

    emu.start_track(track).unwrap();
    println!("Playing track {}... (Ctrl+C to stop)", track);

    let emu = Arc::new(Mutex::new(emu));
    let channels = config.channels() as usize;
    let emu_clone = Arc::clone(&emu);

    let play_f32 = move |output_buffer: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let frame_count = output_buffer.len() / channels;
        let emu_sample_count = frame_count * 2; // stereo
        let mut emu_buffer = vec![0i16; emu_sample_count];

        if let Ok(mut emu) = emu_clone.lock() {
            let _ = emu.play(&mut emu_buffer);
        }

        for frame in 0..frame_count {
            let left = emu_buffer[frame * 2] as f32 / i16::MAX as f32;
            let right = emu_buffer[frame * 2 + 1] as f32 / i16::MAX as f32;
            let base = frame * channels;
            output_buffer[base] = left;
            if channels > 1 {
                output_buffer[base + 1] = right;
            }
            for ch in 2..channels {
                output_buffer[base + ch] = 0.0;
            }
        }
    };

    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_output_stream(config.config(), play_f32, err_fn, None),
        _ => panic!("only F32 sample format supported"),
    }
    .unwrap();
    stream.play().unwrap();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
