//! Save deletion from a target raw card image.
//!
//! The save's chain is reset to formatted slots (frames plus zeroed data
//! blocks), freeing them for future imports.

use crate::ps1::card::{find_links, format_slots, read_frames, SLOT_COUNT};
use anyhow::{bail, Result};

/// Outcome of [`delete_save`].
#[derive(Debug)]
pub struct DeleteReport {
    /// Deleted save name.
    pub save_name: String,
    /// Zero-based freed slots in chain order.
    pub slots: Vec<usize>,
    /// Number of slots (blocks) freed.
    pub blocks: usize,
}

/// Delete the save selected by 1-based slot number or exact save name.
///
/// All-digit selections are slot numbers (which must start a save); anything
/// else is matched against save names. Returns the updated target image plus
/// a report.
pub fn delete_save(target_raw: &[u8], select: &str) -> Result<(Vec<u8>, DeleteReport)> {
    let (frames, _) = read_frames(target_raw)?;
    let start = if let Ok(number) = select.trim().parse::<usize>() {
        if number == 0 || number > SLOT_COUNT {
            bail!("no save at slot '{select}': slots run 1..={SLOT_COUNT}");
        }
        if !frames[number - 1].slot_type.is_initial() {
            bail!("no save starts at slot {number}");
        }
        number - 1
    } else {
        (0..SLOT_COUNT)
            .find(|&slot| frames[slot].slot_type.is_initial() && frames[slot].save_name() == select)
            .ok_or_else(|| anyhow::anyhow!("save '{select}' not found on card"))?
    };
    let chain = find_links(&frames, start);
    let save_name = frames[start].save_name();
    let mut out = target_raw.to_vec();
    format_slots(&mut out, &chain);
    Ok((
        out,
        DeleteReport {
            save_name,
            slots: chain.clone(),
            blocks: chain.len(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ps1::card::{DIR_FRAMES_OFFSET, DIR_FRAME_LEN, SAVE_BLOCK_LEN, SAVE_DATA_OFFSET};

    fn blank_raw() -> Vec<u8> {
        let mut raw = vec![0u8; crate::gme::RAW_CARD_LEN];
        raw[0] = b'M';
        raw[1] = b'C';
        for slot in 0..SLOT_COUNT {
            let start = DIR_FRAMES_OFFSET + slot * DIR_FRAME_LEN;
            raw[start] = 0xA0;
            raw[start + 8] = 0xFF;
            raw[start + 9] = 0xFF;
            let mut xor = 0u8;
            for byte in &raw[start..start + DIR_FRAME_LEN - 1] {
                xor ^= byte;
            }
            raw[start + DIR_FRAME_LEN - 1] = xor;
        }
        raw
    }

    fn make_save(
        raw: &mut [u8],
        slot: usize,
        next: u8,
        region: &[u8; 2],
        prod: &[u8],
        ident: &[u8],
    ) {
        let start = DIR_FRAMES_OFFSET + slot * DIR_FRAME_LEN;
        raw[start] = 0x51;
        raw[start + 4] = 0x00;
        raw[start + 5] = 0x20;
        raw[start + 8] = next;
        raw[start + 9] = if next == 0xFF { 0xFF } else { 0x00 };
        raw[start + 10..start + 12].copy_from_slice(region);
        raw[start + 12..start + 12 + prod.len().min(10)]
            .copy_from_slice(&prod[..prod.len().min(10)]);
        raw[start + 22..start + 22 + ident.len().min(8)]
            .copy_from_slice(&ident[..ident.len().min(8)]);
        let mut xor = 0u8;
        for byte in &raw[start..start + DIR_FRAME_LEN - 1] {
            xor ^= byte;
        }
        raw[start + DIR_FRAME_LEN - 1] = xor;
        let data = SAVE_DATA_OFFSET + slot * SAVE_BLOCK_LEN;
        raw[data] = 0xAB;
    }

    fn make_end_link(raw: &mut [u8], slot: usize) {
        let start = DIR_FRAMES_OFFSET + slot * DIR_FRAME_LEN;
        raw[start] = 0x53;
        raw[start + 8] = 0xFF;
        raw[start + 9] = 0xFF;
        let mut xor = 0u8;
        for byte in &raw[start..start + DIR_FRAME_LEN - 1] {
            xor ^= byte;
        }
        raw[start + DIR_FRAME_LEN - 1] = xor;
        raw[SAVE_DATA_OFFSET + slot * SAVE_BLOCK_LEN] = 0xCD;
    }

    fn two_save_card() -> Vec<u8> {
        let mut raw = blank_raw();
        make_save(&mut raw, 0, 1, b"BE", b"SLES-03946", b"PES-OPT");
        make_end_link(&mut raw, 1);
        make_save(&mut raw, 3, 0xFF, b"BA", b"SLUS-12345", b"SAVE1");
        raw
    }

    #[test]
    fn deletes_by_name() {
        let (out, report) = delete_save(&two_save_card(), "BESLES-03946PES-OPT").expect("deletes");
        assert_eq!(report.save_name, "BESLES-03946PES-OPT");
        assert_eq!(report.slots, vec![0, 1]);
        assert_eq!(report.blocks, 2);
        let summary = crate::ps1::card::parse_raw(&out).expect("re-parses");
        assert!(summary.warnings.is_empty());
        assert_eq!(summary.saves.len(), 1);
        assert_eq!(summary.saves[0].save_name, "BASLUS-12345SAVE1");
        assert_eq!(summary.free_slots, 14);
    }

    #[test]
    fn deletes_by_slot_number() {
        let (out, report) = delete_save(&two_save_card(), "4").expect("deletes");
        assert_eq!(report.save_name, "BASLUS-12345SAVE1");
        assert_eq!(report.slots, vec![3]);
        let summary = crate::ps1::card::parse_raw(&out).expect("re-parses");
        assert!(summary.warnings.is_empty());
        assert_eq!(summary.saves.len(), 1);
        assert_eq!(summary.free_slots, 13);
    }

    #[test]
    fn rejects_bad_selection() {
        assert!(delete_save(&two_save_card(), "0").is_err());
        assert!(delete_save(&two_save_card(), "16").is_err());
        // Slot 2 is an end link, not a save start.
        assert!(delete_save(&two_save_card(), "2").is_err());
        // Formatted slot with no save.
        assert!(delete_save(&two_save_card(), "5").is_err());
        assert!(delete_save(&two_save_card(), "BENOPE-00000NOPE").is_err());
    }
}
