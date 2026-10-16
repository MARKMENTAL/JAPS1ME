use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use japs1me::gme::{parse_gme, GME_FILE_LEN, GME_HEADER_LEN, RAW_CARD_LEN};
use japs1me::install::{
    backup_path, detect_emulator, find_card_files, find_match_by_name, is_multi_disc,
    load_saved_dir, new_card_path, parse_selection, resolve_in_saves_dir, save_dir_config,
};
use japs1me::{delete_save, format_save_table, import_save, import_save_overwrite, parse_raw};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "japs1me",
    version,
    about = "JAPS1ME - Just A PS1 Memory Card Editor (DexDrive .gme, RetroArch .srm, DuckStation .mcd)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List saves on one or more .gme or raw (.srm/.mcd/.mcr) card images.
    List {
        #[arg(required = true, num_args = 1..)]
        cards: Vec<String>,
    },
    /// Convert a DexDrive .gme file to a raw .srm/.mcd card image.
    Convert { input: String, output: String },
    /// Import a save from a .gme file into a raw .srm/.mcd card.
    Import {
        card: String,
        source: String,
        name: String,
    },
    /// Delete a save from a raw .srm/.mcd card by slot number or save name.
    Delete { card: String, target: String },
    /// Copy an emulator card from its configured saves directory into this workspace.
    Copy { source: String, dest: String },
    /// Configure the RetroArch or DuckStation memory-card directory used by copy/install.
    Setup,
    /// Install a raw card into the configured RetroArch or DuckStation directory.
    Install {
        /// Create a convention-named card instead of choosing an existing card to overwrite.
        #[arg(short = 'n', long = "new")]
        new_card: bool,
        source: String,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::List { cards } => {
            let multiple_cards = cards.len() > 1;
            for (index, card) in cards.iter().enumerate() {
                let bytes = std::fs::read(card)
                    .with_context(|| format!("reading {}", Path::new(card).display()))?;
                let (raw, warnings) = load_raw(&bytes, card)?;
                for warning in warnings {
                    eprintln!("warning: {}: {warning}", Path::new(card).display());
                }
                let summary = parse_raw(&raw)?;
                for warning in &summary.warnings {
                    eprintln!("warning: {}: {warning}", Path::new(card).display());
                }
                if multiple_cards {
                    if index > 0 {
                        println!();
                    }
                    println!("== {} ==", Path::new(card).display());
                }
                println!("{}", format_save_table(&summary.saves, summary.free_slots));
            }
        }
        Command::Convert { input, output } => {
            let bytes = std::fs::read(&input)
                .with_context(|| format!("reading {}", Path::new(&input).display()))?;
            if bytes.len() == RAW_CARD_LEN && bytes[0] == b'M' && bytes[1] == b'C' {
                bail!(
                    "input '{input}' is already a raw memory-card image ({RAW_CARD_LEN} bytes); copy or rename it instead of converting"
                );
            }
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
            let (source_raw, warnings) = load_raw(&source_bytes, &source).map_err(|_| {
                anyhow!(
                    "source must be a DexDrive .gme file ({GME_FILE_LEN} bytes) or a raw memory-card image (.srm/.mcd; {RAW_CARD_LEN} bytes starting with MC): found {} bytes in '{}'",
                    source_bytes.len(),
                    source
                )
            })?;
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
            if source_bytes.len() == RAW_CARD_LEN {
                eprintln!(
                    "warning: source '{source}' is raw-sized; treating it as a raw memory-card image despite the .gme extension"
                );
            }
            let target_bytes = std::fs::read(&card)
                .with_context(|| format!("reading {}", Path::new(&card).display()))?;
            if target_bytes.len() != RAW_CARD_LEN {
                bail!(
                    "import target must be a raw .srm/.mcd memory-card image ({RAW_CARD_LEN} bytes); refusing to modify '{card}'"
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
                import_save_overwrite(&target_bytes, &source_raw, &name)?
            } else {
                import_save(&target_bytes, &source_raw, &name)?
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
                    "delete target must be a raw .srm/.mcd memory-card image ({RAW_CARD_LEN} bytes); refusing to modify '{card}'"
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
        Command::Copy { source, dest } => {
            let resolved = resolve_copy_source(&source)?;
            let source_bytes = std::fs::read(&resolved)
                .with_context(|| format!("reading {}", resolved.display()))?;
            if source_bytes.len() != RAW_CARD_LEN
                || source_bytes[0] != b'M'
                || source_bytes[1] != b'C'
            {
                bail!(
                    "copy source must be a raw .srm/.mcd memory-card image ({RAW_CARD_LEN} bytes starting with MC)"
                );
            }
            let source_summary = parse_raw(&source_bytes)?;
            for warning in &source_summary.warnings {
                eprintln!("warning: {warning}");
            }
            let dest_path = resolve_copy_destination(&dest, &resolved)?;
            if same_file(&resolved, &dest_path) {
                bail!(
                    "source and destination are the same file ({}); nothing changed",
                    dest_path.display()
                );
            }
            if dest_path.symlink_metadata().is_ok() {
                require_terminal("cannot confirm overwrite without a terminal")?;
                if !prompt_yes(&format!(
                    "Overwrite {} with {}? (y/n) ",
                    dest_path.display(),
                    resolved.display()
                ))? {
                    bail!("copy cancelled; '{}' unchanged", dest_path.display());
                }
            }
            write_temp_then_persist(&dest_path, &source_bytes, "creating temporary copy file")
                .with_context(|| format!("writing {}", dest_path.display()))?;
            let check = std::fs::read(&dest_path)
                .with_context(|| format!("reading {}", dest_path.display()))?;
            parse_raw(&check).with_context(|| format!("verifying {}", dest_path.display()))?;
            eprintln!(
                "Copied {} to {} ({} save(s))",
                resolved.display(),
                dest_path.display(),
                source_summary.saves.len()
            );
        }
        Command::Setup => {
            let dir = ensure_saved_dir()?;
            let emulator = detect_emulator(&dir);
            eprintln!(
                "{} memory-card directory: {} (.{})",
                emulator.name(),
                dir.display(),
                emulator.card_extension()
            );
        }
        Command::Install { source, new_card } => {
            let source_bytes = std::fs::read(&source)
                .with_context(|| format!("reading {}", Path::new(&source).display()))?;
            if source_bytes.len() != RAW_CARD_LEN
                || source_bytes[0] != b'M'
                || source_bytes[1] != b'C'
            {
                bail!(
                    "install source must be a raw .srm/.mcd memory-card image ({RAW_CARD_LEN} bytes starting with MC)"
                );
            }
            let source_summary = parse_raw(&source_bytes)?;
            for warning in &source_summary.warnings {
                eprintln!("warning: {warning}");
            }
            let dir = ensure_saved_dir()?;
            let emulator = detect_emulator(&dir);
            let extension = emulator.card_extension();
            let cards = find_card_files(&dir, emulator)?;
            let create_new = if new_card || cards.is_empty() {
                if cards.is_empty() && !new_card {
                    bail!(
                        "no .{extension} files found in {}; use `install --new {}` to create a new card",
                        dir.display(),
                        source
                    );
                }
                true
            } else {
                false
            };

            let target = if create_new {
                let target = new_card_path(Path::new(&source), &dir, emulator)?;
                let stem = target
                    .file_stem()
                    .expect("new-card path has a stem")
                    .to_string_lossy();
                let parsed = japs1me::install::parse_card_file_name(&stem)?;
                if is_multi_disc(&parsed.game_region) {
                    warn_multi_disc(&parsed.game_region, emulator);
                }
                if target.symlink_metadata().is_ok() {
                    eprintln!("New-card target already exists; it will be backed up if confirmed");
                }
                target
            } else {
                let target: PathBuf = match find_match_by_name(Path::new(&source), &cards) {
                    Some(matched) => {
                        eprintln!(
                            "Matched {} by game basename; skipping memory-card picker",
                            matched.display()
                        );
                        matched
                    }
                    None => {
                        for (index, card) in cards.iter().enumerate() {
                            let name = card
                                .file_name()
                                .map(|name| name.to_string_lossy())
                                .unwrap_or_else(|| card.as_os_str().to_string_lossy());
                            eprintln!("  {}: {name}", index + 1);
                        }
                        require_terminal("cannot choose a memory card without a terminal")?;
                        let choice = prompt_line(
                            "Which memory card to overwrite? (number, or n for a new card) ",
                        )?
                        .ok_or_else(|| anyhow!("no selection given; nothing changed"))?;
                        if matches!(choice.trim().to_ascii_lowercase().as_str(), "n" | "new") {
                            let target = new_card_path(Path::new(&source), &dir, emulator)?;
                            let stem = target
                                .file_stem()
                                .expect("new-card path has a stem")
                                .to_string_lossy();
                            let parsed = japs1me::install::parse_card_file_name(&stem)?;
                            if is_multi_disc(&parsed.game_region) {
                                warn_multi_disc(&parsed.game_region, emulator);
                            }
                            return install_new_card(
                                &source,
                                &target,
                                &source_bytes,
                                &source_summary,
                            );
                        }
                        let index = parse_selection(&choice, cards.len())
                            .map_err(|error| anyhow!("{error}; nothing changed"))?;
                        cards[index].clone()
                    }
                };
                target
            };

            if create_new {
                return install_new_card(&source, &target, &source_bytes, &source_summary);
            }
            if same_file(&PathBuf::from(&source), &target) {
                bail!(
                    "source and target are the same file ({}); nothing changed",
                    target.display()
                );
            }
            require_terminal("cannot confirm install without a terminal")?;
            if !prompt_yes(&format!(
                "Overwrite {} with {}? (y/n) ",
                target.display(),
                Path::new(&source).display()
            ))? {
                bail!("install cancelled; '{}' unchanged", target.display());
            }
            let backup = backup_path(&target);
            backup_file_atomically(&target, &backup)?;
            replace_file_atomically(&target.to_string_lossy(), &source_bytes)?;
            let check =
                std::fs::read(&target).with_context(|| format!("reading {}", target.display()))?;
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

fn warn_multi_disc(game_region: &str, emulator: japs1me::install::Emulator) {
    match emulator {
        japs1me::install::Emulator::RetroArch => eprintln!(
            "warning: '{game_region}' looks like a multi-disc title; RetroArch uses one .srm per disc, so when you reach the next disc, manually copy this card and rename it for that disc."
        ),
        japs1me::install::Emulator::DuckStation => eprintln!(
            "warning: '{game_region}' looks like a multi-disc title; DuckStation shares one .mcd across all discs, so the disc suffix in this name is not required, and no manual copy is needed between discs."
        ),
    }
}

fn install_new_card(
    source: &str,
    target: &Path,
    source_bytes: &[u8],
    source_summary: &japs1me::CardSummary,
) -> Result<()> {
    if same_file(Path::new(source), target) {
        bail!(
            "source and destination are the same file ({}); nothing changed",
            target.display()
        );
    }

    let target_exists = match target.symlink_metadata() {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error).with_context(|| format!("checking {}", target.display())),
    };
    require_terminal("cannot confirm install without a terminal")?;
    if !prompt_yes(&format!(
        "{} {} from {}? (y/n) ",
        if target_exists { "Overwrite" } else { "Create" },
        target.display(),
        Path::new(source).display()
    ))? {
        bail!("install cancelled; '{}' unchanged", target.display());
    }

    let backup = if target_exists {
        let backup = backup_path(target);
        backup_file_atomically(target, &backup)?;
        Some(backup)
    } else {
        None
    };

    if target_exists {
        replace_file_atomically(&target.to_string_lossy(), source_bytes)?;
    } else {
        write_new_file_atomically(target, source_bytes)?;
    }
    let check = std::fs::read(target).with_context(|| format!("reading {}", target.display()))?;
    parse_raw(&check).with_context(|| format!("verifying {}", target.display()))?;

    match backup {
        Some(backup) => eprintln!(
            "Installed {} as {} (backup: {})",
            Path::new(source).display(),
            target.display(),
            backup.display()
        ),
        None => eprintln!(
            "Created new card {} from {} ({} save(s))",
            target.display(),
            Path::new(source).display(),
            source_summary.saves.len()
        ),
    }
    Ok(())
}

/// Return the configured saves directory, prompting and saving it when needed.
fn ensure_saved_dir() -> Result<PathBuf> {
    if let Some(dir) = load_saved_dir()?.filter(|dir| dir.is_dir()) {
        return Ok(dir);
    }

    require_terminal("no emulator saves directory configured; run interactively to set one")?;
    let answer = prompt_line("Enter your RetroArch or DuckStation PS1 memory-card directory: ")?
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
    Ok(dir)
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

/// True when both paths resolve to the same file on disk.
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Cards whose file name starts with `prefix` (case-sensitive), in order.
fn copy_prefix_matches(prefix: &str, cards: &[PathBuf]) -> Vec<PathBuf> {
    cards
        .iter()
        .filter(|card| {
            card.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        })
        .cloned()
        .collect()
}

/// Resolve a copy destination, using the source basename when `dest` is a directory.
fn resolve_copy_destination(dest: &str, source: &Path) -> Result<PathBuf> {
    let dest_path = Path::new(dest);
    if dest == "." || dest_path.is_dir() {
        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow!("copy source has no file name: {}", source.display()))?;
        Ok(dest_path.join(file_name))
    } else {
        Ok(dest_path.to_path_buf())
    }
}

/// Resolve a copy source, falling back to the configured emulator saves dir.
///
/// A source that exists as given or contains a path separator is used
/// literally. Otherwise a bare name first tries an exact file-name match,
/// then an exact stem match (so `game.srm` can find `game.mcd`), then a prefix
/// match: none errors out, one is used,
/// and several offer a numbered picker.
fn resolve_copy_source(source: &str) -> Result<PathBuf> {
    let literal = Path::new(source);
    if literal.symlink_metadata().is_ok() || source.contains('/') || source.contains('\\') {
        return Ok(literal.to_path_buf());
    }
    let saved = load_saved_dir()?;
    let Some(dir) = saved.filter(|dir| dir.is_dir()) else {
        return Ok(resolve_in_saves_dir(source, None));
    };
    resolve_copy_source_in_dir(source, &dir)
}

fn resolve_copy_source_in_dir(source: &str, dir: &Path) -> Result<PathBuf> {
    let emulator = detect_emulator(dir);
    let cards = find_card_files(dir, emulator)?;
    let requested = Path::new(source);
    if let Some(exact) = cards
        .iter()
        .find(|card| card.file_name() == requested.file_name())
    {
        return Ok(exact.clone());
    }
    if let Some(stem_match) = find_match_by_name(requested, &cards) {
        return Ok(stem_match);
    }
    let matches = copy_prefix_matches(source, &cards);
    match matches.len() {
        0 => bail!(
            "no .{} files in {} starting with '{source}'",
            emulator.card_extension(),
            dir.display()
        ),
        1 => {
            eprintln!("Matched {} by prefix", matches[0].display());
            Ok(matches[0].clone())
        }
        _ => {
            for (index, card) in matches.iter().enumerate() {
                let name = card
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_else(|| card.as_os_str().to_string_lossy());
                eprintln!("  {}: {name}", index + 1);
            }
            require_terminal("cannot choose a save without a terminal")?;
            let choice = prompt_line("Which save to copy? (number) ")?
                .ok_or_else(|| anyhow!("no selection given; nothing changed"))?;
            let index = parse_selection(&choice, matches.len())
                .map_err(|error| anyhow!("{error}; nothing changed"))?;
            Ok(matches[index].clone())
        }
    }
}

/// Load a raw card image from either `.gme` (stripped) or raw (`.srm`/`.mcd`/`.mcr`) bytes.
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
        "unsupported card format for '{name}': expected a .gme file ({GME_FILE_LEN} bytes) or a raw memory-card image (.srm/.mcd; {RAW_CARD_LEN} bytes starting with MC)"
    )
}

fn write_output_atomically(output: &str, data: &[u8]) -> Result<()> {
    let output = PathBuf::from(output);
    if output.symlink_metadata().is_ok() {
        bail!("output already exists: {}", output.display());
    }
    write_temp_then_persist(&output, data, "creating temporary output file")
}

/// Create a new file via a staged temporary file without replacing a target
/// that may have appeared after the caller checked for existence.
fn write_new_file_atomically(output: &Path, data: &[u8]) -> Result<()> {
    if output.symlink_metadata().is_ok() {
        bail!("output already exists: {}", output.display());
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let mut temp = tempfile::NamedTempFile::new_in(parent.unwrap_or_else(|| Path::new(".")))
        .context("creating temporary card file")?;
    use std::io::Write;
    temp.write_all(data)
        .context("writing temporary card file")?;
    temp.persist_noclobber(output)
        .map_err(|error| error.error)
        .with_context(|| format!("creating {} without overwriting", output.display()))?;
    Ok(())
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

/// Back up a card, atomically replacing a prior regular-file backup if present.
fn backup_file_atomically(target: &Path, backup: &Path) -> Result<()> {
    match backup.symlink_metadata() {
        Ok(metadata) if !metadata.file_type().is_file() => {
            bail!(
                "backup path exists and is not a regular file: {}",
                backup.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("checking {}", backup.display())),
    }

    let contents = std::fs::read(target)
        .with_context(|| format!("reading {} for backup", target.display()))?;
    write_temp_then_persist(backup, &contents, "creating temporary backup file")
        .with_context(|| format!("backing up {} to {}", target.display(), backup.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|name| PathBuf::from(format!("/saves/{name}")))
            .collect()
    }

    #[test]
    fn prefix_match_finds_single_and_multiple() {
        let all = cards(&["a.srm", "ab.srm", "b.srm"]);
        assert_eq!(
            copy_prefix_matches("ab", &all),
            vec![PathBuf::from("/saves/ab.srm")]
        );
        assert_eq!(copy_prefix_matches("a", &all).len(), 2);
        assert!(copy_prefix_matches("z", &all).is_empty());
    }

    #[test]
    fn prefix_match_is_case_sensitive() {
        let all = cards(&["game.srm"]);
        assert!(copy_prefix_matches("GAME", &all).is_empty());
        assert_eq!(copy_prefix_matches("game", &all).len(), 1);
    }

    #[test]
    fn copy_source_matches_stem_across_emulator_extensions() {
        let cards = cards(&["game.mcd"]);
        assert_eq!(
            find_match_by_name(Path::new("game.srm"), &cards),
            Some(PathBuf::from("/saves/game.mcd"))
        );
        assert_eq!(
            find_match_by_name(Path::new("game"), &cards),
            Some(PathBuf::from("/saves/game.mcd"))
        );
    }

    #[test]
    fn copy_source_resolution_finds_duckstation_card_by_workspace_name() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("duckstation").join("memcards");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let card = dir.join("game.mcd");
        std::fs::write(&card, [0u8; 4]).expect("write card");

        assert_eq!(
            resolve_copy_source_in_dir("game.srm", &dir).expect("resolves"),
            card
        );
    }

    #[test]
    fn cli_install_accepts_new_flag() {
        assert!(Cli::try_parse_from(["japs1me", "install", "--new", "Game (USA).srm"]).is_ok());
        assert!(Cli::try_parse_from(["japs1me", "install", "-n", "Game (USA).mcd"]).is_ok());
        assert!(Cli::try_parse_from(["japs1me", "install", "Game (USA).srm"]).is_ok());
    }

    #[test]
    fn cli_list_accepts_one_or_more_cards() {
        assert!(Cli::try_parse_from(["japs1me", "list", "one.srm"]).is_ok());
        assert!(Cli::try_parse_from(["japs1me", "list", "one.srm", "two.gme"]).is_ok());
        assert!(Cli::try_parse_from(["japs1me", "list"]).is_err());
    }

    #[test]
    fn cli_setup_takes_no_arguments() {
        assert!(Cli::try_parse_from(["japs1me", "setup"]).is_ok());
        assert!(Cli::try_parse_from(["japs1me", "setup", "extra"]).is_err());
    }

    #[test]
    fn copy_destination_directory_uses_source_file_name() {
        let source = Path::new("/saves/game.srm");
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            resolve_copy_destination(&dir.path().to_string_lossy(), source).expect("resolves"),
            dir.path().join("game.srm")
        );
        assert_eq!(
            resolve_copy_destination(".", source).expect("resolves"),
            PathBuf::from("./game.srm")
        );
    }

    #[test]
    fn copy_destination_file_path_stays_exact() {
        let source = Path::new("/saves/game.srm");
        assert_eq!(
            resolve_copy_destination("/tmp/custom-name.srm", source).expect("resolves"),
            PathBuf::from("/tmp/custom-name.srm")
        );
    }

    #[test]
    fn existing_backup_is_replaced_with_current_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("game.srm");
        let backup = backup_path(&target);
        std::fs::write(&target, b"new previous card").expect("write target");
        std::fs::write(&backup, b"older backup").expect("write old backup");

        backup_file_atomically(&target, &backup).expect("replaces backup");

        assert_eq!(
            std::fs::read(&backup).expect("read backup"),
            b"new previous card"
        );
        assert_eq!(
            std::fs::read(&target).expect("read target"),
            b"new previous card"
        );
    }

    #[test]
    fn new_card_write_refuses_to_replace_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("game.srm");
        std::fs::write(&target, b"existing").expect("write existing");
        assert!(write_new_file_atomically(&target, b"new").is_err());
        assert_eq!(std::fs::read(&target).expect("read target"), b"existing");
    }
}
