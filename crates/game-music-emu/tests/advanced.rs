use game_music_emu::{Format, GameMusicEmu, type_list};
use std::ffi::{c_char, c_int, c_long, c_void};
use std::path::Path;

// The vendored reference exposes these upstream functions through its C ABI.
unsafe extern "C" {
    fn gme_identify_extension(name: *const c_char) -> *const c_void;
    fn gme_new_emu_multi_channel(kind: *const c_void, rate: c_int) -> *mut c_void;
    fn gme_multi_channel(player: *mut c_void) -> c_int;
    fn gme_load_data(player: *mut c_void, data: *const c_void, size: c_long) -> *const c_char;
    fn gme_start_track(player: *mut c_void, track: c_int) -> *const c_char;
    fn gme_play(player: *mut c_void, size: c_int, out: *mut i16) -> *const c_char;
    fn gme_delete(player: *mut c_void);
}
struct Reference(*mut c_void);
impl Drop for Reference {
    fn drop(&mut self) {
        unsafe { gme_delete(self.0) }
    }
}

fn asset(fmt: Format) -> Vec<u8> {
    let extension = fmt.info().extension.to_lowercase();
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/test.{extension}")),
    )
    .unwrap()
}

#[test]
fn discrete_voice_pairs_match_upstream() {
    let _compiled = game_music_emu_wrapper::type_list();
    for &fmt in type_list() {
        if fmt == Format::Vgz {
            continue;
        }
        let ext = std::ffi::CString::new(fmt.info().extension).unwrap();
        let data = asset(fmt);
        let track = i32::from(fmt == Format::Hes);
        let reference = Reference(unsafe {
            gme_new_emu_multi_channel(gme_identify_extension(ext.as_ptr()), 44100)
        });
        assert!(!reference.0.is_null());
        let mut player = GameMusicEmu::new_multi_channel(fmt, 44100).unwrap();
        assert_eq!(
            player.multi_channel(),
            unsafe { gme_multi_channel(reference.0) != 0 },
            "{fmt:?}"
        );
        if !player.multi_channel() {
            continue;
        }
        player.load_data(&data).unwrap();
        player.start_track(track).unwrap();
        assert!(
            unsafe { gme_load_data(reference.0, data.as_ptr().cast(), data.len() as c_long) }
                .is_null()
        );
        assert!(unsafe { gme_start_track(reference.0, track) }.is_null());
        let mut audible = false;
        for i in 0..12 {
            let mut expected = vec![0; 4096];
            let mut got = vec![0; 4096];
            player.play(&mut got).unwrap();
            assert!(unsafe { gme_play(reference.0, 4096, expected.as_mut_ptr()) }.is_null());
            assert_eq!(
                got.iter().zip(&expected).position(|(a, b)| a != b),
                None,
                "{fmt:?} block {i}"
            );
            assert_eq!(player.tell_samples(), (i + 1) * 4096);
            audible |= got.iter().any(|&x| x != 0);
        }
        assert!(audible, "{fmt:?}");
        assert!(player.play(&mut [0; 2]).is_err());
    }
}

#[test]
fn reader_metadata_and_sample_position() {
    for &fmt in type_list() {
        let data = asset(fmt);
        let mut info = GameMusicEmu::new_info_only(fmt).unwrap();
        info.load_reader(std::io::Cursor::new(&data)).unwrap();
        let mut full = GameMusicEmu::new(fmt, 44100).unwrap();
        full.load_data(&data).unwrap();
        let track = i32::from(fmt == Format::Hes);
        assert_eq!(info.track_count(), full.track_count(), "{fmt:?}");
        assert_eq!(
            info.track_info(track).unwrap().game,
            full.track_info(track).unwrap().game
        );
        assert!(info.start_track(track).is_err());
        full.start_track(track).unwrap();
        full.seek_samples(2468).unwrap();
        assert_eq!(full.tell_samples(), 2468, "{fmt:?}");
        full.set_tempo(2.0);
        full.play(&mut [0; 8820]).unwrap();
        assert_eq!(full.tell_scaled(), 227, "{fmt:?}");
        full.seek_scaled(100).unwrap();
        assert_eq!(full.tell_scaled(), 100, "{fmt:?}");
    }
}

