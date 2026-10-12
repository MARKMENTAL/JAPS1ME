//! RetroArch saves-directory configuration and `.srm` discovery for `install`.
//!
//! The directory is stored as a single line in `~/.config/japs1mesavedir`
//! (`$XDG_CONFIG_HOME` is honored when set).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Config file name inside the user's config directory.
pub const CONFIG_FILE_NAME: &str = "japs1mesavedir";

/// Resolve the user config directory (`$XDG_CONFIG_HOME`, else `~/.config`).
fn resolve_config_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config"))
}

/// Config file path for an explicit config directory (testable core).
fn config_path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(CONFIG_FILE_NAME)
}

/// Config file path for this user.
pub fn config_path() -> Result<PathBuf> {
    Ok(config_path_in(&resolve_config_dir()?))
}

/// Load the saved RetroArch saves directory, if configured.
///
/// Returns `Ok(None)` when the config file is missing or blank.
fn load_saved_dir_in(config_dir: &Path) -> Result<Option<PathBuf>> {
    let path = config_path_in(config_dir);
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    let line = contents.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(line)))
}

/// Load the saved RetroArch saves directory for this user.
pub fn load_saved_dir() -> Result<Option<PathBuf>> {
    load_saved_dir_in(&resolve_config_dir()?)
}

/// Persist the RetroArch saves directory, creating the config directory.
fn save_dir_config_in(config_dir: &Path, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("creating {}", config_dir.display()))?;
    let path = config_path_in(config_dir);
    std::fs::write(&path, format!("{}\n", dir.display()))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Persist the RetroArch saves directory for this user.
pub fn save_dir_config(dir: &Path) -> Result<PathBuf> {
    save_dir_config_in(&resolve_config_dir()?, dir)
}

/// Find `.srm` files (case-insensitive) directly inside `dir`, sorted by name.
pub fn find_srm_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut cards = Vec::new();
    let entries = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("reading {}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("srm"))
        {
            cards.push(path);
        }
    }
    cards.sort();
    Ok(cards)
}

/// Parse a 1-based numbered selection into a 0-based index.
pub fn parse_selection(input: &str, count: usize) -> Result<usize> {
    let number: usize = input
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid selection '{input}'"))?;
    if number == 0 || number > count {
        bail!("invalid selection '{input}': pick 1..={count}");
    }
    Ok(number - 1)
}

/// Backup path for an install target (`<name>.srm.japs1me-bak`).
pub fn backup_path(target: &Path) -> PathBuf {
    target.with_extension("srm.japs1me-bak")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_parses_one_based() {
        assert_eq!(parse_selection("1", 3).expect("parses"), 0);
        assert_eq!(parse_selection(" 3 ", 3).expect("parses"), 2);
        assert!(parse_selection("0", 3).is_err());
        assert!(parse_selection("4", 3).is_err());
        assert!(parse_selection("abc", 3).is_err());
        assert!(parse_selection("", 3).is_err());
    }

    #[test]
    fn finds_srm_files_sorted() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("b.srm"), [0u8; 4]).expect("write");
        std::fs::write(dir.path().join("a.srm"), [0u8; 4]).expect("write");
        std::fs::write(dir.path().join("C.SRM"), [0u8; 4]).expect("write");
        std::fs::write(dir.path().join("notes.txt"), [0u8; 4]).expect("write");
        std::fs::create_dir(dir.path().join("d.srm")).expect("mkdir");
        let cards = find_srm_files(dir.path()).expect("scans");
        let names: Vec<_> = cards
            .iter()
            .map(|path| {
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["C.SRM", "a.srm", "b.srm"]);
    }

    #[test]
    fn config_roundtrip() {
        let home = tempfile::tempdir().expect("tempdir");
        let config_dir = home.path().join(".config");
        assert_eq!(load_saved_dir_in(&config_dir).expect("loads"), None);
        let saved =
            save_dir_config_in(&config_dir, Path::new("/saves/Beetle PSX HW")).expect("saves");
        assert_eq!(saved, config_dir.join(CONFIG_FILE_NAME));
        assert_eq!(
            load_saved_dir_in(&config_dir).expect("loads"),
            Some(PathBuf::from("/saves/Beetle PSX HW"))
        );
    }

    #[test]
    fn blank_config_counts_as_missing() {
        let home = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(home.path()).expect("mkdir");
        std::fs::write(home.path().join(CONFIG_FILE_NAME), "\n").expect("write");
        assert_eq!(load_saved_dir_in(home.path()).expect("loads"), None);
    }

    #[test]
    fn backup_path_appends_suffix() {
        let backup = backup_path(Path::new("/saves/game.srm"));
        assert_eq!(backup, PathBuf::from("/saves/game.srm.japs1me-bak"));
    }
}
