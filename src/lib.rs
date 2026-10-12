//! JAPS1ME - PS1 RetroArch Save Converter
//!
//! A library for reading DexDrive (`.gme`) and raw (`.srm`/`.mcr`) PlayStation 1
//! memory-card images, with support for converting `.gme` files to RetroArch
//! `.srm` format.

pub mod commands;
pub mod gme;
pub mod ps1;

pub use commands::list::{format_save_table, format_size};
pub use gme::{parse_gme, GmeCard, GME_FILE_LEN, GME_HEADER_LEN, RAW_CARD_LEN};
pub use ps1::card::{parse_raw, CardSummary, SlotInfo, SlotType};
pub use ps1::title::decode_title;
