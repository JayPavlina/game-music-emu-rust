//! Sega Genesis GYM format support, ported 1:1 from the vendored C++
//! Game_Music_Emu `Gym_Emu` (bit-exact against the C++ build).

pub mod gym_emu;
pub mod header;

pub use gym_emu::GymEmu;
