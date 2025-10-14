use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use game_music_emu::GameMusicEmu;

fn main() {
    let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    let config = device.default_output_config().unwrap();
    let sample_rate = config.sample_rate().0 as u32;

    let game_music_emu = GameMusicEmu::from_file("assets/test.nsf", sample_rate).unwrap();
    game_music_emu.start_track(0).unwrap();

    let play_f32 = move |output_buffer: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let len = output_buffer.len();
        let mut emu_buffer = vec![0i16; len];
        game_music_emu.play(len, &mut emu_buffer).unwrap();
        for (sample, &emu_sample) in output_buffer.iter_mut().zip(&emu_buffer) {
            *sample = emu_sample as f32 / i16::MAX as f32;
        }
    };

    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_output_stream(&config.config(), play_f32, err_fn),
        _ => panic!("only implemented for f32"),
    }
    .unwrap();
    stream.play().unwrap();

    loop {}
}
