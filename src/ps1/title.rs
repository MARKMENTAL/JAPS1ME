//! Shift-JIS save-title decoding for PS1 saves.
//!
//! Each save's data block starts with a 64-byte Shift-JIS title at offset 4.
//! The field is NUL-terminated on an even byte boundary (matching MemcardRex's
//! `loadStringData` behavior).

/// Length of the title field in a save data block.
pub const TITLE_LEN: usize = 64;

/// Decode a save title from the 64-byte field at data offset 4.
///
/// Returns `None` when the field carries no usable text.
pub fn decode_title(field: &[u8]) -> Option<String> {
    let mut len = field.len().min(TITLE_LEN);
    for (index, &byte) in field.iter().enumerate().take(len) {
        if index % 2 == 0 && byte == 0 {
            len = index;
            break;
        }
    }
    let field = &field[..len];
    let (text, _, _) = encoding_rs::SHIFT_JIS.decode(field);
    let title = text
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_owned();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_ascii_title() {
        let mut field = [0u8; TITLE_LEN];
        field[..7].copy_from_slice(b"PES-OPT");
        assert_eq!(decode_title(&field).as_deref(), Some("PES-OPT"));
    }

    #[test]
    fn decodes_shift_jis_title() {
        // "ゲーム" in Shift-JIS.
        let mut field = [0u8; TITLE_LEN];
        field[..6].copy_from_slice(&[0x83, 0x51, 0x81, 0x5b, 0x83, 0x80]);
        assert_eq!(decode_title(&field).as_deref(), Some("ゲーム"));
    }

    #[test]
    fn returns_none_for_empty_title() {
        assert_eq!(decode_title(&[0u8; TITLE_LEN]), None);
    }

    #[test]
    fn stops_at_even_nul() {
        let mut field = [0u8; TITLE_LEN];
        field[..3].copy_from_slice(b"ABC");
        // NUL at even index 4 terminates; trailing bytes are ignored.
        field[6..9].copy_from_slice(b"XYZ");
        assert_eq!(decode_title(&field).as_deref(), Some("ABC"));
    }
}
