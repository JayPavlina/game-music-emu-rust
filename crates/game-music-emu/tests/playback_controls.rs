use game_music_emu::{Format, GameMusicEmu, type_list};

fn asset(extension: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/test.{extension}"))
}

#[test]
fn fade_ends_each_format_and_restart_cancels_it() {
    for &format in type_list() {
        let path = asset(&format.info().extension.to_ascii_lowercase());
        let mut emu = GameMusicEmu::from_file(path, 44100).unwrap();
        let track = i32::from(format == Format::Hes);
        emu.start_track(track).unwrap();
        emu.set_fade(10, 20);
        let mut buf = [0; 1024];
        for _ in 0..16 {
            emu.play(&mut buf).unwrap();
        }
        assert!(emu.track_ended(), "{format:?} fade");
        emu.play(&mut buf).unwrap();
        assert!(buf.iter().all(|&s| s == 0), "{format:?} after fade");
        emu.start_track(track).unwrap();
        for _ in 0..16 {
            emu.play(&mut buf).unwrap();
        }
        assert!(!emu.track_ended(), "{format:?} restart");
    }
}

#[cfg(feature = "ay")]
#[test]
fn ay_load_rejects_an_incomplete_track_table() {
    let mut header = std::fs::read(asset("ay")).unwrap();
    header.truncate(0x14);
    header[0x10] = 7; // Eight four-byte entries are required.
    assert!(GameMusicEmu::from_data(&header, 44100).is_err());
    assert!(game_music_emu_wrapper::GameMusicEmu::from_data(&header, 44100).is_err());
}

#[cfg(feature = "nsf")]
#[test]
fn typed_reload_playlist_and_metadata() {
    let mut emu = GameMusicEmu::new(Format::Nsf, 44100).unwrap();
    assert_eq!((emu.track_count(), emu.voice_count()), (0, 0));
    assert!(emu.voice_name(0).is_none());
    assert!(emu.start_track(0).is_err());
    emu.load_file(asset("nsf")).unwrap();
    assert_eq!(emu.voice_name(0).as_deref(), Some("Square 1"));
    emu.load_m3u(asset("m3u")).unwrap();
    assert_eq!(emu.track_count(), 1);
    let info = emu.track_info(0).unwrap();
    assert_eq!((info.song.as_str(), info.length), ("BGM C", 76000));
    emu.load_m3u_data(
        b"# @COMPOSER Test\r\n# Coauthor\r\ntest.nsf::NSF,1,Track,1:02.345,0:12-,3,2",
    )
    .unwrap();
    assert!(emu.load_m3u_data(b"# empty").is_err());
    let info = emu.track_info(0).unwrap();
    assert_eq!(
        (
            info.length,
            info.intro_length,
            info.loop_length,
            info.fade_length,
            info.repeat_count
        ),
        (62345, 12000, 50345, 3000, 2)
    );
    assert_eq!(info.composer, "Test, Coauthor");
    emu.start_track(0).unwrap();
    let mut original = vec![0; 8192];
    emu.play(&mut original).unwrap();
    emu.seek(0).unwrap();
    let mut restart = vec![0; original.len()];
    emu.play(&mut restart).unwrap();
    assert_eq!(original, restart);
    emu.clear_playlist();
    assert_eq!(emu.track_count(), 256);
    assert!(emu.load_data(b"wrong format").is_err());
    assert_eq!(emu.track_count(), 256);
    emu.load_data(std::fs::read(asset("nsf")).unwrap()).unwrap();
    assert!(emu.play(&mut restart).is_err());
}

#[cfg(feature = "nsf")]
#[test]
fn nsf_controls_change_audio_and_survive_reload() {
    use game_music_emu::Equalizer;
    fn render(emu: &mut GameMusicEmu) -> Vec<i16> {
        emu.start_track(0).unwrap();
        let mut buffer = vec![0; 44100];
        emu.play(&mut buffer).unwrap();
        buffer
    }
    let mut emu = GameMusicEmu::new(Format::Nsf, 44100).unwrap();
    let defaults = emu.equalizer();
    assert_eq!(defaults.bass, 80.0);
    emu.ignore_silence(true);
    emu.load_file(asset("nsf")).unwrap();
    let baseline = render(&mut emu);
    assert!(baseline.iter().any(|&s| s != 0));
    emu.mute_voices(-1);
    emu.load_file(asset("nsf")).unwrap();
    assert!(render(&mut emu).iter().all(|&s| s == 0));
    emu.mute_voice(0, false);
    let solo = render(&mut emu);
    assert!(solo.iter().any(|&s| s != 0));
    assert!(solo != baseline);
    emu.mute_voices(0);
    emu.set_equalizer(Equalizer::new(-24.0, 300.0)).unwrap();
    emu.load_file(asset("nsf")).unwrap();
    assert_eq!(emu.equalizer().bass, 300.0);
    assert!(render(&mut emu) != baseline);
    emu.set_equalizer(defaults).unwrap();
    emu.set_stereo_depth(0.8);
    let enhanced = render(&mut emu);
    assert!(enhanced.as_chunks::<2>().0.iter().any(|s| s[0] != s[1]));
    emu.set_stereo_depth(0.0);
    emu.set_tempo(1.5);
    emu.load_file(asset("nsf")).unwrap();
    assert!(render(&mut emu) != baseline);
}

