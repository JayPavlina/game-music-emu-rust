
fn main() {}
// // Note: MSVC is not supported
//
// fn main() {
//     let mut defines = Vec::new();
//
//     let ay = cfg!(feature = "ay");
//     if ay { defines.push("USE_GME_AY") }
//
//     let gbs = cfg!(feature = "gbs");
//     if gbs { defines.push("USE_GME_GBS") }
//
//     let gym = cfg!(feature = "gym");
//     if gym { defines.push("USE_GME_GYM") }
//
//     let hes = cfg!(feature = "hes");
//     if hes { defines.push("USE_GME_HES") }
//
//     let kss = cfg!(feature = "kss");
//     if kss { defines.push("USE_GME_KSS") }
//
//     let nsf = cfg!(feature = "nsf");
//     if nsf { defines.push("USE_GME_NSF") }
//
//     let nsfe = cfg!(feature = "nsfe");
//     if nsfe { defines.push("USE_GME_NSFE") }
//
//     let sap = cfg!(feature = "sap");
//     if sap { defines.push("USE_GME_SAP") }
//
//     let spc = cfg!(feature = "spc");
//     if spc { defines.push("USE_GME_SPC") }
//
//     let vgm = cfg!(feature = "vgm");
//     if vgm { defines.push("USE_GME_VGM"); }
//
//     let mut files = vec![
//         "Blip_Buffer.cpp",
//         "Classic_Emu.cpp",
//         "Data_Reader.cpp",
//         "Dual_Resampler.cpp",
//         "Effects_Buffer.cpp",
//         "Fir_Resampler.cpp",
//         "gme.cpp",
//         "Gme_File.cpp",
//         "M3u_Playlist.cpp",
//         "Multi_Buffer.cpp",
//         "Music_Emu.cpp",
//     ];
//
//     if ay || kss {
//         files.push("Ay_Apu.cpp")
//     }
//
//     if vgm || gym {
//         if cfg!(feature = "ym2612_emu_nuked") {
//             defines.push("VGM_YM2612_NUKED");
//             files.push("Ym2612_Nuked.cpp");
//         } else if cfg!(feature = "ym2612_emu_mame") {
//             defines.push("VGM_YM2612_MAME");
//             files.push("Ym2612_MAME.cpp");
//         } else {
//             defines.push("VGM_YM2612_GENS");
//             files.push("Ym2612_GENS.cpp");
//         }
//     }
//
//     if vgm || gym || kss {
//         files.push("Sms_Apu.cpp");
//     }
//
//     if ay {
//         files.extend_from_slice(&[
//             "Ay_Cpu.cpp",
//             "Ay_Emu.cpp",
//         ]);
//     }
//
//     if gbs {
//         files.extend_from_slice(&[
//             "Gb_Apu.cpp",
//             "Gb_Cpu.cpp",
//             "Gb_Oscs.cpp",
//             "Gbs_Emu.cpp"
//         ]);
//     }
//
//     if gym {
//         files.push("Gym_Emu.cpp")
//     }
//
//     if hes {
//         files.extend_from_slice(&[
//             "Hes_Apu.cpp",
//             "Hes_Cpu.cpp",
//             "Hes_Emu.cpp"
//         ]);
//     }
//
//     if kss {
//         files.extend_from_slice(&[
//             "Kss_Cpu.cpp",
//             "Kss_Emu.cpp",
//             "Kss_Scc_Apu.cpp",
//         ]);
//     }
//
//
//     if nsf || nsfe {
//         files.extend_from_slice(&[
//             "Nes_Apu.cpp",
//             "Nes_Cpu.cpp",
//             "Nes_Fme7_Apu.cpp",
//             "Nes_Namco_Apu.cpp",
//             "Nes_Oscs.cpp",
//             "Nes_Vrc6_Apu.cpp",
//             "Nsf_Emu.cpp"
//         ]);
//     }
//
//     if nsfe {
//         files.push("Nsfe_Emu.cpp");
//     }
//
//     if sap {
//         files.extend_from_slice(&[
//             "Sap_Apu.cpp",
//             "Sap_Cpu.cpp",
//             "Sap_Emu.cpp"
//         ]);
//     }
//
//     if spc {
//         files.extend_from_slice(&[
//             "Snes_Spc.cpp",
//             "Spc_Cpu.cpp",
//             "Spc_Dsp.cpp",
//             "Spc_Emu.cpp",
//             "Spc_Filter.cpp"
//         ]);
//     }
//
//     if vgm {
//         files.extend_from_slice(&[
//             "Vgm_Emu.cpp",
//             "Vgm_Emu_Impl.cpp",
//             "Ym2413_Emu.cpp",
//         ]);
//     }
//
//
//     let mut build = cc::Build::new();
//     build.cpp(true);
//
//     build.flag("-std=c++11");
//
//     for file in files {
//         build.file(format!("src/gme/{}", file));
//     }
//
//     for flag in defines {
//         build.flag(&format!("-D {}", flag));
//     }
//
//     build.compile("gme");
// }