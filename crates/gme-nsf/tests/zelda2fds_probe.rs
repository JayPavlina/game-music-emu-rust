// Rejection parity: the C++ rejects NSFs whose load/init address is below
// $8000 ("Corrupt file"); the Rust side must reject them too.
use gme_common::gme_file::GmeFile;
use gme_nsf::nsf_emu::NsfEmu;

#[test]
fn invalid_load_addr_rejected_like_cpp() {
    let path = "/Volumes/Speedy/Code/Rust/personal/cross-engine/castlevania-crossover/assets/audio/sequenced/nsf/Zelda2Fds.nsf";
    let Ok(data) = std::fs::read(path) else {
        eprintln!("corpus file not present; skipping");
        return;
    };
    let cpp_err = game_music_emu_wrapper::GameMusicEmu::from_data(&data, 44100).is_err();
    let mut emu = NsfEmu::new();
    let rust_err = emu.load_mem_impl(&data).is_err();
    assert_eq!(cpp_err, rust_err, "rejection parity (cpp_err={cpp_err})");
}
