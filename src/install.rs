//! Emulator saves-directory configuration and memory-card discovery for `copy`/`install`.
//!
//! The directory is stored as a single line in `./.japs1mesavedir` when
//! present (a per-workspace override), else in `~/.config/japs1mesavedir`
//! (`$XDG_CONFIG_HOME` is honored when set). A local file wins when both
//! exist so a workspace can override the global default.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Parsed game title plus an optional DuckStation card slot suffix.
#[derive(Debug, Eq, PartialEq)]
pub struct CardFileName {
    pub game_region: String,
    pub slot: Option<u32>,
}

/// Emulator-specific raw memory-card filename conventions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Emulator {
    RetroArch,
    DuckStation,
}

impl Emulator {
    /// File extension used for this emulator's card images, without a dot.
    pub fn card_extension(self) -> &'static str {
        match self {
            Self::RetroArch => "srm",
            Self::DuckStation => "mcd",
        }
    }

    /// Display name for this emulator.
    pub fn name(self) -> &'static str {
        match self {
            Self::RetroArch => "RetroArch",
            Self::DuckStation => "DuckStation",
        }
    }

    /// Render a convention-named card filename for this emulator.
    pub fn new_card_file_name(self, game_region: &str, source_slot: Option<u32>) -> String {
        match self {
            Self::RetroArch => format!("{game_region}.srm"),
            Self::DuckStation => format!("{game_region}_{}.mcd", source_slot.unwrap_or(1)),
        }
    }
}

/// Parse `GAMENAME (REGIONCODE)[_SLOT]` from a filename stem.
///
/// The final parenthesized group identifies the region, allowing earlier
/// parenthesized suffixes such as `(Disc 1)`. Slot numbers are DuckStation's
/// optional `_N` suffix and must be positive.
pub fn parse_card_file_name(stem: &str) -> Result<CardFileName> {
    let (game_region, slot) = match stem.rsplit_once('_') {
        Some((base, suffix))
            if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let slot = suffix
                .parse::<u32>()
                .with_context(|| format!("invalid memory-card slot suffix '_{suffix}'"))?;
            if slot == 0 {
                bail!("memory-card slot number must be greater than zero");
            }
            (base, Some(slot))
        }
        _ => (stem, None),
    };

    let Some(open) = game_region.rfind(" (") else {
        bail!("filename must use 'GAMENAME (REGIONCODE)' convention");
    };
    let region = &game_region[open + 2..];
    if !game_region[..open].trim().is_empty()
        && region.ends_with(')')
        && !region[..region.len() - 1].trim().is_empty()
    {
        Ok(CardFileName {
            game_region: game_region.to_owned(),
            slot,
        })
    } else {
        bail!("filename must use 'GAMENAME (REGIONCODE)' convention");
    }
}

/// Build the configured emulator's target path from a source filename.
pub fn new_card_path(source: &Path, dir: &Path, emulator: Emulator) -> Result<PathBuf> {
    let stem = source
        .file_stem()
        .ok_or_else(|| anyhow::anyhow!("source has no file name: {}", source.display()))?
        .to_string_lossy();
    let parsed = parse_card_file_name(&stem).with_context(|| {
        format!(
            "cannot create a new card from '{}'; rename it to 'GAMENAME (REGIONCODE).srm' or 'GAMENAME (REGIONCODE)_SLOT.mcd'",
            source.display()
        )
    })?;
    Ok(dir.join(emulator.new_card_file_name(&parsed.game_region, parsed.slot)))
}

/// Detect conventional multi-disc labels without matching unrelated words like `Discovery`.
pub fn is_multi_disc(game_region: &str) -> bool {
    let lower = game_region.to_ascii_lowercase();
    for token in ["disc", "disk"] {
        let mut rest = lower.as_str();
        while let Some(index) = rest.find(token) {
            let before = rest[..index].chars().next_back();
            let suffix = &rest[index + token.len()..];
            let after_token = suffix.trim_start();
            let has_number = after_token
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit());
            let token_boundary = before.is_none_or(|ch| !ch.is_ascii_alphabetic());
            if token_boundary && has_number {
                return true;
            }
            let Some(next) = suffix.chars().next() else {
                break;
            };
            rest = &suffix[next.len_utf8()..];
            if rest.is_empty() {
                break;
            }
        }
    }
    false
}

