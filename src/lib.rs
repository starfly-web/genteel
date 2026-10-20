#![deny(warnings)]
//! Genteel - An instrumentable Sega Mega Drive/Genesis emulator
//!
//! This library provides the core emulation components for the Genesis.

pub mod api;
pub mod apu;
pub mod audio;
pub mod cpu;
pub mod debugger;
pub mod frontend;
pub mod input;
pub mod io;
pub mod memory;
pub mod vdp;
pub mod wav_writer;
pub mod z80;

#[cfg(test)]
pub mod test_utils;

pub use audio::{create_audio_buffer, AudioBuffer, SharedAudioBuffer};
pub use cpu::Cpu;
pub use input::{InputManager, InputScript};
pub use memory::Memory;
pub use z80::Z80;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
