//! DexDrive `.gme` memory-card image parsing.
//!
//! A `.gme` file is a 3904-byte header followed by a 131072-byte raw card
//! image, for a total of 134976 bytes. The canonical header starts with the
//! `123-456-STD` magic and carries per-slot status bytes and 256-byte
//! comments. Some files in the wild (including the PES 2 fixture in
//! `save-games/`) have a zeroed or otherwise non-canonical header; those are
//! accepted as long as the embedded raw image starts with `MC`, matching
//! MemcardRex's corrupted-header fallback.

use anyhow::{bail, Result};

/// Length of the DexDrive header prepended to the raw card image.
pub const GME_HEADER_LEN: usize = 3904;
/// Length of the raw card image (also the `.srm` length).
pub const RAW_CARD_LEN: usize = 131072;
/// Total length of a `.gme` file.
pub const GME_FILE_LEN: usize = GME_HEADER_LEN + RAW_CARD_LEN;
/// Canonical DexDrive header magic (`123-456-STD`).
pub const GME_MAGIC: &[u8; 11] = b"123-456-STD";
/// Offset of the per-slot comments within the header.
const COMMENT_OFFSET: usize = 64;
/// Length of each per-slot comment in the header.
const COMMENT_LEN: usize = 256;

/// A parsed DexDrive image.
pub struct GmeCard {
    /// True when the header carries the canonical `123-456-STD` magic.
    pub header_present: bool,
    /// Per-slot comments decoded from the header (empty for `.srm`/zeroed headers).
    pub comments: [String; 15],
    /// The raw 131072-byte card image.
    pub raw: Vec<u8>,
}

/// Parse `.gme` bytes, returning the card plus non-fatal warnings.
///
/// Warnings cover the missing-header fallback; hard errors cover wrong file
/// length and a missing `MC` signature at the raw offset.
pub fn parse_gme(bytes: &[u8]) -> Result<(GmeCard, Vec<String>)> {
    let mut warnings = Vec::new();
    if bytes.len() != GME_FILE_LEN {
        bail!(
            "not a DexDrive .gme file: expected {} bytes, found {}",
            GME_FILE_LEN,
            bytes.len()
        );
    }
    let header = &bytes[..GME_HEADER_LEN];
    let raw = &bytes[GME_HEADER_LEN..];
    if raw[0] != b'M' || raw[1] != b'C' {
        bail!("not a DexDrive .gme file: missing MC signature at raw offset");
    }
    let header_present = header[..GME_MAGIC.len()] == GME_MAGIC[..];
    if !header_present {
        warnings.push(
            "missing DexDrive header magic (headerless or zeroed header); treating bytes 3904.. as the raw card".to_owned(),
        );
    }
    let mut comments: [String; 15] = Default::default();
    for (slot, comment) in comments.iter_mut().enumerate() {
        let start = COMMENT_OFFSET + slot * COMMENT_LEN;
        *comment = decode_comment(&header[start..start + COMMENT_LEN]);
    }
    Ok((
        GmeCard {
            header_present,
            comments,
            raw: raw.to_vec(),
        },
        warnings,
    ))
}

/// Decode one 256-byte header comment using Windows-1252 (MemcardRex uses the
/// system codepage, which is Windows-1252 on Western systems).
fn decode_comment(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes[..end]);
    text.trim_end_matches(['\0', ' ', '\t', '\r', '\n'])
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_raw() -> Vec<u8> {
        let mut raw = vec![0u8; RAW_CARD_LEN];
        raw[0] = b'M';
        raw[1] = b'C';
        // One formatted directory frame with a valid XOR checksum.
        let off = 128;
        raw[off] = 0xA0;
        raw[off + 8] = 0xFF;
        raw[off + 9] = 0xFF;
        let mut xor = 0u8;
        for b in &raw[off..off + 127] {
            xor ^= b;
        }
        raw[off + 127] = xor;
        raw
    }

    fn canonical_header() -> Vec<u8> {
        let mut header = vec![0u8; GME_HEADER_LEN];
        header[..GME_MAGIC.len()].copy_from_slice(GME_MAGIC);
        header
    }

    #[test]
    fn parses_canonical_header_without_warnings() {
        let mut file = canonical_header();
        file.extend_from_slice(&sample_raw());
        let (card, warnings) = parse_gme(&file).expect("canonical file parses");
        assert!(card.header_present);
        assert!(warnings.is_empty());
        assert_eq!(card.raw.len(), RAW_CARD_LEN);
        assert_eq!(&card.raw[0..2], b"MC");
    }

    #[test]
    fn accepts_headerless_file_with_warning() {
        let mut file = vec![0u8; GME_HEADER_LEN];
        file.extend_from_slice(&sample_raw());
        let (card, warnings) = parse_gme(&file).expect("headerless file parses");
        assert!(!card.header_present);
        assert_eq!(warnings.len(), 1);
        assert_eq!(card.raw.len(), RAW_CARD_LEN);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(parse_gme(&[0u8; RAW_CARD_LEN]).is_err());
        assert!(parse_gme(&[0u8; 100]).is_err());
    }

    #[test]
    fn rejects_missing_mc_signature() {
        let mut file = canonical_header();
        file.extend_from_slice(&vec![0u8; RAW_CARD_LEN]);
        assert!(parse_gme(&file).is_err());
    }

    #[test]
    fn decodes_header_comments() {
        let mut header = canonical_header();
        header[COMMENT_OFFSET..COMMENT_OFFSET + 5].copy_from_slice(b"Hello");
        let mut file = header;
        file.extend_from_slice(&sample_raw());
        let (card, _) = parse_gme(&file).expect("parses");
        assert_eq!(card.comments[0], "Hello");
        assert_eq!(card.comments[1], "");
    }

    const PES2_GME: &[u8] = include_bytes!("../save-games/pro-evolution-soccer-2.22507.gme");

    #[test]
    fn pes2_fixture_strips_to_raw() {
        assert_eq!(PES2_GME.len(), GME_FILE_LEN);
        let (card, warnings) = parse_gme(PES2_GME).expect("fixture parses");
        assert!(!card.header_present);
        assert_eq!(warnings.len(), 1);
        assert_eq!(&card.raw[0..2], b"MC");
        assert_eq!(card.raw.len(), RAW_CARD_LEN);
        assert_eq!(card.raw, &PES2_GME[GME_HEADER_LEN..]);
    }
}
