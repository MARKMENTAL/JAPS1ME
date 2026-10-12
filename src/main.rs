use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use japs1me::gme::{parse_gme, GME_FILE_LEN, GME_HEADER_LEN, RAW_CARD_LEN};
use japs1me::install::{
    backup_path, find_srm_files, load_saved_dir, parse_selection, save_dir_config,
};
use japs1me::{delete_save, format_save_table, import_save, import_save_overwrite, parse_raw};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "japs1me",
    version,
    about = "JAPS1ME - PS1 RetroArch Save Converter (DexDrive .gme to RetroArch .srm)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List saves on a .gme or raw (.srm/.mcr) card image.
    List { card: String },
    /// Convert a DexDrive .gme file to a RetroArch .srm file.
    Convert { input: String, output: String },
    /// Import a save from a .gme file into a RetroArch .srm card.
    Import {
        card: String,
        source: String,
        name: String,
    },
    /// Delete a save from a RetroArch .srm card by slot number or save name.
    Delete { card: String, target: String },
    /// Install a .srm card into the configured RetroArch saves directory.
    Install { source: String },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::List { card } => {
            let bytes = std::fs::read(&card)
                .with_context(|| format!("reading {}", Path::new(&card).display()))?;
            let (raw, warnings) = load_raw(&bytes, &card)?;
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
            let summary = parse_raw(&raw)?;
            for warning in &summary.warnings {
                eprintln!("warning: {warning}");
            }
            println!("{}", format_save_table(&summary.saves, summary.free_slots));
        }
        Command::Convert { input, output } => {
            let bytes = std::fs::read(&input)
                .with_context(|| format!("reading {}", Path::new(&input).display()))?;
            if bytes.len() != GME_FILE_LEN {
                bail!(
                    "not a DexDrive .gme file: expected {GME_FILE_LEN} bytes, found {}",
                    bytes.len()
                );
            }
            let (card, warnings) = parse_gme(&bytes)?;
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
            let summary = parse_raw(&card.raw)?;
            for warning in &summary.warnings {
                eprintln!("warning: {warning}");
            }
            write_output_atomically(&output, &card.raw)?;
            eprintln!(
                "Converted {} ({} save(s)) to {}",
                Path::new(&input).display(),
                summary.saves.len(),
                Path::new(&output).display()
            );
        }
        Command::Import { card, source, name } => {
            let source_bytes = std::fs::read(&source)
                .with_context(|| format!("reading {}", Path::new(&source).display()))?;
            if source_bytes.len() != GME_FILE_LEN {
                bail!(
                    "source must be a DexDrive .gme file: expected {GME_FILE_LEN} bytes, found {}",
                    source_bytes.len()
                );
            }
            let (source_card, warnings) = parse_gme(&source_bytes)?;
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
            let target_bytes = std::fs::read(&card)
                .with_context(|| format!("reading {}", Path::new(&card).display()))?;
            if target_bytes.len() != RAW_CARD_LEN {
                bail!(
                    "import target must be a raw .srm image ({RAW_CARD_LEN} bytes); refusing to modify '{card}'"
                );
            }
            let target_summary = parse_raw(&target_bytes)?;
            for warning in &target_summary.warnings {
                eprintln!("warning: {warning}");
            }
            let (new_raw, report) = if target_summary
                .saves
                .iter()
                .any(|save| save.save_name == name)
            {
                require_terminal(&format!("save '{name}' already exists on target card"))?;
                if !prompt_yes(&format!(
                    "Save {name} already exists on {card}, overwrite it with the one from {source}? (y/n) "
                ))? {
                    bail!("import cancelled; '{card}' unchanged");
                }
                import_save_overwrite(&target_bytes, &source_card.raw, &name)?
            } else {
                import_save(&target_bytes, &source_card.raw, &name)?
            };
            let summary = parse_raw(&new_raw)?;
            for warning in &summary.warnings {
                eprintln!("warning: {warning}");
            }
            replace_file_atomically(&card, &new_raw)?;
            let slots = report
                .slots
                .iter()
                .map(|slot| (slot + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!(
                "Imported {} ({} block(s), slot(s) {}) into {}",
                report.save_name,
                report.blocks,
                slots,
                Path::new(&card).display()
            );
        }
        Command::Delete { card, target } => {
            let target_bytes = std::fs::read(&card)
                .with_context(|| format!("reading {}", Path::new(&card).display()))?;
            if target_bytes.len() != RAW_CARD_LEN {
                bail!(
                    "delete target must be a raw .srm image ({RAW_CARD_LEN} bytes); refusing to modify '{card}'"
                );
            }
            let (new_raw, report) = delete_save(&target_bytes, &target)?;
            let summary = parse_raw(&new_raw)?;
            for warning in &summary.warnings {
                eprintln!("warning: {warning}");
            }
            replace_file_atomically(&card, &new_raw)?;
            eprintln!(
                "Deleted {} ({} block(s)), {} of 15 free",
                report.save_name, report.blocks, summary.free_slots
            );
        }
        Command::Install { source } => {
            let source_bytes = std::fs::read(&source)
                .with_context(|| format!("reading {}", Path::new(&source).display()))?;
            if source_bytes.len() != RAW_CARD_LEN
                || source_bytes[0] != b'M'
                || source_bytes[1] != b'C'
            {
                bail!(
                    "install source must be a raw .srm image ({RAW_CARD_LEN} bytes starting with MC)"
                );
            }
            let source_summary = parse_raw(&source_bytes)?;
            for warning in &source_summary.warnings {
                eprintln!("warning: {warning}");
            }
            let dir = match load_saved_dir()? {
                Some(dir) if dir.is_dir() => dir,
                _ => {
                    require_terminal(
                        "no RetroArch saves directory configured; run interactively to set one",
                    )?;
                    let answer = prompt_line("Enter your RetroArch PS1 saves directory: ")?
                        .ok_or_else(|| anyhow!("no directory given; nothing changed"))?;
                    if answer.trim().is_empty() {
                        bail!("no directory given; nothing changed");
                    }
                    let dir = PathBuf::from(answer.trim());
                    if !dir.is_dir() {
                        bail!("not a directory: '{}'; nothing changed", answer.trim());
                    }
                    let saved = save_dir_config(&dir)?;
                    eprintln!("Saved saves directory to {}", saved.display());
                    dir
                }
            };
            let cards = find_srm_files(&dir)?;
            if cards.is_empty() {
                bail!("no .srm files found in {}", dir.display());
            }
            for (index, card) in cards.iter().enumerate() {
                let name = card
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_else(|| card.as_os_str().to_string_lossy());
                eprintln!("  {}: {name}", index + 1);
            }
            require_terminal("cannot choose a memory card without a terminal")?;
            let choice = prompt_line("Which memory card to overwrite? (number) ")?
                .ok_or_else(|| anyhow!("no selection given; nothing changed"))?;
            let index = parse_selection(&choice, cards.len())
                .map_err(|error| anyhow!("{error}; nothing changed"))?;
            let target = &cards[index];
            if !prompt_yes(&format!(
                "Overwrite {} with {}? (y/n) ",
                target.display(),
                Path::new(&source).display()
            ))? {
                bail!("install cancelled; '{}' unchanged", target.display());
            }
            let backup = backup_path(target);
            if backup.symlink_metadata().is_ok() {
                bail!(
                    "backup already exists: {}; move or delete it first",
                    backup.display()
                );
            }
            std::fs::copy(target, &backup).with_context(|| {
                format!("backing up {} to {}", target.display(), backup.display())
            })?;
            replace_file_atomically(&target.to_string_lossy(), &source_bytes)?;
            let check =
                std::fs::read(target).with_context(|| format!("reading {}", target.display()))?;
            parse_raw(&check).with_context(|| format!("verifying {}", target.display()))?;
            eprintln!(
                "Installed {} as {} (backup: {})",
                Path::new(&source).display(),
                target.display(),
                backup.display()
            );
        }
    }
    Ok(())
}