/// Infer the emulator from conventional saves-directory path components.
///
/// DuckStation uses a `memcards` directory or a `duckstation` component; all
/// other configured paths are treated as RetroArch.
pub fn detect_emulator(dir: &Path) -> Emulator {
    if dir.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("memcards")
            || component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case("duckstation")
    }) {
        Emulator::DuckStation
    } else {
        Emulator::RetroArch
    }
}

/// Config file name inside the user's config directory.
pub const CONFIG_FILE_NAME: &str = "japs1mesavedir";
/// Config file name for a per-workspace override in the invocation directory.
pub const LOCAL_CONFIG_FILE_NAME: &str = ".japs1mesavedir";

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

/// Parse one saved-directory file's contents; blank counts as missing.
fn parse_saved_dir_contents(contents: &str) -> Option<PathBuf> {
    let line = contents.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }
    Some(PathBuf::from(line))
}

/// Read a saved directory from an explicit file path.
///
/// Returns `Ok(None)` when the file is missing or blank.
fn load_saved_dir_from(path: &Path) -> Result<Option<PathBuf>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    Ok(parse_saved_dir_contents(&contents))
}

/// Load the saved emulator saves directory, if configured.
///
/// Returns `Ok(None)` when the config file is missing or blank.
fn load_saved_dir_in(config_dir: &Path) -> Result<Option<PathBuf>> {
    load_saved_dir_from(&config_path_in(config_dir))
}

/// Local override path for an explicit base directory (testable core).
fn local_config_path_in(base: &Path) -> PathBuf {
    base.join(LOCAL_CONFIG_FILE_NAME)
}

/// Load the workspace-local override from an explicit base directory.
fn load_local_saved_dir_in(base: &Path) -> Result<Option<PathBuf>> {
    load_saved_dir_from(&local_config_path_in(base))
}

/// Load the workspace-local override from the invocation directory.
///
/// Missing/unreadable working directories count as missing.
fn load_local_saved_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    load_local_saved_dir_in(&cwd).ok()?
}

/// Prefer the workspace-local override; the global default wins only when
/// no local file is configured.
fn pick_saved_dir(local: Option<PathBuf>, user: Option<PathBuf>) -> Option<PathBuf> {
    local.or(user)
}

/// Load the saved emulator saves directory for this user.
///
/// `./.japs1mesavedir` wins over the user config so a workspace can override
/// the default. Unresolvable user config directories (e.g. `HOME` unset on
/// Windows) fall through to the local file instead of erroring.
pub fn load_saved_dir() -> Result<Option<PathBuf>> {
    let local = load_local_saved_dir();
    let user = match resolve_config_dir() {
        Ok(dir) => load_saved_dir_in(&dir)?,
        Err(_) => None,
    };
    Ok(pick_saved_dir(local, user))
}

/// Persist the emulator saves directory, creating the config directory.
fn save_dir_config_in(config_dir: &Path, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("creating {}", config_dir.display()))?;
    let path = config_path_in(config_dir);
    std::fs::write(&path, format!("{}\n", dir.display()))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Persist the emulator saves directory for this user.
///
/// Falls back to `./.japs1mesavedir` when no user config directory resolves
/// (e.g. `HOME`/`XDG_CONFIG_HOME` unset on Windows).
pub fn save_dir_config(dir: &Path) -> Result<PathBuf> {
    match resolve_config_dir() {
        Ok(config_dir) => save_dir_config_in(&config_dir, dir),
        Err(_) => save_dir_config_local_in(
            &std::env::current_dir().context("no working directory")?,
            dir,
        ),
    }
}

/// Persist the saves directory as a workspace-local override (testable core).
fn save_dir_config_local_in(base: &Path, dir: &Path) -> Result<PathBuf> {
    let path = local_config_path_in(base);
    std::fs::write(&path, format!("{}\n", dir.display()))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Find card files for `emulator` (case-insensitive) directly inside `dir`, sorted by name.
pub fn find_card_files(dir: &Path, emulator: Emulator) -> Result<Vec<PathBuf>> {
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
            .is_some_and(|ext| ext.eq_ignore_ascii_case(emulator.card_extension()))
        {
            cards.push(path);
        }
    }
    cards.sort();
    Ok(cards)
}

/// Find the card whose stem exactly matches the source file stem.
///
/// Comparing stems permits a workspace `.srm` card to match a DuckStation
/// `.mcd` card while remaining case-sensitive. At most one entry can match.
pub fn find_match_by_name(source: &Path, cards: &[PathBuf]) -> Option<PathBuf> {
    let source_stem = source.file_stem()?;
    cards
        .iter()
        .find(|card| card.file_stem().is_some_and(|stem| stem == source_stem))
        .cloned()
}

