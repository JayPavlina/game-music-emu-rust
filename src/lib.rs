#![allow(dead_code, unused_imports, unused_variables, unused_must_use, unused_mut, non_camel_case_types)]
#![feature(maybe_uninit)]

pub mod native;
mod experimental;
mod structures;

pub use self::{
    experimental::GameMusicEmu,
    structures::{EmuType, EmuHandle}
};