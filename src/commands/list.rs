//! Unicode table rendering for `japs1me list`.

use crate::ps1::card::SlotInfo;
use unicode_width::UnicodeWidthStr;

const HEADERS: [&str; 7] = [
    "Slot", "Save", "Title", "Region", "Blocks", "Size", "Status",
];

/// Format a byte count with binary units.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 1;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn pad_cell(content: &str, width: usize, right_align: bool) -> String {
    let content_width = UnicodeWidthStr::width(content);
    let padding = width.saturating_sub(content_width);
    if right_align {
        format!("{}{content}", " ".repeat(padding))
    } else {
        format!("{content}{}", " ".repeat(padding))
    }
}

/// Render saves as a Unicode table with a free-slot footer.
pub fn format_save_table(saves: &[SlotInfo], free_slots: usize) -> String {
    let rows: Vec<[String; 7]> = saves
        .iter()
        .map(|save| {
            [
                (save.slot + 1).to_string(),
                save.save_name.clone(),
                save.game_title.clone(),
                save.region.clone(),
                save.blocks.to_string(),
                format_size(save.size_bytes as u64),
                if save.deleted {
                    "deleted".to_owned()
                } else {
                    "active".to_owned()
                },
            ]
        })
        .collect();

    let mut widths = HEADERS.map(UnicodeWidthStr::width);
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let border = |left: &str, middle: &str, right: &str| -> String {
        let mut line = String::from(left);
        for (index, width) in widths.iter().enumerate() {
            if index > 0 {
                line.push_str(middle);
            }
            line.push_str(&"─".repeat(width + 2));
        }
        line.push_str(right);
        line
    };
    let row_line = |cells: &[String; 7]| -> String {
        let mut line = String::from("│");
        for (index, cell) in cells.iter().enumerate() {
            let right_align = matches!(index, 0 | 4 | 5);
            line.push(' ');
            line.push_str(&pad_cell(cell, widths[index], right_align));
            line.push_str(" │");
        }
        line
    };

    let header_cells = HEADERS.map(String::from);
    let mut output = String::new();
    output.push_str(&border("┌", "┬", "┐"));
    output.push('\n');
    output.push_str(&row_line(&header_cells));
    output.push('\n');
    output.push_str(&border("├", "┼", "┤"));
    output.push('\n');
    for row in &rows {
        output.push_str(&row_line(row));
        output.push('\n');
    }
    output.push_str(&border("└", "┴", "┘"));
    output.push('\n');
    output.push_str(&format!("Free slots: {free_slots} of 15"));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ps1::card::SlotInfo;

    fn sample_save() -> SlotInfo {
        SlotInfo {
            slot: 0,
            save_name: "BESLES-03946PES-OPT".to_owned(),
            region: "Europe".to_owned(),
            game_title: "PES Option File".to_owned(),
            blocks: 2,
            size_bytes: 16384,
            deleted: false,
        }
    }

    #[test]
    fn renders_table_and_footer() {
        let table = format_save_table(&[sample_save()], 11);
        assert!(table.contains("┌"));
        assert!(table.contains("Slot"));
        assert!(table.contains("BESLES-03946PES-OPT"));
        assert!(table.contains("PES Option File"));
        assert!(table.contains("Free slots: 11 of 15"));
    }

    #[test]
    fn renders_empty_card() {
        let table = format_save_table(&[], 15);
        assert!(table.contains("Slot"));
        assert!(table.contains("Free slots: 15 of 15"));
    }

    #[test]
    fn marks_deleted_saves() {
        let mut save = sample_save();
        save.deleted = true;
        let table = format_save_table(&[save], 13);
        assert!(table.contains("deleted"));
    }
}
