# JAPS1ME

**JAPS1ME - Just A PS1 Memory Card Editor** is a command-line tool for managing
PlayStation 1 memory-card saves for RetroArch-style emulators. It converts
DexDrive (`.gme`) card images to RetroArch (`.srm`) format, lists the saves on
`.gme` and raw (`.srm`/`.mcr`) card images, and imports, deletes, and installs
saves and cards.

## Features

- Convert DexDrive `.gme` files (including headerless/zeroed-header variants) to RetroArch `.srm` with validated stripping.
- Import a named save from a `.gme` file into a RetroArch `.srm` card, with duplicate and free-space checks.
- Delete saves from a `.srm` card by slot number or save name.
- Install a `.srm` card into your RetroArch saves directory (remembers the directory, backs up the replaced card).
- Browse saves with slot numbers, save names, Shift-JIS game titles, regions, block counts, payload sizes, and free slots.
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
| `japs1me list <CARD.gme\|CARD.srm>` | List saves and free slots. |
| `japs1me convert <INPUT.gme> <OUTPUT.srm>` | Convert a DexDrive `.gme` file to RetroArch `.srm`. |
| `japs1me import <CARD.srm> <SOURCE.gme> <SAVE-NAME>` | Import a save from a `.gme` file into a `.srm` card. |
| `japs1me delete <CARD.srm> <SLOT-OR-SAVE>` | Delete a save from a `.srm` card by 1-based slot number or save name. |
| `japs1me install <SOURCE.srm>` | Install a `.srm` card into the configured RetroArch saves directory. |

Examples:

```sh
target/release/japs1me list save-games/pro-evolution-soccer-2.22507.gme
target/release/japs1me convert save-games/pro-evolution-soccer-2.22507.gme pes2.srm
target/release/japs1me import mysaves.srm save-games/pro-evolution-soccer-2.22507.gme BESLES-03946PES-OPT
target/release/japs1me delete mysaves.srm BESLES-03946PES-OPT
target/release/japs1me install mysaves.srm
```

### `list`

- Accepts `.gme` files and raw images (`.srm`, `.mcr`, …) detected by size and `MC` signature.
- Shows each save's 1-based slot, save name, game title (Shift-JIS decoded, falling back to the save name), region, block count, payload size, and active/deleted status.
- The footer reports free slots out of 15. Directory checksum mismatches and orphaned links are reported as warnings on stderr.

### `convert`

- Accepts DexDrive `.gme` input only (134976 bytes with an `MC` signature at offset 3904). Files with a missing/zeroed header are accepted with a warning.
- Writes the raw 131072-byte payload verbatim via a temporary file; existing output files are not overwritten.

### `import`

- Copies one save (matched by exact save name, e.g. `BESLES-03946PES-OPT`) from a `.gme` source into a raw `.srm` card, using the first free slots in slot order.
- If the save already exists on the target, asks `Save <name> already exists on <card>, overwrite it with the one from <source>? (y/n)` on interactive terminals. Only `y`/`yes` proceeds; anything else cancels and leaves the card unchanged. The old save is freed first, so replacements with a different block count work.
- When stdin is not a terminal, an existing save is a hard error instead of a prompt.
- Link bytes are relinked to the new slots, the size field is recomputed, and touched directory checksums are regenerated. Identity bytes (region, product code, identifier, title, save data) are preserved verbatim; saves imported from a deleted source slot are normalized to active.
- The target is staged in a temporary file, re-validated, and only then replaced.

### `delete`

- Deletes one save selected by 1-based slot number (the save's initial slot, as shown in `list`) or exact save name. All-digit selections are slot numbers.
- The save's slots are reset to formatted with fresh checksums and zeroed data, freeing them for future imports.
- `.srm` targets only; the updated card is staged, re-validated, and only then replaced.

### `install`

- On first use (no `~/.config/japs1mesavedir`), asks for your RetroArch PS1 core's saves directory and saves it as a single line in that file (`$XDG_CONFIG_HOME` is honored when set).
- Scans the directory for `.srm` files and asks which one to overwrite (numbered, single attempt). A second strict `y`/`yes` confirmation follows; anything else cancels.
- Backs up the existing card to `<name>.srm.japs1me-bak` first (refuses to proceed if that backup already exists), then replaces the target's contents in place so the filename still matches the game's `.cue` basename.
- Prompts require an interactive terminal; piped stdin fails with an error instead.

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
  `.gme` DexDrive archives, so `import` is limited to `.gme` sources; there is
  no single-save (`.mcs`/`.psx`) support.

## Scope compared with MemcardRex

JAPS1ME focuses on `.gme` → `.srm` conversion, save listing, and save
importing rather than general memory-card editing:

- `list` covers saves and free slots; there is no per-frame browser.
- `convert` handles DexDrive `.gme` only; there is no VGS/VMP/MCX support and no reverse conversion.
- `import` copies whole saves from `.gme` into `.srm`; there is no single-save (`.mcs`/`.psx`) support and no import into `.gme`.
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

JAPS1ME is licensed under the **GNU General Public License, version 3 or (at your option) any later version** (SPDX: `GPL-3.0-or-later`). This project license does not automatically apply to the individual save files in `save-games/`; see that directory's README for contribution and attribution guidance.

## Acknowledgements

Thanks to [ShendoXT and the MemcardRex project](https://github.com/ShendoXT/memcardrex), an advanced PlayStation 1 memory-card editor. Its implementation provided a valuable technical reference for understanding the PS1 memory-card and DexDrive formats.
