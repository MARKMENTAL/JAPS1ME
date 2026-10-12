# Agent and Contributor Guide

## Project overview

JAPS1ME (PS1 RetroArch Save Converter) is a Rust CLI and library for listing
PlayStation 1 memory-card saves and converting DexDrive `.gme` files to
RetroArch `.srm` format. Current CLI commands:

- `list <CARD.gme|CARD.srm>` — list saves as a Unicode table with slot numbers, save names, titles, regions, block counts, payload sizes, and free slots.
- `convert <INPUT.gme> <OUTPUT.srm>` — validate a DexDrive `.gme` file and write its raw payload as `.srm`.
- `import <CARD.srm> <SOURCE.gme> <SAVE-NAME>` — copy a named save from a `.gme` file into a `.srm` card.
- `delete <CARD.srm> <SLOT-OR-SAVE>` — delete a save from a `.srm` card by slot number or save name.
- `install <SOURCE.srm>` — install a `.srm` card into the configured RetroArch saves directory.
- `--version` / `-V` — print the package version and exit.

The current package version is `0.1.0`; keep the root package versions in `Cargo.toml` and `Cargo.lock` in sync.

The project is licensed under GPL-3.0-or-later. `memcardrex` is a technical
reference for PS1 memory-card and DexDrive behavior; see its attribution in
`README.md`. The `JAPS2ME/` and `memcardrex/` directories are reference
material, not part of the build.

## Repository map

- `src/main.rs` — Clap CLI, command dispatch, `.gme`/raw input detection, interactive prompts, and atomic output writes.
- `src/commands/list.rs` — Unicode table rendering and byte-size formatting for `list`.
- `src/gme.rs` — DexDrive `.gme` header parsing, headerless fallback, and comment decoding.
- `src/install.rs` — RetroArch saves-directory config and `.srm` discovery for `install`.
- `src/ps1/card.rs` — raw card parsing, slot/link chains, region names, XOR warnings, save summaries, and slot formatting shared by delete and overwrite imports.
- `src/ps1/delete.rs` — save deletion by slot number or save name from a target raw image.
- `src/ps1/import.rs` — save imports from a source card into a target raw image.
- `src/ps1/title.rs` — Shift-JIS save-title decoding.
- `src/lib.rs` — crate modules and public exports.
- `save-games/README.md` — inventory and contribution guidance for community-shared `.gme` saves in `save-games/`.
- `save-games/` — curated save containers, including the `pro-evolution-soccer-2.22507.gme` fixture used by `include_bytes!` tests; preserve these user-provided files and do not use them as writable integration-test targets.

## Build and checks

Run these from the repository root:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

After changing code, rebuild the executable being invoked. For example, `cargo build` refreshes `target/debug/japs1me`; `cargo build --release` refreshes `target/release/japs1me`. A stale binary can continue exhibiting behavior fixed in the source.

## Memory-card safety

- Never use the user's only save as a test target. Make a temporary copy under `/tmp/opencode/` for integration tests.
- The convert command stages output in a temporary file, refuses to overwrite existing outputs, and only persists after validation.
- The import command stages the updated card in a temporary file, re-validates it, and only then replaces the target.
- The delete command uses the same staged replace flow. The install command backs up the overwritten card before replacing it in place.
- Recommend an independent backup before converting over anything important.

## Card invariants

- A raw card image is exactly 131072 bytes starting with `MC`. A `.gme` file is a 3904-byte header plus the raw image (134976 bytes total).
- Directory frames are 128 bytes at raw offset `128 + slot * 128`; save data blocks are 8192 bytes at raw offset `8192 + slot * 8192`.
- Directory byte 127 is the XOR of bytes `0..127`. JAPS1ME verifies checksums read-only and reports mismatches as warnings; it never recalculates them.
- A save is an initial slot (`0x51`/`0xA1`) plus its link chain via the next-pointer at frame byte 8 (`0xFF` terminates). Links unreachable from any initial slot are orphaned and counted as free, matching MemcardRex.
- Save names are `region + product code + identifier` (frame bytes 10..30, NUL-trimmed). Regions `BA`/`BE`/`BI` display as America/Europe/Japan.
- Save titles are 64-byte Shift-JIS fields at data offset 4, cut at the first even-index NUL; `list` falls back to the save name when no usable title exists.
- `list` summaries count saves in slot order; sizes are `blocks * 8192` payload bytes. Free slots are formatted frames plus orphaned links.

## Change guidelines

- Keep parsing separate from output writing; both input kinds should produce validated raw bytes before any write.
- Prefer focused tests for parser boundaries, link invariants, and refused overwrites.
- Do not weaken length, `MC` signature, or checksum checks to accept malformed input without a documented compatibility reason and a regression test. The headerless-`.gme` fallback is the documented exception.
- Save files in `save-games/` are separate assets, not automatically covered by the project's GPL license. Do not claim redistribution rights or assign a license without source/permission information; follow the contribution guidance in that directory's README.
- Update `README.md` when user-facing commands, supported formats, build steps, or safety behavior change.
