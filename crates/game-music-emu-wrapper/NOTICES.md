# C++ wrapper notices

The Rust bindings and original project code use the [MIT license](LICENSE-MIT). The bundled Game_Music_Emu library is copyright © 2003–2009 Shay Green and is distributed under [LGPL-2.1-or-later](COPYING.LESSER). Source files carry their individual notices. The bundled emu2413 implementation is copyright © 2001–2019, 2020 Mitsutaka Okazaki and includes its [MIT permission notice](src/gme/ext/LICENSE).

For VGM and GYM, the build selects the first applicable YM2612 backend below.

| Build configuration | Compiled backend | Upstream terms |
| --- | --- | --- |
| `ym2612_emu_nuked` enabled (default) | Nuked OPN2, copyright © 2017 Alexey Khokholov | LGPL-2.1-or-later |
| `ym2612_emu_mame` enabled and Nuked disabled | MAME, copyright © 2003 Jarek Burczynski and Tatsuyuki Satoh, with Genesis Plus GX changes | GPL-2.0-or-later |
| Both flags disabled | GENS, copyright © 2002 Stéphane Dallongeville, adapted by Shay Green | LGPL-2.1-or-later |

The Cargo package includes all three backends as source, so its manifest lists MIT, LGPL-2.1-or-later and GPL-2.0-or-later together. Upstream specifies GPL-2.0-or-later for the library when built with MAME. Distribution of an executable linked to that build must follow the applicable GPL terms, including corresponding source for the combined work. The default build selects Nuked and uses the LGPL linking provisions. A build with both VGM and GYM disabled compiles no YM2612 backend. [Upstream backend licensing](https://github.com/libgme/game-music-emu/blob/8238afd9503a59ffcec0e2685584d5ce819ad7c6/readme.txt#L38-L41) · [GPL version 2](COPYING)
