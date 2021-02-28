use game_music_emu::GameMusicEmu;
use cpal::{SampleFormat};
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};

fn main() {
    let game_music_emu = GameMusicEmu::from_file("assets/test.nsf", 44100).unwrap();
    game_music_emu.start_track(0).unwrap();

    let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    let config = device.default_output_config().unwrap();
    const BUFFER_SIZE: usize = 1024;
    let mut emu_buffer = [0 as i16; BUFFER_SIZE];

    let play_f32 = move |output_buffer: &mut [f32], _: &cpal::OutputCallbackInfo| {
        game_music_emu.play(BUFFER_SIZE, &mut emu_buffer).unwrap();
        output_buffer.iter_mut().enumerate().for_each(|(i, sample)| {
            *sample = emu_buffer[i] as f32 / 32768.0;
        });
    };

    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_output_stream(&config.config(), play_f32, err_fn),
        _ => panic!("only implemented for f32")
    }.unwrap();
    stream.play().unwrap();

    loop {}
}