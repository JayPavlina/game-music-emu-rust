fn main() {
    let mut flags = Vec::new();

    let ay = cfg!(feature = "ay");
    if ay { flags.push("USE_GME_AY") }

    let gbs = cfg!(feature = "gbs");
    if gbs { flags.push("USE_GME_GBS") }

    let gym = cfg!(feature = "gym");
    if gym { flags.push("USE_GME_GYM") }

    let hes = cfg!(feature = "hes");
    if hes { flags.push("USE_GME_HES") }

    let kss = cfg!(feature = "kss");
    if kss { flags.push("USE_GME_KSS") }

    let nsf = cfg!(feature = "nsf");
    if nsf { flags.push("USE_GME_NSF") }

    let nsfe = cfg!(feature = "nsfe");
    if nsfe { flags.push("USE_GME_NSFE") }

    let sap = cfg!(feature = "sap");
    if sap { flags.push("USE_GME_SAP") }

    let spc = cfg!(feature = "spc");
    if spc { flags.push("USE_GME_SPC") }

    let vgm = cfg!(feature = "vgm");
    if vgm { flags.push("USE_GME_VGM"); }

    let mut files = Vec::new();

    files.extend_from_slice(&vec![
        "Blip_Buffer.cpp",
        "Classic_Emu.cpp",
        "Data_Reader.cpp",
        "Dual_Resampler.cpp",
        "Effects_Buffer.cpp",
        "Fir_Resampler.cpp",
        "gme.cpp",
        "Gme_File.cpp",
        "M3u_Playlist.cpp",
        "Multi_Buffer.cpp",
        "Music_Emu.cpp"
    ]);

    if ay || kss {
        files.push("Ay_Apu.cpp")
    }

    if vgm || gym {
        files.push("Ym2612_Emu.cpp");
    }

    if vgm || gym || kss {
        files.push("Sms_Apu.cpp");
    }

    if ay {
        files.extend_from_slice(&vec![
            "Ay_Cpu.cpp",
            "Ay_Emu.cpp"
        ]);
    }

    if gbs {
        files.extend_from_slice(&vec![
            "Gb_Apu.cpp",
            "Gb_Cpu.cpp",
            "Gb_Oscs.cpp",
            "Gbs_Emu.cpp"
        ]);
    }

    if gym {
        files.push("Gym_Emu.cpp")
    }

    if hes {
        files.extend_from_slice(&vec![
            "Hes_Apu.cpp",
            "Hes_Cpu.cpp",
            "Hes_Emu.cpp"
        ]);
    }

    if kss {
        files.extend_from_slice(&vec![
            "Kss_Cpu.cpp",
            "Kss_Emu.cpp",
            "Kss_Scc_Apu.cpp",
        ]);
    }


    if nsf || nsfe {
        files.extend_from_slice(&vec![
            "Nes_Apu.cpp",
            "Nes_Cpu.cpp",
            "Nes_Fme7_Apu.cpp",
            "Nes_Namco_Apu.cpp",
            "Nes_Oscs.cpp",
            "Nes_Vrc6_Apu.cpp",
            "Nsf_Emu.cpp"
        ]);
    }

    if nsfe {
        files.push("Nsfe_Emu.cpp");
    }

    if sap {
        files.extend_from_slice(&vec![
            "Sap_Apu.cpp",
            "Sap_Cpu.cpp",
            "Sap_Emu.cpp"
        ]);
    }

    if spc {
        files.extend_from_slice(&vec![
            "Snes_Spc.cpp",
            "Spc_Cpu.cpp",
            "Spc_Dsp.cpp",
            "Spc_Emu.cpp",
            "Spc_Filter.cpp"
        ]);
    }

    if vgm {
        files.extend_from_slice(&vec![
            "Vgm_Emu.cpp",
            "Vgm_Emu_Impl.cpp",
            "Ym2413_Emu.cpp",
        ]);
    }


    let mut build = cc::Build::new();
    build.cpp(true);

    for file in files {
        build.file(format!("src/gme/{}", file));
    }

    for flag in flags {
        build.flag(&format!("-D {}", flag));
    }

    build.compile("gme");
}