/// Abort with `message` when stdin is not a terminal.
fn require_terminal(message: &str) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        return Err(anyhow!("{message}"));
    }
    Ok(())
}

/// Print `prompt` and read one line; `Ok(None)` on EOF.
fn prompt_line(prompt: &str) -> Result<Option<String>> {
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut answer = String::new();
    let bytes_read = std::io::stdin()
        .read_line(&mut answer)
        .context("reading answer")?;
    if bytes_read == 0 {
        return Ok(None);
    }
    Ok(Some(answer))
}

/// Print `prompt` and return true only for an explicit `y`/`yes` (strict).
fn prompt_yes(prompt: &str) -> Result<bool> {
    Ok(prompt_line(prompt)?
        .is_some_and(|answer| matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")))
}

/// Load a raw card image from either `.gme` (stripped) or raw (`.srm`/`.mcr`) bytes.
fn load_raw(bytes: &[u8], name: &str) -> Result<(Vec<u8>, Vec<String>)> {
    if bytes.len() == GME_FILE_LEN
        && bytes[GME_HEADER_LEN] == b'M'
        && bytes[GME_HEADER_LEN + 1] == b'C'
    {
        let (card, warnings) = parse_gme(bytes)?;
        return Ok((card.raw, warnings));
    }
    if bytes.len() == RAW_CARD_LEN && bytes[0] == b'M' && bytes[1] == b'C' {
        return Ok((bytes.to_vec(), Vec::new()));
    }
    bail!(
        "unsupported card format for '{name}': expected a .gme file ({GME_FILE_LEN} bytes) or a raw .srm image ({RAW_CARD_LEN} bytes starting with MC)"
    )
}

fn write_output_atomically(output: &str, data: &[u8]) -> Result<()> {
    let output = PathBuf::from(output);
    if output.symlink_metadata().is_ok() {
        bail!("output already exists: {}", output.display());
    }
    write_temp_then_persist(&output, data, "creating temporary output file")
}

/// Replace an existing card file with new bytes via a staged temporary file.
///
/// The replacement only happens after the caller has validated `data`, so the
/// target is never left half-written.
fn replace_file_atomically(card: &str, data: &[u8]) -> Result<()> {
    let card = PathBuf::from(card);
    write_temp_then_persist(&card, data, "creating temporary card file")
}

fn write_temp_then_persist(output: &Path, data: &[u8], context: &'static str) -> Result<()> {
    use std::io::Write;
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let mut temp = tempfile::NamedTempFile::new_in(parent.unwrap_or_else(|| Path::new(".")))
        .context(context)?;
    temp.write_all(data)
        .context("writing temporary output file")?;
    temp.persist(output)
        .with_context(|| format!("writing {}", output.display()))?;
    Ok(())
}
