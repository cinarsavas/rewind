//! Platform-independent logic for Rewind. Everything here is unit-tested on any OS;
//! the Windows-only capture engine and UI live in the `rewind` app crate.

pub mod config;
pub mod estimate;
pub mod hotkey;
pub mod library;
pub mod packs;
pub mod perf;
pub mod theme;
pub mod toast;
pub mod trim;
pub mod voice;
