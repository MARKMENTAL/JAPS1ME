# JAPS1ME

**JAPS1ME - PS1 RetroArch Save Converter** is a command-line tool for converting
DexDrive PlayStation 1 memory-card images (`.gme`) to RetroArch memory-card
format (`.srm`). It also lists the saves on `.gme` and raw (`.srm`/`.mcr`)
card images.

## Features

- Convert DexDrive `.gme` files (including headerless/zeroed-header variants) to RetroArch `.srm` with validated stripping.
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

Examples:

```sh
target/release/japs1me list save-games/pro-evolution-soccer-2.22507.gme
target/release/japs1me convert save-games/pro-evolution-soccer-2.22507.gme pes2.srm
```

### `list`

- Accepts `.gme` files and raw images (`.srm`, `.mcr`, …) detected by size and `MC` signature.
- Shows each save's 1-based slot, save name, game title (Shift-JIS decoded, falling back to the save name), region, block count, payload size, and active/deleted status.
- The footer reports free slots out of 15. Directory checksum mismatches and orphaned links are reported as warnings on stderr.

### `convert`

- Accepts DexDrive `.gme` input only (134976 bytes with an `MC` signature at offset 3904). Files with a missing/zeroed header are accepted with a warning.
- Writes the raw 131072-byte payload verbatim via a temporary file; existing output files are not overwritten.

## Safety

- Back up saves before converting.
- `convert` refuses to overwrite; delete or rename a previous `.srm` first if you intend to replace it.
- Avoid testing against your only copy of a save; the fixture in `save-games/` is read-only test input.

## Scope compared with MemcardRex

JAPS1ME focuses on `.gme` → `.srm` conversion and save listing rather than general memory-card editing:

- `list` covers saves and free slots; there is no per-frame browser.
- `convert` handles DexDrive `.gme` only; there is no VGS/VMP/MCX support and no reverse conversion.
- There is no GUI, save import/export of single saves (`.mcs`/`.psx`), deletion, formatting, comment editing, or hardware (DexDrive/serial) support. JAPS1ME validates directory checksums read-only and never recalculates them.

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