#[cfg(feature = "spc")]
#[test]
fn collection_switches_snapshots_and_preserves_playlist() {
    let first = asset(Format::Spc);
    let mut second = first.clone();
    second[0x2e..0x4e].fill(0);
    second[0x2e..0x32].copy_from_slice(b"Next");
    let mut emu = GameMusicEmu::new(Format::Spc, 32000).unwrap();
    emu.load_tracks(&[&first, &second]).unwrap();
    assert_eq!(emu.track_count(), 2);
    assert_eq!(emu.track_info(1).unwrap().song, "Next");
    emu.load_m3u_data(b"x.spc,$01,Second\nx.spc,$00,First\n")
        .unwrap();
    for track in [0, 1, 0] {
        emu.start_track(track).unwrap();
        emu.play(&mut [0; 200]).unwrap();
        emu.seek_samples(10).unwrap();
        assert_eq!(
            emu.track_info(track).unwrap().song,
            if track == 0 { "Second" } else { "First" }
        );
    }
    assert!(emu.load_tracks(&[&first[..], b"bad"]).is_err());
    emu.clear_playlist();
    assert_eq!(emu.track_info(1).unwrap().song, "Next");
}

#[cfg(feature = "kss")]
#[test]
fn decoder_warnings_are_consumed_once() {
    let mut data = asset(Format::Kss);
    data[15] |= 1;
    let mut emu = GameMusicEmu::from_data(&data, 44100).unwrap();
    assert_eq!(emu.warning().as_deref(), Some("FM sound not supported"));
    assert_eq!(emu.warning(), None);
}

#[cfg(feature = "spc")]
#[test]
fn spc_echo_and_metadata_limit_survive_load() {
    let mut data = asset(Format::Spc);
    data[169..172].copy_from_slice(b"001");
    let mut player = GameMusicEmu::new(Format::Spc, 32000).unwrap();
    let render = |player: &mut GameMusicEmu, disable: bool, limit: bool| {
        player.disable_echo(disable);
        player.set_autoload_playback_limit(limit);
        player.load_data(&data).unwrap();
        assert_eq!(player.autoload_playback_limit(), limit);
        assert_eq!(player.track_info(0).unwrap().length, 1000);
        player.start_track(0).unwrap();
        let mut beginning = [0; 4096];
        player.play(&mut beginning).unwrap();
        for _ in 0..20 {
            player.play(&mut [0; 4096]).unwrap();
        }
        assert_eq!(player.track_ended(), limit);
        beginning
    };
    assert_ne!(
        render(&mut player, true, true),
        render(&mut player, false, false)
    );
}

#[cfg(any(feature = "ym2612_emu_gens", feature = "ym2612_emu_mame"))]
#[test]
fn fm_backend_selection_reaches_file_playback() {
    use game_music_emu::Ym2612Backend;
    for fmt in [Format::Gym, Format::Vgz] {
        if !type_list().contains(&fmt) {
            continue;
        }
        let data = asset(fmt);
        let mut results = Vec::new();
        for backend in [
            Ym2612Backend::Nuked,
            #[cfg(feature = "ym2612_emu_gens")]
            Ym2612Backend::Gens,
            #[cfg(feature = "ym2612_emu_mame")]
            Ym2612Backend::Mame,
        ] {
            let mut player = GameMusicEmu::new_with_ym2612(fmt, 44100, backend).unwrap();
            player.load_data(&data).unwrap();
            player.start_track(0).unwrap();
            let mut pcm = vec![0; 16384];
            player.play(&mut pcm).unwrap();
            assert!(pcm.iter().any(|&x| x != 0));
            assert!(!results.contains(&pcm), "{fmt:?} {backend:?}");
            player.load_data(&data).unwrap();
            player.start_track(0).unwrap();
            let mut again = vec![0; pcm.len()];
            player.play(&mut again).unwrap();
            assert_eq!(again, pcm);
            results.push(pcm);
        }
    }
}
