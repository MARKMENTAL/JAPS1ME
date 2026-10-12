//! Save imports from a source card into a target raw card image.
//!
//! The named save is copied block-for-block from the source image into the
//! first free slots of the target. Link bytes are relinked to the new slots,
//! the size field is recomputed, and touched directory checksums are
//! regenerated (the `calculateXOR` rule). Identity bytes (region, product
//! code, identifier, title, save data) are preserved verbatim. Saves imported
//! from a deleted source slot are normalized to active.

use crate::ps1::card::{
    find_links, format_slots, read_frames, SlotType, DIR_FRAMES_OFFSET, SAVE_BLOCK_LEN,
    SAVE_DATA_OFFSET, SLOT_COUNT,
};
use anyhow::{bail, Result};

/// Outcome of [`import_save`].
#[derive(Debug)]
pub struct ImportReport {
    /// Imported save name.
    pub save_name: String,
    /// Zero-based destination slots in chain order.
    pub slots: Vec<usize>,
    /// Number of slots (blocks) the save occupies.
    pub blocks: usize,
}

/// Copy the named save from `source_raw` into `target_raw`.
///
/// Both images must be 131072-byte raw cards. Returns the updated target image
/// plus a report. Fails when the save is missing from the source, already
/// exists on the target, or does not fit in the target's free slots.
///
/// To replace an existing save, use [`import_save_overwrite`].
pub fn import_save(
    target_raw: &[u8],
    source_raw: &[u8],
    save_name: &str,
) -> Result<(Vec<u8>, ImportReport)> {
    import_inner(target_raw, source_raw, save_name, false)
}

/// Copy the named save from `source_raw` into `target_raw`, replacing the
/// existing save of the same name when present.
///
/// The old chain is freed first (frames reset to formatted, data blocks
/// zeroed), then the normal first-free allocation runs, so a replacement with
/// a different block count is handled uniformly. Fails when the save is
/// missing from the source or does not fit in the target's free slots.
pub fn import_save_overwrite(
    target_raw: &[u8],
    source_raw: &[u8],
    save_name: &str,
) -> Result<(Vec<u8>, ImportReport)> {
    import_inner(target_raw, source_raw, save_name, true)
}

