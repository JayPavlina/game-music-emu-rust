use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use game_music_emu::GameMusicEmu;

fn main() {
    let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    println!(
        "Output device: {}",
        device.name().unwrap_or_else(|_| "unknown".to_string())
    );
    let config = device.default_output_config().unwrap();
    let sample_rate = config.sample_rate().0;
    println!("Default output config: {:?}", config);
    println!("Sample rate: {} Hz", sample_rate);

    let game_music_emu = GameMusicEmu::from_file("assets/test.nsf", sample_rate).unwrap();
    game_music_emu.start_track(0).unwrap();
    println!("Stream config: {:?}", config.config());

    let channels = config.channels() as usize;
    let play_f32 = move |output_buffer: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let frame_count = output_buffer.len() / channels;
        let emu_sample_count = frame_count * 2; // emulator outputs stereo
        let mut emu_buffer = vec![0i16; emu_sample_count];
        game_music_emu
            .play(emu_sample_count, &mut emu_buffer)
            .unwrap();

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
        SampleFormat::F32 => device.build_output_stream(&config.config(), play_f32, err_fn, None),
        _ => panic!("only implemented for f32"),
    }
    .unwrap();
    stream.play().unwrap();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