/// Resolve a copy source: bare file names fall back to the saves directory.
///
/// A source that exists as given or contains a path separator is used
/// literally so files in the current directory are never shadowed. Otherwise,
/// when a saves directory is configured, the bare name is joined onto it.
pub fn resolve_in_saves_dir(source: &str, saved_dir: Option<&Path>) -> PathBuf {
    let path = Path::new(source);
    let has_separator = source.contains('/') || source.contains('\\');
    if has_separator || path.symlink_metadata().is_ok() {
        return path.to_path_buf();
    }
    match saved_dir {
        Some(dir) => dir.join(path),
        None => path.to_path_buf(),
    }
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

/// Backup path for an install target (`<name>.<ext>.japs1me-bak`).
pub fn backup_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".japs1me-bak");
    target.with_file_name(name)
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
    fn finds_emulator_cards_sorted_and_by_extension() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("b.srm"), [0u8; 4]).expect("write");
        std::fs::write(dir.path().join("a.srm"), [0u8; 4]).expect("write");
        std::fs::write(dir.path().join("C.SRM"), [0u8; 4]).expect("write");
        std::fs::write(dir.path().join("duck.mcd"), [0u8; 4]).expect("write");
        std::fs::write(dir.path().join("notes.txt"), [0u8; 4]).expect("write");
        std::fs::create_dir(dir.path().join("d.srm")).expect("mkdir");
        let cards = find_card_files(dir.path(), Emulator::RetroArch).expect("scans");
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
        let duckstation_cards = find_card_files(dir.path(), Emulator::DuckStation).expect("scans");
        assert_eq!(duckstation_cards, vec![dir.path().join("duck.mcd")]);
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
        assert_eq!(
            backup_path(Path::new("/saves/game.mcd")),
            PathBuf::from("/saves/game.mcd.japs1me-bak")
        );
    }

    #[test]
    fn match_by_name_finds_exact_stem_across_extensions() {
        let cards = vec![
            PathBuf::from("/saves/a.srm"),
            PathBuf::from("/saves/game.mcd"),
            PathBuf::from("/saves/b.srm"),
        ];
        assert_eq!(
            find_match_by_name(Path::new("/tmp/game.srm"), &cards),
            Some(PathBuf::from("/saves/game.mcd"))
        );
    }

    #[test]
    fn match_by_name_misses_and_is_case_sensitive() {
        let cards = vec![PathBuf::from("/saves/game.mcd")];
        assert_eq!(
            find_match_by_name(Path::new("/tmp/other.srm"), &cards),
            None
        );
        assert_eq!(find_match_by_name(Path::new("/tmp/GAME.SRM"), &cards), None);
        assert_eq!(find_match_by_name(Path::new("/tmp/game.srm"), &[]), None);
    }

    #[test]
    fn copy_source_bare_name_falls_back_to_saves_dir() {
        assert_eq!(
            resolve_in_saves_dir("game.srm", Some(Path::new("/saves"))),
            PathBuf::from("/saves/game.srm")
        );
        assert_eq!(
            resolve_in_saves_dir("game.srm", None),
            PathBuf::from("game.srm")
        );
    }

    #[test]
    fn copy_source_with_separator_stays_literal() {
        assert_eq!(
            resolve_in_saves_dir("sub/game.srm", Some(Path::new("/saves"))),
            PathBuf::from("sub/game.srm")
        );
        assert_eq!(
            resolve_in_saves_dir("/abs/game.srm", Some(Path::new("/saves"))),
            PathBuf::from("/abs/game.srm")
        );
    }

    #[test]
    fn copy_source_absolute_path_stays_literal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let abs_file = dir.path().join("local.srm");
        std::fs::write(&abs_file, [0u8; 1]).expect("write");
        let saved = tempfile::tempdir().expect("tempdir");
        let abs = abs_file.to_string_lossy().into_owned();
        assert_eq!(
            resolve_in_saves_dir(&abs, Some(saved.path())),
            PathBuf::from(&abs)
        );
    }

    #[test]
    fn local_override_loads_and_blank_counts_as_missing() {
        let base = tempfile::tempdir().expect("tempdir");
        assert_eq!(load_local_saved_dir_in(base.path()).expect("loads"), None);
        std::fs::write(local_config_path_in(base.path()), "/saves/local\n").expect("write");
        assert_eq!(
            load_local_saved_dir_in(base.path()).expect("loads"),
            Some(PathBuf::from("/saves/local"))
        );
        std::fs::write(local_config_path_in(base.path()), "\n").expect("write");
        assert_eq!(load_local_saved_dir_in(base.path()).expect("loads"), None);
    }

    #[test]
    fn local_override_wins_over_user_config() {
        assert_eq!(
            pick_saved_dir(
                Some(PathBuf::from("/saves/local")),
                Some(PathBuf::from("/saves/user"))
            ),
            Some(PathBuf::from("/saves/local"))
        );
        assert_eq!(
            pick_saved_dir(None, Some(PathBuf::from("/saves/user"))),
            Some(PathBuf::from("/saves/user"))
        );
        assert_eq!(pick_saved_dir(None, None), None);
    }

    #[test]
    fn local_save_roundtrip() {
        let base = tempfile::tempdir().expect("tempdir");
        let saved = save_dir_config_local_in(base.path(), Path::new("/saves/ws")).expect("saves");
        assert_eq!(saved, local_config_path_in(base.path()));
        assert_eq!(
            load_local_saved_dir_in(base.path()).expect("loads"),
            Some(PathBuf::from("/saves/ws"))
        );
    }

    #[test]
    fn detects_emulator_from_directory_components() {
        assert_eq!(
            detect_emulator(Path::new("/home/user/.local/share/duckstation/memcards")),
            Emulator::DuckStation
        );
        assert_eq!(
            detect_emulator(Path::new(
                "/home/user/.local/share/Steam/steamapps/common/RetroArch/saves/Beetle PSX"
            )),
            Emulator::RetroArch
        );
        assert_eq!(
            detect_emulator(Path::new("/home/user/DuckStation/MemCards")),
            Emulator::DuckStation
        );
        assert_eq!(
            detect_emulator(Path::new("/home/user/ps1-saves")),
            Emulator::RetroArch
        );
    }

    #[test]
    fn parses_convention_names_and_converts_extensions() {
        let source = parse_card_file_name("Crash Bandicoot (USA)").expect("parses");
        assert_eq!(
            source,
            CardFileName {
                game_region: "Crash Bandicoot (USA)".to_owned(),
                slot: None,
            }
        );
        assert_eq!(
            Emulator::DuckStation.new_card_file_name(&source.game_region, source.slot),
            "Crash Bandicoot (USA)_1.mcd"
        );
        let duckstation = parse_card_file_name("Final Fantasy VII (Disc 1) (USA)_2")
            .expect("parses disc title with slot");
        assert_eq!(duckstation.slot, Some(2));
        assert_eq!(
            Emulator::RetroArch.new_card_file_name(&duckstation.game_region, duckstation.slot),
            "Final Fantasy VII (Disc 1) (USA).srm"
        );
        assert_eq!(
            Emulator::DuckStation.new_card_file_name(&duckstation.game_region, duckstation.slot),
            "Final Fantasy VII (Disc 1) (USA)_2.mcd"
        );
    }

    #[test]
    fn rejects_invalid_convention_and_zero_slot() {
        assert!(parse_card_file_name("mysaves").is_err());
        assert!(parse_card_file_name(" (USA)").is_err());
        assert!(parse_card_file_name("Game ()").is_err());
        assert!(parse_card_file_name("Game (USA)_0").is_err());
        assert!(parse_card_file_name("Game (USA)_999999999999999999999").is_err());
    }

    #[test]
    fn detects_multi_disc_names_without_matching_discovery() {
        assert!(is_multi_disc("Game (Disc 1) (USA)"));
        assert!(is_multi_disc("Game [Disc2] (USA)"));
        assert!(is_multi_disc("Game Disk 3 (USA)"));
        assert!(!is_multi_disc("Discovery (USA)"));
        assert!(!is_multi_disc("Game (USA)"));
    }

    #[test]
    fn new_card_path_uses_configured_emulator_extension() {
        let dir = Path::new("/saves");
        assert_eq!(
            new_card_path(
                Path::new("Crash Bandicoot (USA).srm"),
                dir,
                Emulator::DuckStation
            )
            .expect("path"),
            PathBuf::from("/saves/Crash Bandicoot (USA)_1.mcd")
        );
        assert_eq!(
            new_card_path(
                Path::new("Crash Bandicoot (USA)_2.mcd"),
                dir,
                Emulator::RetroArch
            )
            .expect("path"),
            PathBuf::from("/saves/Crash Bandicoot (USA).srm")
        );
    }
}
