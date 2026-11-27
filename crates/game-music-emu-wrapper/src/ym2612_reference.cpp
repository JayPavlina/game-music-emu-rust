// Test-only access to both upstream cores. Build each in its own translation unit.
// The renamed classes keep the reference instances independent of the player backend.
#if defined(REFERENCE_GENS)
#define VGM_YM2612_GENS
#define Ym2612_GENS_Impl GensReferenceImpl
#define Ym2612_GENS_Emu GensReference
#include "gme/Ym2612_GENS.cpp"
using Reference = GensReference;
#define API(name) gens_reference_##name
#elif defined(REFERENCE_MAME)
#define VGM_YM2612_MAME
#define Ym2612_MameImpl MameReferenceImpl
#define Ym2612_MAME_Emu MameReference
#include "gme/Ym2612_MAME.cpp"
using Reference = MameReference;
#define API(name) mame_reference_##name
#endif

extern "C" {
void* API(create)(double rate, double clock) {
    auto* chip = new Reference();
    chip->set_rate(rate, clock);
    chip->reset();
    return chip;
}
void API(destroy)(Reference* chip) { delete chip; }
void API(write)(Reference* chip, int bank, int addr, int data) {
    if (bank) chip->write1(addr, data);
    else chip->write0(addr, data);
}
void API(render)(Reference* chip, int frames, short* out) { chip->run(frames, out); }
void API(mute)(Reference* chip, int mask) { chip->mute_voices(mask); }
}