fn import_inner(
    target_raw: &[u8],
    source_raw: &[u8],
    save_name: &str,
    overwrite: bool,
) -> Result<(Vec<u8>, ImportReport)> {
    let (target_frames, _) = read_frames(target_raw)?;
    let (source_frames, _) = read_frames(source_raw)?;

    let source_start = (0..SLOT_COUNT)
        .find(|&slot| {
            source_frames[slot].slot_type.is_initial()
                && source_frames[slot].save_name() == save_name
        })
        .ok_or_else(|| anyhow::anyhow!("save '{save_name}' not found in source card"))?;
    let chain = find_links(&source_frames, source_start);
    let blocks = chain.len();

    let mut freed = [false; SLOT_COUNT];
    let mut out = target_raw.to_vec();
    if (0..SLOT_COUNT).any(|slot| {
        target_frames[slot].slot_type.is_initial() && target_frames[slot].save_name() == save_name
    }) {
        if !overwrite {
            bail!("save '{save_name}' already exists on target card");
        }
        // Free the old chain (frames reset to formatted, data blocks zeroed,
        // like MemcardRex `FormatSave`), then allocate normally below.
        let old_start = (0..SLOT_COUNT)
            .find(|slot| {
                target_frames[*slot].slot_type.is_initial()
                    && target_frames[*slot].save_name() == save_name
            })
            .expect("existing save has an initial slot");
        for slot in find_links(&target_frames, old_start) {
            freed[slot] = true;
        }
        format_slots(&mut out, &find_links(&target_frames, old_start));
    }

    let mut reachable = [false; SLOT_COUNT];
    for slot in 0..SLOT_COUNT {
        if target_frames[slot].slot_type.is_initial() {
            for linked in find_links(&target_frames, slot) {
                reachable[linked] = true;
            }
        }
    }
    let free: Vec<usize> = (0..SLOT_COUNT)
        .filter(|&slot| {
            freed[slot]
                || target_frames[slot].slot_type == SlotType::Formatted
                || (target_frames[slot].slot_type.is_link() && !reachable[slot])
        })
        .collect();
    if free.len() < blocks {
        bail!(
            "not enough free slots: '{save_name}' needs {blocks} block(s), target has {} free",
            free.len()
        );
    }
    let dest = &free[..blocks];

    for (index, &source_slot) in chain.iter().enumerate() {
        let dest_slot = dest[index];
        // Copy the data block verbatim.
        let source_data = SAVE_DATA_OFFSET + source_slot * SAVE_BLOCK_LEN;
        let dest_data = SAVE_DATA_OFFSET + dest_slot * SAVE_BLOCK_LEN;
        out[dest_data..dest_data + SAVE_BLOCK_LEN]
            .copy_from_slice(&source_raw[source_data..source_data + SAVE_BLOCK_LEN]);
        // Copy the directory frame, then relink it to the new slots.
        let source_frame = DIR_FRAMES_OFFSET + source_slot * crate::ps1::card::DIR_FRAME_LEN;
        let dest_frame = DIR_FRAMES_OFFSET + dest_slot * crate::ps1::card::DIR_FRAME_LEN;
        out[dest_frame..dest_frame + crate::ps1::card::DIR_FRAME_LEN].copy_from_slice(
            &source_raw[source_frame..source_frame + crate::ps1::card::DIR_FRAME_LEN],
        );
        let (slot_type, next, next_hi) = if blocks == 1 {
            (0x51u8, 0xFFu8, 0xFFu8)
        } else if index == 0 {
            (0x51, dest[1] as u8, 0x00)
        } else if index == blocks - 1 {
            (0x53, 0xFF, 0xFF)
        } else {
            (0x52, dest[index + 1] as u8, 0x00)
        };
        out[dest_frame] = slot_type;
        out[dest_frame + 8] = next;
        out[dest_frame + 9] = next_hi;
        if index == 0 {
            let size = (blocks * SAVE_BLOCK_LEN) as u32;
            out[dest_frame + 4] = (size & 0xFF) as u8;
            out[dest_frame + 5] = ((size >> 8) & 0xFF) as u8;
            out[dest_frame + 6] = ((size >> 16) & 0xFF) as u8;
        }
        let mut xor = 0u8;
        for byte in &out[dest_frame..dest_frame + crate::ps1::card::DIR_FRAME_LEN - 1] {
            xor ^= *byte;
        }
        out[dest_frame + crate::ps1::card::DIR_FRAME_LEN - 1] = xor;
    }

    Ok((
        out,
        ImportReport {
            save_name: save_name.to_owned(),
            slots: dest.to_vec(),
            blocks,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ps1::card::{parse_raw, DIR_FRAME_LEN};

    fn blank_raw() -> Vec<u8> {
        let mut raw = vec![0u8; crate::gme::RAW_CARD_LEN];
        raw[0] = b'M';
        raw[1] = b'C';
        for slot in 0..SLOT_COUNT {
            let start = DIR_FRAMES_OFFSET + slot * DIR_FRAME_LEN;
            raw[start] = 0xA0;
            raw[start + 8] = 0xFF;
            raw[start + 9] = 0xFF;
            fix_xor(&mut raw, start);
        }
        raw
    }

    fn fix_xor(raw: &mut [u8], start: usize) {
        let mut xor = 0u8;
        for byte in &raw[start..start + DIR_FRAME_LEN - 1] {
            xor ^= byte;
        }
        raw[start + DIR_FRAME_LEN - 1] = xor;
    }

    fn make_save(
        raw: &mut [u8],
        slot: usize,
        next: u8,
        region: &[u8; 2],
        prod: &[u8],
        ident: &[u8],
        title: &[u8],
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
        fix_xor(raw, start);
        let data = SAVE_DATA_OFFSET + slot * SAVE_BLOCK_LEN;
        raw[data + 4..data + 4 + title.len().min(64)]
            .copy_from_slice(&title[..title.len().min(64)]);
    }

    fn make_end_link(raw: &mut [u8], slot: usize) {
        let start = DIR_FRAMES_OFFSET + slot * DIR_FRAME_LEN;
        raw[start] = 0x53;
        raw[start + 8] = 0xFF;
        raw[start + 9] = 0xFF;
        fix_xor(raw, start);
        let data = SAVE_DATA_OFFSET + slot * SAVE_BLOCK_LEN;
        raw[data] = 0xAB;
    }

    #[test]
    fn imports_single_block_save() {
        let mut source = blank_raw();
        make_save(
            &mut source,
            0,
            0xFF,
            b"BE",
            b"SLES-03946",
            b"PES-OPT",
            b"PES-OPT",
        );
        let target = blank_raw();
        let (out, report) = import_save(&target, &source, "BESLES-03946PES-OPT").expect("imports");
        assert_eq!(report.blocks, 1);
        assert_eq!(report.slots, vec![0]);
        let summary = parse_raw(&out).expect("re-parses");
        assert!(summary.warnings.is_empty());
        assert_eq!(summary.saves.len(), 1);
        assert_eq!(summary.saves[0].save_name, "BESLES-03946PES-OPT");
        assert_eq!(summary.saves[0].game_title, "PES-OPT");
        assert_eq!(summary.free_slots, SLOT_COUNT - 1);
    }

    #[test]
    fn allocates_first_free_slots_in_order() {
        let mut source = blank_raw();
        make_save(&mut source, 0, 1, b"BA", b"SLUS-12345", b"SAVE1", b"Save");
        make_end_link(&mut source, 1);
        let mut target = blank_raw();
        make_save(
            &mut target,
            0,
            0xFF,
            b"BE",
            b"SLES-00001",
            b"OTHER",
            b"Other",
        );
        let (out, report) = import_save(&target, &source, "BASLUS-12345SAVE1").expect("imports");
        assert_eq!(report.slots, vec![1, 2]);
        let summary = parse_raw(&out).expect("re-parses");
        assert!(summary.warnings.is_empty());
        assert_eq!(summary.saves.len(), 2);
        assert_eq!(summary.saves[1].blocks, 2);
        assert_eq!(summary.free_slots, SLOT_COUNT - 3);
    }

    #[test]
    fn rejects_duplicates() {
        let mut source = blank_raw();
        make_save(
            &mut source,
            0,
            0xFF,
            b"BE",
            b"SLES-03946",
            b"PES-OPT",
            b"PES-OPT",
        );
        let (target, _) = import_save(&blank_raw(), &source, "BESLES-03946PES-OPT").expect("first");
        let err = import_save(&target, &source, "BESLES-03946PES-OPT").unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn rejects_missing_save() {
        let err = import_save(&blank_raw(), &blank_raw(), "BENOPE-00000NOPE").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn rejects_when_full() {
        let mut source = blank_raw();
        make_save(&mut source, 0, 1, b"BA", b"SLUS-12345", b"SAVE1", b"Save");
        make_end_link(&mut source, 1);
        let mut target = blank_raw();
        for slot in 0..SLOT_COUNT - 1 {
            make_save(
                &mut target,
                slot,
                0xFF,
                b"BE",
                format!("SLES-{slot:05}").as_bytes(),
                b"FILLER",
                b"Filler",
            );
        }
        let err = import_save(&target, &source, "BASLUS-12345SAVE1").unwrap_err();
        assert!(err.to_string().contains("not enough free slots"));
    }

    #[test]
    fn normalizes_deleted_source_to_active() {
        let mut source = blank_raw();
        make_save(
            &mut source,
            0,
            0xFF,
            b"BE",
            b"SLES-03946",
            b"PES-OPT",
            b"PES-OPT",
        );
        source[DIR_FRAMES_OFFSET] = 0xA1;
        fix_xor(&mut source, DIR_FRAMES_OFFSET);
        let (out, _) = import_save(&blank_raw(), &source, "BESLES-03946PES-OPT").expect("imports");
        let summary = parse_raw(&out).expect("re-parses");
        assert_eq!(summary.saves.len(), 1);
        assert!(!summary.saves[0].deleted);
        assert!(summary.warnings.is_empty());
    }

    const PES2_GME: &[u8] = include_bytes!("../../save-games/pro-evolution-soccer-2.22507.gme");

    #[test]
    fn pes2_fixture_import_roundtrip() {
        let source = &PES2_GME[crate::gme::GME_HEADER_LEN..];
        let (out, report) =
            import_save(&blank_raw(), source, "BESLES-03946PES-OPT").expect("imports");
        assert_eq!(report.blocks, 2);
        assert_eq!(report.slots, vec![0, 1]);
        let summary = parse_raw(&out).expect("re-parses");
        assert!(summary.warnings.is_empty());
        assert_eq!(summary.saves.len(), 1);
        assert_eq!(summary.saves[0].save_name, "BESLES-03946PES-OPT");
        assert_eq!(summary.saves[0].region, "Europe");
        assert_eq!(
            summary.saves[0].game_title,
            parse_raw(source).expect("source parses").saves[0].game_title
        );
        assert_eq!(summary.free_slots, 13);
    }

    #[test]
    fn overwrite_replaces_with_more_blocks() {
        let mut old_source = blank_raw();
        make_save(
            &mut old_source,
            0,
            0xFF,
            b"BE",
            b"SLES-03946",
            b"PES-OPT",
            b"Old title",
        );
        let (target, _) =
            import_save(&blank_raw(), &old_source, "BESLES-03946PES-OPT").expect("first import");
        let mut new_source = blank_raw();
        make_save(
            &mut new_source,
            0,
            1,
            b"BE",
            b"SLES-03946",
            b"PES-OPT",
            b"New title",
        );
        make_end_link(&mut new_source, 1);
        let (out, report) =
            import_save_overwrite(&target, &new_source, "BESLES-03946PES-OPT").expect("overwrites");
        assert_eq!(report.blocks, 2);
        assert_eq!(report.slots, vec![0, 1]);
        let summary = parse_raw(&out).expect("re-parses");
        assert!(summary.warnings.is_empty());
        assert_eq!(summary.saves.len(), 1);
        assert_eq!(summary.saves[0].game_title, "New title");
        assert_eq!(summary.saves[0].blocks, 2);
        assert_eq!(summary.free_slots, SLOT_COUNT - 2);
    }

    #[test]
    fn overwrite_replaces_with_fewer_blocks() {
        let mut old_source = blank_raw();
        make_save(
            &mut old_source,
            0,
            1,
            b"BE",
            b"SLES-03946",
            b"PES-OPT",
            b"Old title",
        );
        make_end_link(&mut old_source, 1);
        let (target, _) =
            import_save(&blank_raw(), &old_source, "BESLES-03946PES-OPT").expect("first import");
        let mut new_source = blank_raw();
        make_save(
            &mut new_source,
            0,
            0xFF,
            b"BE",
            b"SLES-03946",
            b"PES-OPT",
            b"New title",
        );
        let (out, report) =
            import_save_overwrite(&target, &new_source, "BESLES-03946PES-OPT").expect("overwrites");
        assert_eq!(report.blocks, 1);
        let summary = parse_raw(&out).expect("re-parses");
        assert!(summary.warnings.is_empty());
        assert_eq!(summary.saves.len(), 1);
        assert_eq!(summary.saves[0].game_title, "New title");
        assert_eq!(summary.free_slots, SLOT_COUNT - 1);
    }

    #[test]
    fn overwrite_missing_save_still_fails() {
        let err =
            import_save_overwrite(&blank_raw(), &blank_raw(), "BENOPE-00000NOPE").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
