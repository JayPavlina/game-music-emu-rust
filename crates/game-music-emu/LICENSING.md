# Licenses for Game Music Emu

The Rust playback API, examples, and original project code are available under the [MIT license](LICENSE-MIT). The Rust audio engine and decoders incorporate translations of Game_Music_Emu, copyright © 2003–2009 Shay Green and maintained by Michael Pyne, under the [GNU Lesser General Public License, version 2.1 or later](COPYING.LESSER). The upstream library and Rust adaptations are provided without warranty. The source headers identify the corresponding upstream file, copyright holder, license and modification dates.

## Source and feature coverage

`game-music-emu` declares MIT for its facade source. Its normal dependencies always include `gme-common` and `gme-core`, even with default features disabled. Every playback format uses LGPL-covered implementations. Each engine and decoder crate declares `MIT AND LGPL-2.1-or-later` for its combination of original project files and translated sources.

| Cargo package or feature | Upstream components in the Rust implementation |
| --- | --- |
| `gme-core` | Blip_Buffer, Multi_Buffer, Effects_Buffer, Fir_Resampler, Data_Reader and M3u_Playlist. The shared emu2413 OPLL core has its own MIT notice. |
| `gme-common` | Classic_Emu, Music_Emu, Gme_File, format descriptors and track information |
| `ay` | Ay_Emu, Ay_Cpu and Ay_Apu |
| `gbs` | Gbs_Emu, Gb_Cpu, Gb_Apu, Gb_Oscs and Classic_Emu ROM handling |
| `gym` | Gym_Emu and the Genesis synthesis and resampling code from `gme-vgm` |
| `hes` | Hes_Emu, Hes_Cpu, Hes_Apu, Hes_Apu_Adpcm and Classic_Emu ROM handling |
| `kss` | Kss_Emu, Kss_Cpu, Kss_Scc_Apu, Ay_Apu and the shared SMS PSG |
| `nsf`, `nsfe` | Nsf_Emu, Nsfe_Emu, Nes_Cpu, Nes_Apu, Nes_Oscs, expansion-chip adapters and Classic_Emu ROM handling |
| `sap` | Sap_Emu, Sap_Cpu and Sap_Apu |
| `spc` | Spc_Emu, Snes_Spc, Spc_Cpu, Spc_Dsp and Spc_Filter |
| `vgm` | Vgm_Emu, Vgm_Emu_Impl, Dual_Resampler, Sms_Apu and Nuked OPN2. VGZ uses the same decoder. |

The Nuked OPN2 port in `gme-vgm/src/ym2612_nuked.rs` derives from Alexey Khokholov’s 2017 YM3438 core under LGPL-2.1-or-later. VGM and GYM use it for YM2612 sound. The OPLL core in `gme-core/src/emu2413.rs` derives from Mitsutaka Okazaki’s MIT-licensed emu2413 1.5.9. The original license credits 2001–2019 and the C source credits 2020. Its complete permission notice ships as `LICENSE-emu2413` in `gme-core`. NSF uses this core for VRC7, and VGM uses it for YM2413. [Game_Music_Emu upstream notices](https://github.com/libgme/game-music-emu/blob/8238afd9503a59ffcec0e2685584d5ce819ad7c6/readme.txt#L235-L240) · [emu2413](https://github.com/digital-sound-antiques/emu2413)

## Shipping a game or player

Commercial distribution is permitted. When distributing a game linked with the LGPL library, give prominent notice that it uses Game Music Emu under the LGPL, supply the license and applicable copyright notices, and use terms that permit modification for the customer’s own use and reverse engineering for debugging those modifications. Retain the MIT notices for the MIT-covered portions.

For a statically linked game, one route under LGPL 2.1 §6(a) is to provide the complete corresponding source of the LGPL library, including your changes, along with relinkable application object files or application source and the build inputs needed to produce the executable with a modified library. Test that the supplied materials actually relink with a modified library, especially if you enable link-time optimization. Section 6 also provides other routes, including a suitable replaceable shared library under §6(b). [LGPL 2.1, sections 2, 4 and 6](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html#section6)

The optional C++ wrapper has a separate MAME YM2612 backend under GPL-2.0-or-later. The pure Rust playback crates use Nuked. Test music and third-party Cargo dependencies carry their own terms.
