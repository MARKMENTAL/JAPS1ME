# PS1 Save and Memory-Card Formats

This guide describes the formats JAPS1ME currently reads or writes. The notes
were informed by the [MemcardRex project](https://github.com/ShendoXT/memcardrex),
which is also credited in the main README. Corresponding JAPS1ME
implementations: `src/gme.rs` and `src/ps1/`.

## DexDrive `.gme`

A `.gme` file is a 3904-byte header followed by a 131072-byte raw card image
(134976 bytes total). JAPS1ME identifies the format by length plus the `MC`
signature at offset 3904.

### Header

| Offset | Size | Meaning |
| --- | ---: | --- |
| `0x00` | 11 | Magic: `123-456-STD` (canonical files) |
| `0x12`–`0x15` | — | Version/status bytes written by MemcardRex (`0x01`, `0x01`/`M`) |
| `0x16`–`0x25` | 15 | Per-slot type bytes (copies of each directory frame's type) |
| `0x26`–`0x35` | 15 | Per-slot next-pointer bytes |
| `0x40`–`0xF3F` | 15 × 256 | Per-slot comments (system-codepage text, NUL-terminated) |

Some files in the wild have a zeroed or otherwise non-canonical header. Like
MemcardRex, JAPS1ME accepts those when the size is 134976 and bytes at 3904
are `MC`, and reports a `missing DexDrive header magic` warning. Comments are
decoded as Windows-1252 and are informational only; `convert` never modifies
them.

## Raw card image (`.srm`/`.mcr`)

The raw image is 131072 bytes (1024 frames of 128 bytes). RetroArch `.srm`,
ePSXe `.mcr`, and similar formats share this layout; only the file extension
differs. `convert` writes exactly the `.gme` payload bytes `3904..` with no
modification, so directory checksums and save data are preserved verbatim.

### Geometry

| Raw offset | Size | Meaning |
| --- | ---: | --- |
| `0x00` | 128 | Card header frame (`MC` magic) |
| `0x80`–`0x7FF` | 15 × 128 | Directory frames (slots 1–15) |
| `0x2000`– | 15 × 8192 | Save data blocks |

### Directory frames

Each 128-byte frame describes one slot. Relevant fields:

| Offset | Size | Meaning |
| --- | ---: | --- |
| `0x00` | 1 | Slot type: `0xA0` formatted, `0x51` initial, `0x52`/`0x53` middle/end link, `0xA1`–`0xA3` deleted variants |
| `0x04` | 3 | Payload size, little-endian |
| `0x08` | 1 | Next-slot pointer (`0xFF` = end of chain) |
| `0x0A` | 2 | Region (`BA` America, `BE` Europe, `BI` Japan) |
| `0x0C` | 10 | Product code |
| `0x16` | 8 | Save identifier |
| `0x7F` | 1 | XOR checksum over bytes `0x00..0x7F` |

A save is an initial slot plus its link chain (up to 15 slots). Links not
reachable from any initial slot are orphaned; `list` reports them and counts
them as free, matching MemcardRex's broken-link handling.

### Save titles

Each data block carries a 64-byte Shift-JIS title at block offset 4,
NUL-terminated on an even byte boundary. JAPS1ME decodes it with a
malformed-sequence replacement and falls back to the save name
(`region + product code + identifier`) when no usable title exists.

## References

- [MemcardRex](https://github.com/ShendoXT/memcardrex): `MemcardRex.Core/ps1card.cs`
  (`getGmeHeader`, `OpenMemoryCard`, `calculateXOR`, `loadStringData`,
  `FindSaveLinks`) and `MemcardRex.Core/Hardware/DexDrive.cs` (128-byte frame
  protocol and XOR framing on the wire).
- The corresponding JAPS1ME implementations: `src/gme.rs`, `src/ps1/`.
