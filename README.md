# JAPS1ME

**JAPS1ME - Just A PS1 Memory Card Editor** is a command-line tool for managing
PlayStation 1 memory-card saves for RetroArch-style emulators. It converts
DexDrive (`.gme`) card images to RetroArch (`.srm`) format, lists the saves on
`.gme` and raw (`.srm`/`.mcr`) card images, and imports, deletes, and installs
saves and cards.

| Made for | Developed on |
| --- | --- |
| ![Linux](https://img.shields.io/badge/Linux-%23FFF.svg?style=for-the-badge&logo=linux&logoColor=black) | ![Debian](https://img.shields.io/badge/Debian-%23D70A53.svg?style=for-the-badge&logo=debian&logoColor=white) |

## Features

- Convert DexDrive `.gme` files (including headerless/zeroed-header variants) to RetroArch `.srm` with validated stripping.
- Import a named save from a `.gme` file (or a raw card misnamed `.gme`) into a RetroArch `.srm` card, with duplicate and free-space checks.
- Delete saves from a `.srm` card by slot number or save name.
- Copy an emulator `.srm` out of the RetroArch saves directory into this workspace for JAPS1ME work (bare file name or prefix; asks before overwriting).
- Configure the RetroArch saves directory ahead of time with `setup`.
- Install a `.srm` card into your RetroArch saves directory (remembers the directory, backs up the replaced card).
- Browse one or more cards with slot numbers, save names, Shift-JIS game titles, regions, block counts, payload sizes, and free slots.
- Read-only `list`; `convert` validates before writing and never overwrites an existing output file.

## Build

You need a Rust toolchain with Cargo installed.

```sh
cargo build --release
```

The executable is created at `target/release/japs1me`.

## Usage

| Command | Purpose |
| --- | --- |
| `japs1me --version` | Print the version (`-V` is also supported). |
| `japs1me list <CARD.gme\|CARD.srm>...` | List saves and free slots for one or more cards. |
| `japs1me convert <INPUT.gme> <OUTPUT.srm>` | Convert a DexDrive `.gme` file to RetroArch `.srm`. |
| `japs1me import <CARD.srm> <SOURCE.gme> <SAVE-NAME>` | Import a save from a `.gme` (or raw misnamed `.gme`) file into a `.srm` card. |
| `japs1me delete <CARD.srm> <SLOT-OR-SAVE>` | Delete a save from a `.srm` card by 1-based slot number or save name. |
| `japs1me copy <SOURCE.srm> <DEST.srm\|DEST-DIR>` | Copy an emulator `.srm` into the workspace for JAPS1ME editing; bare source names resolve in the saves directory. A directory destination keeps the source filename. |
| `japs1me setup` | Configure the RetroArch saves directory used by `copy` and `install`. |
| `japs1me install <SOURCE.srm>` | Install a `.srm` card into the configured RetroArch saves directory. |

Examples:

```sh
target/release/japs1me list save-games/pro-evolution-soccer-2.22507.gme
target/release/japs1me list save-games/*.gme
target/release/japs1me convert save-games/pro-evolution-soccer-2.22507.gme pes2.srm
target/release/japs1me import mysaves.srm save-games/pro-evolution-soccer-2.22507.gme BESLES-03946PES-OPT
target/release/japs1me delete mysaves.srm BESLES-03946PES-OPT
target/release/japs1me copy "ISS Pro Evolution (USA).srm" /tmp/work.srm
target/release/japs1me copy "ISS Pro Evolution (USA).srm" .
target/release/japs1me setup
target/release/japs1me install mysaves.srm
```

### `list`

- Accepts `.gme` files and raw images (`.srm`, `.mcr`, …) detected by size and `MC` signature.
- Accepts one or more card paths; shell-expanded globs such as `japs1me list *.srm` list each card in its own labeled table.
- Shows each save's 1-based slot, save name, game title (Shift-JIS decoded, falling back to the save name), region, block count, payload size, and active/deleted status.
- The footer reports free slots out of 15. Directory checksum mismatches and orphaned links are reported as warnings on stderr.

### `convert`

- Accepts DexDrive `.gme` input only (134976 bytes with an `MC` signature at offset 3904). Files with a missing/zeroed header are accepted with a warning.
- A raw 131072-byte input is rejected with an “already a raw `.srm` image” error; copy or rename it instead of converting.
- Writes the raw 131072-byte payload verbatim via a temporary file; existing output files are not overwritten.

### `import`

- Copies one save (matched by exact save name, e.g. `BESLES-03946PES-OPT`) from a `.gme` source into a raw `.srm` card, using the first free slots in slot order. A raw 131072-byte source starting with `MC` is also accepted (with a warning) for `.gme` files that are actually misnamed raw images.
- If the save already exists on the target, asks `Save <name> already exists on <card>, overwrite it with the one from <source>? (y/n)` on interactive terminals. Only `y`/`yes` proceeds; anything else cancels and leaves the card unchanged. The old save is freed first, so replacements with a different block count work.
- When stdin is not a terminal, an existing save is a hard error instead of a prompt.
- Link bytes are relinked to the new slots, the size field is recomputed, and touched directory checksums are regenerated. Identity bytes (region, product code, identifier, title, save data) are preserved verbatim; saves imported from a deleted source slot are normalized to active.
- The target is staged in a temporary file, re-validated, and only then replaced.

### `delete`

- Deletes one save selected by 1-based slot number (the save's initial slot, as shown in `list`) or exact save name. All-digit selections are slot numbers.
- The save's slots are reset to formatted with fresh checksums and zeroed data, freeing them for future imports.
- `.srm` targets only; the updated card is staged, re-validated, and only then replaced.

### `copy`

- Pull an emulator save out of the RetroArch saves directory into this workspace copy, so `list`/`import`/`delete` work happens here and the emulator original is never modified (raw `.srm`, 131072 bytes starting with `MC`).
- A bare source file name (no `/`) that does not exist as given resolves inside the configured RetroArch saves directory (`./.japs1mesavedir` wins over the global config when both exist); paths with a separator or that exist literally are used as-is. An exact file-name match wins; otherwise a prefix match applies: no match errors out, one match is used, and several show a numbered picker.
- An existing destination asks `Overwrite <dest> with <source>? (y/n)` on interactive terminals; only `y`/`yes` proceeds. Piped stdin fails instead of overwriting. Copying a file onto itself is refused.
- If the destination is `.` or an existing directory, the resolved source filename is used inside that directory; other destinations are treated as exact file paths.
- The destination is staged in a temporary file and re-validated before it becomes visible.

### `install`

- On first use (no saved directory configured), asks for your RetroArch PS1 core's saves directory and saves it as a single line (`$XDG_CONFIG_HOME` is honored when set; falls back to `./.japs1mesavedir` when no user config directory resolves, e.g. on Windows). A `./.japs1mesavedir` file in the invocation directory overrides the global config when both exist, so a workspace can point elsewhere.
- When the source file name exactly matches a `.srm` in the saves directory, that card is inferred and the numbered picker is skipped. Otherwise scans the directory for `.srm` files and asks which one to overwrite (numbered, single attempt). A second strict `y`/`yes` confirmation follows; anything else cancels.
- Backs up the existing card to `<name>.srm.japs1me-bak` first, replacing an existing regular-file backup with the card currently being overwritten, then replaces the target's contents in place so the filename still matches the game's `.cue` basename.
- Prompts require an interactive terminal; piped stdin fails with an error instead.

### `setup`

- Configure the RetroArch PS1 saves directory used by `copy` and `install` without starting an install.
- Reuses a valid configured directory; if the setting is missing or points to a nonexistent directory, prompts for a valid one and saves it using the same local/global config rules as `install`.

## Safety

- Back up saves before converting.
- `convert` refuses to overwrite; delete or rename a previous `.srm` first if you intend to replace it.
- `import` and `delete` replace the target `.srm` after staging and validation; back it up first and keep RetroArch closed while modifying it.
- `install` backs up the overwritten card automatically, but keep an independent backup of anything irreplaceable.
- Avoid testing against your only copy of a save; the fixture in `save-games/` is read-only test input.

## Design compared with JAPS2ME

JAPS1ME's CLI mirrors JAPS2ME, but the workflow differs because the emulators
manage cards differently:

- PCSX2 and its derivatives share one memory card across many games, so
  JAPS2ME trades in single-save containers (`.cbs`/`.max`) imported into a
  shared card.
- RetroArch and PS1 emulators like PCSXR instead use one memory card per
  game, with the `.srm` basename matching the disc image. Whole-card
  `convert` and `install` are therefore the primary workflow here.
- Importing individual saves out of a shared card image only makes sense from
  `.gme` DexDrive archives (raw images misnamed `.gme` are also accepted as
  sources), so `import` is limited to `.gme`/raw sources; there is
  no single-save (`.mcs`/`.psx`) support.

## Scope compared with MemcardRex

JAPS1ME focuses on `.gme` → `.srm` conversion, save listing, and save
importing rather than general memory-card editing:

- `list` covers saves and free slots; there is no per-frame browser.
- `convert` handles DexDrive `.gme` only; there is no VGS/VMP/MCX support and no reverse conversion.
- `import` copies whole saves from `.gme` (or misnamed raw) into `.srm`; there is no single-save (`.mcs`/`.psx`) support and no import into `.gme`.
- There is no GUI, card formatting, comment editing, or hardware (DexDrive/serial) support. JAPS1ME validates directory checksums read-only and never recalculates them, except when relinking frames during imports and resetting them during deletions.

For details about the card formats, see the [technical format guide](docs/FORMATS.md). The [save-games collection](save-games/README.md) lists the saves shared in this repository.

## Tests

Run formatting, tests, lint checks, and a release build from the repository root:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

## License

Copyright (C) 2026 Mark Robillard Jr (MARKMENTAL).

JAPS1ME is licensed under the **GNU General Public License, version 3 or (at your option) any later version** (SPDX: `GPL-3.0-or-later`). See [LICENSE](LICENSE) for the full text. This project license does not automatically apply to the individual save files in `save-games/`; see that directory's README for contribution and attribution guidance.

## Acknowledgements

Thanks to [ShendoXT and the MemcardRex project](https://github.com/ShendoXT/memcardrex), an advanced PlayStation 1 memory-card editor. Its implementation provided a valuable technical reference for understanding the PS1 memory-card and DexDrive formats.
