extern crate cc;

use std::env;

fn main() {
    env::set_var("CXXFLAGS_CC_", "/Users/jay/Code/clang+llvm-8.0.0-x86_64-apple-darwin/bin/clang++");
//    env::set_var("RUST_BACKTRACE", "1");
//    env::set_var("CXXSTDLIB", "/Users/jay/Code/clang+llvm-8.0.0-x86_64-apple-darwin/lib/c++");
//    env::set_var("CXX", "/Users/jay/Code/clang+llvm-8.0.0-x86_64-apple-darwin/bin/clang++");
    cc::Build::new()
        .cpp(true)
//        .compiler("/Users/jay/Code/clang+llvm-8.0.0-x86_64-apple-darwin/bin/clang++")
//        .flag("-cxx-isystem /Users/jay/Code/clang+llvm-8.0.0-x86_64-apple-darwin/lib/")
//        .cpp_set_stdlib("c++")
//        .flag("-L/Users/jay/Code/clang+llvm-8.0.0-x86_64-apple-darwin/lib/")
//        .flag("-stdlib=c++")
//        .file("src/hello.cpp")

        // nsf
        .file("src/gme/Blip_Buffer.cpp")
        .file("src/gme/Classic_Emu.cpp")
        .file("src/gme/Data_Reader.cpp")
        .file("src/gme/Dual_Resampler.cpp")
        .file("src/gme/Effects_Buffer.cpp")
        .file("src/gme/Fir_Resampler.cpp")
        .file("src/gme/gme.cpp")
        .file("src/gme/Gme_File.cpp")
        .file("src/gme/M3u_Playlist.cpp")
        .file("src/gme/Multi_Buffer.cpp")
        .file("src/gme/Music_Emu.cpp")
        .file("src/gme/Nes_Apu.cpp")
        .file("src/gme/Nes_Cpu.cpp")
        .file("src/gme/Nes_Fme7_Apu.cpp")
        .file("src/gme/Nes_Namco_Apu.cpp")
        .file("src/gme/Nes_Oscs.cpp")
        .file("src/gme/Nes_Vrc6_Apu.cpp")
        .file("src/gme/Nsf_Emu.cpp")

        // nsfe
        .file("src/gme/Nsfe_Emu.cpp")

        // kss
        .file("src/gme/Ay_Apu.cpp")
        .file("src/gme/Sms_Apu.cpp")
        .file("src/gme/Kss_Cpu.cpp")
        .file("src/gme/Kss_Emu.cpp")
        .file("src/gme/Kss_Scc_Apu.cpp")

//        .debug(true)
        .compile("gme");
}