#[cfg(feature = "spc")]
#[test]
fn spc_accuracy_filter_matches_reference() {
    let mut outputs = Vec::new();
    for accuracy in [false, true] {
        let mut rust = GameMusicEmu::new(Format::Spc, 32000).unwrap();
        rust.enable_accuracy(accuracy);
        rust.load_file(asset("spc")).unwrap();
        let cpp = game_music_emu_wrapper::GameMusicEmu::from_file(asset("spc"), 32000).unwrap();
        cpp.enable_accuracy(accuracy);
        rust.start_track(0).unwrap();
        cpp.start_track(0).unwrap();
        let mut samples = vec![0; 32000];
        let mut reference = vec![0; samples.len()];
        cpp.play(reference.len(), &mut reference).unwrap();
        rust.play(&mut samples).unwrap();
        assert!(samples == reference, "SPC accuracy={accuracy}");
        outputs.push(samples);
    }
    assert!(outputs[0] != outputs[1]);
}

#[test]
fn compare_sound_controls_with_cpp() {
    use game_music_emu::Equalizer;
    let mut differences = Vec::new();
    for &format in type_list() {
        if format == Format::Vgz {
            continue;
        } // The C++ wrapper has no gzip reader.
        let path = asset(&format.info().extension.to_ascii_lowercase());
        for control in ["mute", "tempo", "eq", "depth"] {
            let mut rust = GameMusicEmu::from_file(&path, 44100).unwrap();
            let cpp = game_music_emu_wrapper::GameMusicEmu::from_file(&path, 44100).unwrap();
            let track = i32::from(format == Format::Hes);
            rust.start_track(track).unwrap();
            cpp.start_track(track as usize).unwrap();
            match control {
                "mute" => {
                    rust.mute_voice(0, true);
                    cpp.mute_voice(0, true);
                }
                "tempo" => {
                    rust.set_tempo(1.5);
                    cpp.set_tempo(1.5);
                }
                "eq" => {
                    rust.set_equalizer(Equalizer::new(-20.0, 200.0)).unwrap();
                    let mut eq = cpp.equalizer();
                    eq.bass = 200.0;
                    eq.treble = -20.0;
                    cpp.set_equalizer(eq).unwrap();
                }
                _ => {
                    rust.set_stereo_depth(0.75);
                    cpp.set_stereo_depth(0.75);
                }
            }
            let mut samples = [0; 1024];
            let mut reference = [0; 1024];
            for block in 0..48 {
                rust.play(&mut samples).unwrap();
                cpp.play(reference.len(), &mut reference).unwrap();
                if let Some(i) = samples.iter().zip(reference).position(|(&r, c)| r != c) {
                    differences.push((format, control, block * 1024 + i, samples[i], reference[i]));
                    break;
                }
            }
        }
    }
    assert!(differences.is_empty(), "{differences:?}");
}

#[cfg(feature = "vgm")]
#[test]
fn ym2413_playback_vgz_restart_and_seek() {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut vgm = vec![0; 0x40];
    vgm[..4].copy_from_slice(b"Vgm ");
    vgm[8..12].copy_from_slice(&0x150u32.to_le_bytes());
    vgm[0x10..0x14].copy_from_slice(&3_579_545u32.to_le_bytes());
    vgm[0x34..0x38].copy_from_slice(&12u32.to_le_bytes());
    // OPLL flute, key on, wait one second, key off, wait another second.
    vgm.extend_from_slice(&[
        0x51, 0x30, 0x40, 0x51, 0x10, 0xa0, 0x51, 0x20, 0x19, 0x61, 0x44, 0xac, 0x51, 0x20, 0x09,
        0x61, 0x44, 0xac, 0x66,
    ]);
    let eof = (vgm.len() as u32 - 4).to_le_bytes();
    vgm[4..8].copy_from_slice(&eof);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&vgm).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut baseline = vec![0; 28800];
    for data in [&vgm, &compressed] {
        let mut player = GameMusicEmu::from_data(data, 48000).unwrap();
        assert_eq!(player.voice_name(6).as_deref(), Some("Rhythm"));
        player.start_track(0).unwrap();
        let mut audio = vec![0; baseline.len()];
        player.play(&mut audio).unwrap();
        assert!(audio.iter().any(|s| s.unsigned_abs() > 16));
        if data == &vgm {
            baseline.copy_from_slice(&audio);
        }
        assert_eq!(audio, baseline);

        player.seek(100).unwrap();
        let mut block = [0; 4096];
        player.play(&mut block).unwrap();
        assert_eq!(block, baseline[9600..9600 + block.len()]);
        player.start_track(0).unwrap();
        player.play(&mut block).unwrap();
        assert_eq!(block, baseline[..block.len()]);
    }
}
