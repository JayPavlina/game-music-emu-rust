#![cfg(any(feature = "ym2612_emu_gens", feature = "ym2612_emu_mame"))]

use gme_common::music_emu::Ym2612Backend;
use gme_vgm::ym2612::Ym2612;
use std::ffi::c_void;
type Chip = *mut c_void;
struct Reference {
    create: unsafe extern "C" fn(f64, f64) -> Chip,
    destroy: unsafe extern "C" fn(Chip),
    write: unsafe extern "C" fn(Chip, i32, i32, i32),
    render: unsafe extern "C" fn(Chip, i32, *mut i16),
    mute: unsafe extern "C" fn(Chip, i32),
}

macro_rules! reference {
    ($create:ident,$destroy:ident,$write:ident,$render:ident,$mute:ident) => {{
        unsafe extern "C" {
            fn $create(rate: f64, clock: f64) -> Chip;
            fn $destroy(chip: Chip);
            fn $write(chip: Chip, bank: i32, address: i32, data: i32);
            fn $render(chip: Chip, frames: i32, samples: *mut i16);
            fn $mute(chip: Chip, mask: i32);
        }
        Reference {
            create: $create,
            destroy: $destroy,
            write: $write,
            render: $render,
            mute: $mute,
        }
    }};
}

#[test]
fn chip_backends_match_upstream_pcm() {
    let _linked = game_music_emu_wrapper::type_list();
    for (backend, api) in [
        #[cfg(feature = "ym2612_emu_gens")]
        (
            Ym2612Backend::Gens,
            reference!(
                gens_reference_create,
                gens_reference_destroy,
                gens_reference_write,
                gens_reference_render,
                gens_reference_mute
            ),
        ),
        #[cfg(feature = "ym2612_emu_mame")]
        (
            Ym2612Backend::Mame,
            reference!(
                mame_reference_create,
                mame_reference_destroy,
                mame_reference_write,
                mame_reference_render,
                mame_reference_mute
            ),
        ),
    ] {
        for rate in [32_000.0, 44_100.0, 53_267.0] {
            let clock = 7_670_454.0;
            let mut rust = Ym2612::select(backend).unwrap();
            rust.set_rate(rate, clock).unwrap();
            rust.reset();
            let cpp = unsafe { (api.create)(rate, clock) };
            let mut heard = false;
            for algorithm in 0..8 {
                for bank in 0..2 {
                    for ch in 0..3 {
                        let mut write = |addr, data| {
                            if bank == 0 {
                                rust.write0(addr, data)
                            } else {
                                rust.write1(addr, data)
                            }
                            unsafe { (api.write)(cpp, bank, addr, data) }
                        };
                        for op in [0, 4, 8, 12] {
                            for (addr, val) in [
                                (0x30, 1 + op / 4),
                                (0x40, 4 + op),
                                (0x50, 0x1f),
                                (0x60, 0x89),
                                (0x70, 0x07),
                                (0x80, 0x27),
                                (0x90, 0x08),
                            ] {
                                write(addr + op + ch, val);
                            }
                        }
                        write(0xb0 + ch, algorithm | 0x28);
                        write(0xb4 + ch, if ch == 1 { 0x7f } else { 0xcf });
                        write(0xa4 + ch, 0x22);
                        write(0xa0 + ch, 0x34 + ch);
                        let key = ch + 4 * bank;
                        rust.write0(0x28, 0xf0 | key);
                        unsafe { (api.write)(cpp, 0, 0x28, 0xf0 | key) };
                    }
                }
                rust.write0(0x22, 0xf);
                unsafe { (api.write)(cpp, 0, 0x22, 0xf) };
                let mask = if algorithm == 4 { 0x11 } else { 0 };
                rust.mute_voices(mask);
                unsafe { (api.mute)(cpp, mask) };
                let mut got = [17; 2048];
                let mut want = got;
                rust.run(1024, &mut got);
                unsafe { (api.render)(cpp, 1024, want.as_mut_ptr()) };
                assert_eq!(
                    got.iter().zip(&want).position(|(a, b)| a != b),
                    None,
                    "{backend:?} {rate} algorithm {algorithm}"
                );
                heard |= got.iter().any(|&v| v != 17);
            }
            unsafe { (api.destroy)(cpp) };
            assert!(heard);
        }
    }
}
