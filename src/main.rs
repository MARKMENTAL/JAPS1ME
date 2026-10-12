use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use japs1me::gme::{parse_gme, GME_FILE_LEN, GME_HEADER_LEN, RAW_CARD_LEN};
use japs1me::{format_save_table, parse_raw};
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
    }
    Ok(())
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
    use std::io::Write;
    let output = PathBuf::from(output);
    if output.symlink_metadata().is_ok() {
        bail!("output already exists: {}", output.display());
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let mut temp = tempfile::NamedTempFile::new_in(parent.unwrap_or_else(|| Path::new(".")))
        .context("creating temporary output file")?;
    temp.write_all(data)
        .context("writing temporary output file")?;
    temp.persist(&output)
        .with_context(|| format!("writing {}", output.display()))?;
    Ok(())
}
