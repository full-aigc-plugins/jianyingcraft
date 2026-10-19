#!/usr/bin/env python3
"""Rename the app-icon record in the checked-in DMG Finder layout (.DS_Store).

`packaging/macos/dmg/dmg-layout.DS_Store` is a macOS Finder DSDB file.  The
checked-in copy predates the fork: its icon-location record is keyed
`FilmCraft.app`, while the product, the DMG volume and `generate.py` use
`JianyingCraft.app`.  `generate.py` can rewrite the file, but that needs
`ds_store`, `mac_alias`, `resvg` and Pillow, which are not available in every
environment.  This script renames that one record key with the standard
library only, so the change is reproducible anywhere:

    python3 packaging/macos/dmg/rename_layout_entry.py --check
    python3 packaging/macos/dmg/rename_layout_entry.py --dry-run
    python3 packaging/macos/dmg/rename_layout_entry.py

How the file is built.  A 36-byte header (magic `00 00 00 01`, `Bud1`, then
seven 32-bit words) is followed by 4 KiB blocks.  Each block starts with a
header and then a list of records:

    key length (u32, UTF-16 code units)
    key        (UTF-16BE)
    code       (4 ASCII bytes, e.g. b"Iloc")
    type       (4 ASCII bytes, e.g. b"blob", b"long")
    value      (blob/ustr: u32 byte length + bytes; long/shor/bool/type/
                comp/dutc: fixed size, no length prefix)

A record list is followed by zero padding up to the end of its block.  Blocks
sit on 4 KiB boundaries, but not every block is a record block: the root and
DSDB blocks hold bookkeeping with a different payload.  The script therefore
parses every 4 KiB boundary, keeps the ones whose header and record list are
self-consistent and whose tail is zero padding, and searches those records.

The seven 32-bit words after the signature are the writer's block bookkeeping
(Finder's format describes them as the root block, block count and block-table
pointers).  In this file they do not form a simple offset table, so the script
treats them as opaque: it never moves or rewrites a block, and it verifies that
all 36 header bytes are identical afterwards.

Why in-place growth is safe.  `JianyingCraft.app` is 17 UTF-16 code units
against 13 for `FilmCraft.app`, so the record grows by 8 bytes.  The record is
the last one in its block and the block is zero-padded to its end, so the 8
bytes come out of that padding: the file size, every block offset, every block
header and every other record keep their exact bytes and positions.  The
script proves this before writing and refuses to touch the file otherwise:

* the header magic/signature is intact;
* the block parses, its `next` pointer is 0, and the record list ends in zero
  padding (the checked-in file has ~2.9 KiB free); if there is not enough, the
  script exits 2 rather than relocating a block and rewriting the writer's
  bookkeeping;
* no 32-bit value stored anywhere in the file points into the edited range, so
  no offset table entry can become stale (there is nothing to relocate);
* after writing, the file is re-parsed and compared with the original: same
  size, same block headers, same records, only the `.app` key changed.

`--check` exits 1 while the stale key is present (so CI can gate on it) and 0
once the checked-in file is current.  `--dry-run` prints the plan and exits 0
without writing.  The script is idempotent: a second run changes nothing.

Scope: this script renames the icon record key only.  The `icvp` record's
embedded background alias still carries the upstream volume name (the checked-in
file points at a volume called `FilmCraft`); regenerating with `generate.py` on
a machine with its dependencies is the way to refresh that alias.

The audit trail lives in the script's output, not in the file: this is
structural verification of the DSDB, not visual confirmation in Finder.
`generate.py` remains the canonical generator on a machine with its
dependencies; see README.md.
"""

from __future__ import annotations

import argparse
import dataclasses
import os
import shutil
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_FILE = HERE / "dmg-layout.DS_Store"

HEADER_SIZE = 36
BLOCK_SIZE = 4096
MAGIC = b"\x00\x00\x00\x01"
SIGNATURE = b"Bud1"
APP_KEY = "JianyingCraft.app"

# Value encodings used by Finder's records.  `blob` is prefixed with a byte
# length; `ustr` with a length in UTF-16 code units; the other types have a
# fixed size and no prefix.
LENGTH_PREFIXED_TYPES = {b"blob", b"ustr"}
FIXED_VALUE_SIZES = {
    b"long": 4,
    b"shor": 2,
    b"bool": 1,
    b"type": 4,
    b"comp": 8,
    b"dutc": 8,
}

EXIT_OK = 0
EXIT_STALE = 1
EXIT_ERROR = 2


class LayoutError(Exception):
    """The file is not a layout this script can patch safely."""


@dataclasses.dataclass(frozen=True)
class Record:
    """One Finder record inside a block."""

    key: str
    code: str
    value_type: str
    key_units: int
    start: int
    value_start: int
    value_length: int
    end: int

    def describe(self) -> str:
        return (
            f"{self.key!r} {self.code} {self.value_type} "
            f"@0x{self.start:x} ({self.value_length} value bytes)"
        )


@dataclasses.dataclass(frozen=True)
class Block:
    """A parsed record block."""

    start: int
    header_size: int
    next_block: int
    reserved: int | None
    records: tuple[Record, ...]

    def describe(self) -> str:
        reserved = "" if self.reserved is None else f" reserved=0x{self.reserved:x}"
        return (
            f"block @0x{self.start:x}: {self.header_size}-byte header, "
            f"next=0x{self.next_block:x}{reserved}, {len(self.records)} records"
        )


def read_u32(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 4 > len(data):
        raise LayoutError(f"truncated 32-bit field at 0x{offset:x}")
    return int.from_bytes(data[offset : offset + 4], "big")


def pack_u32(value: int) -> bytes:
    return value.to_bytes(4, "big")


def parse_record(data: bytes, offset: int, limit: int) -> Record:
    """Parse one record at `offset`; `limit` is the end of the block."""
    if offset + 16 > limit:
        raise LayoutError(f"record header at 0x{offset:x} runs past the block")
    key_units = read_u32(data, offset)
    if key_units > 255:
        raise LayoutError(f"implausible key length {key_units} at 0x{offset:x}")
    key_start = offset + 4
    key_end = key_start + 2 * key_units
    code_end = key_end + 4
    type_end = code_end + 4
    if type_end > limit:
        raise LayoutError(f"record at 0x{offset:x} runs past the block")
    try:
        key = data[key_start:key_end].decode("utf-16-be")
        code = data[key_end:code_end].decode("ascii")
        value_type = data[code_end:type_end].decode("ascii")
    except UnicodeDecodeError as exc:
        raise LayoutError(f"record at 0x{offset:x}: {exc}") from exc
    if not (code.isprintable() and value_type.isprintable()):
        raise LayoutError(f"record at 0x{offset:x}: non-text code/type")

    if value_type.encode("ascii") in LENGTH_PREFIXED_TYPES:
        value_start = type_end + 4
        if value_start > limit:
            raise LayoutError(f"record at 0x{offset:x} has a truncated value")
        raw_length = read_u32(data, type_end)
        # `blob` counts bytes, `ustr` counts UTF-16 code units.
        value_length = raw_length if value_type == "blob" else 2 * raw_length
        end = value_start + value_length
    else:
        fixed = FIXED_VALUE_SIZES.get(value_type.encode("ascii"))
        if fixed is None:
            raise LayoutError(f"record at 0x{offset:x}: unsupported type {value_type!r}")
        value_start = type_end
        value_length = fixed
        end = value_start + fixed
    if end > limit:
        raise LayoutError(f"record at 0x{offset:x} runs past the block")
    return Record(key, code, value_type, key_units, offset, value_start, value_length, end)


def parse_block(data: bytes, start: int) -> Block | None:
    """Parse a block at `start`, or return None if it is not a record block.

    Finder's blocks are 8 bytes (next, count), but the writer of the checked-in
    file puts three words before the records.  Try both; accept only a block
    whose whole record list parses and whose tail is zero padding.
    """
    for header_size in (12, 8):
        if start + header_size > len(data):
            continue
        record_count = read_u32(data, start + header_size - 4)
        if not 1 <= record_count <= 4096:
            continue
        next_block = read_u32(data, start)
        if next_block != 0 and not 0 < next_block < len(data):
            continue
        reserved = read_u32(data, start + 4) if header_size == 12 else None
        limit = min(start + BLOCK_SIZE, len(data))
        offset = start + header_size
        records = []
        try:
            for _ in range(record_count):
                record = parse_record(data, offset, limit)
                records.append(record)
                offset = record.end
        except LayoutError:
            continue
        tail = data[offset:limit]
        # A record list is followed by a zero key length (or just padding).
        if len(tail) < 4 or any(tail[:4]) or any(tail[4:]):
            continue
        return Block(start, header_size, next_block, reserved, tuple(records))
    return None


def parse_blocks(data: bytes) -> list[Block]:
    """Parse every record block on the 4 KiB block grid."""
    blocks = []
    for start in range(BLOCK_SIZE, len(data) - 4, BLOCK_SIZE):
        block = parse_block(data, start)
        if block is not None:
            blocks.append(block)
    return blocks


def parse_header(data: bytes) -> None:
    """Validate the 36-byte header; the words are bookkeeping and stay as-is."""
    if len(data) < HEADER_SIZE:
        raise LayoutError("file is shorter than the 36-byte header")
    if data[:4] != MAGIC:
        raise LayoutError(f"bad magic {data[:4].hex(' ')}, expected {MAGIC.hex(' ')}")
    if data[4:8] != SIGNATURE:
        raise LayoutError(f"bad signature {data[4:8]!r}, expected {SIGNATURE!r}")
    if data[HEADER_SIZE:] and len(data) % BLOCK_SIZE not in (4, 0):
        # The writer appends a 4-byte footer to full blocks; anything else is a
        # layout we have not seen and should not edit blind.
        raise LayoutError(
            f"unexpected file length {len(data)} (not a whole number of "
            f"{BLOCK_SIZE}-byte blocks plus the 4-byte footer)"
        )


def find_app_record(blocks: list[Block]) -> tuple[Block, int]:
    """Return the unique record whose key ends in `.app`."""
    matches = [
        (block, index)
        for block in blocks
        for index, record in enumerate(block.records)
        if record.key.endswith(".app")
    ]
    if not matches:
        raise LayoutError("no record key ending in '.app'")
    if len(matches) > 1:
        keys = [block.records[index].key for block, index in matches]
        raise LayoutError(f"more than one '.app' record: {keys}")
    return matches[0]


def plan_rename(data: bytes, block: Block, index: int) -> bytes:
    """Return the full new file contents (same size) with the key renamed."""
    record = block.records[index]
    if record.key == APP_KEY:
        raise LayoutError(f"record key is already {APP_KEY!r}")
    if not record.key.endswith(".app"):
        raise LayoutError(f"record key {record.key!r} does not end in '.app'")
    if block.next_block != 0:
        raise LayoutError(
            f"block @0x{block.start:x} is chained to 0x{block.next_block:x}; "
            "refusing to grow a chained block"
        )

    # Keep the record's code/type/value bytes, replace only the key.
    suffix_start = record.start + 4 + 2 * record.key_units
    new_record = (
        pack_u32(len(APP_KEY)) + APP_KEY.encode("utf-16-be") + data[suffix_start : record.end]
    )
    delta = len(new_record) - (record.end - record.start)
    if delta <= 0:
        raise LayoutError("the new key does not grow the record; refusing a shrinking edit")

    block_start = block.start
    block_end = block_start + BLOCK_SIZE
    if block_end > len(data):
        raise LayoutError(f"block @0x{block_start:x} is truncated")
    tail = data[record.end : block_end]
    if any(tail):
        raise LayoutError(f"block @0x{block_start:x} has no zero padding after the record")
    if len(tail) < delta:
        raise LayoutError(f"block @0x{block_start:x} has only {len(tail)} free bytes")

    # Nothing in the file may point into the bytes we change (records are
    # sequential, so this should hold; prove it instead of assuming it).
    edit_start = record.start
    edit_end = block_end  # zero padding shifts with the tail, so cover it all
    pointers = []
    for offset in range(0, len(data) - 3, 4):
        value = int.from_bytes(data[offset : offset + 4], "big")
        if edit_start <= value < edit_end:
            pointers.append((offset, value))
    if pointers:
        raise LayoutError(
            "stored pointer(s) refer to the edited range: "
            + ", ".join(f"0x{offset:x}->0x{value:x}" for offset, value in pointers)
        )

    new_block = data[block_start:record.start] + new_record + tail
    new_block = new_block[:BLOCK_SIZE]
    if len(new_block) != BLOCK_SIZE:
        raise LayoutError("internal error: the rebuilt block has the wrong size")

    new_data = data[:block_start] + new_block + data[block_end:]
    if len(new_data) != len(data):
        raise LayoutError("internal error: the edit changed the file size")
    return new_data


def verify_patch(before: bytes, after: bytes) -> None:
    """Re-parse both images and check that only the app record key changed."""
    if len(before) != len(after):
        raise LayoutError("file size changed")
    if before[:HEADER_SIZE] != after[:HEADER_SIZE]:
        raise LayoutError("header bytes changed")
    blocks_before = parse_blocks(before)
    blocks_after = parse_blocks(after)
    if [(b.start, b.header_size, b.next_block, b.reserved) for b in blocks_before] != [
        (b.start, b.header_size, b.next_block, b.reserved) for b in blocks_after
    ]:
        raise LayoutError("block headers changed")
    if len(blocks_before) != len(blocks_after):
        raise LayoutError("record block count changed")

    app_before = find_app_record(blocks_before)
    app_after = find_app_record(blocks_after)
    if app_after[0].start != app_before[0].start or app_after[1] != app_before[1]:
        raise LayoutError("the app record moved")
    old = app_before[0].records[app_before[1]]
    new = app_after[0].records[app_after[1]]
    if new.key != APP_KEY or old.key == APP_KEY:
        raise LayoutError(f"rename did not take: {old.key!r} -> {new.key!r}")
    if (new.code, new.value_type, new.value_length) != (
        old.code,
        old.value_type,
        old.value_length,
    ):
        raise LayoutError("the app record's type or value changed")
    if before[old.value_start : old.end] != after[new.value_start : new.end]:
        raise LayoutError("the app record's Iloc payload changed")

    # Every other record must be byte-identical (content, not position).
    def others(image, blocks, skip):
        return [
            (record.key, image[record.start : record.end])
            for block in blocks
            for record in block.records
            if (block, record) != skip
        ]

    if others(before, blocks_before, (app_before[0], old)) != others(
        after, blocks_after, (app_after[0], new)
    ):
        raise LayoutError("another record changed")


def describe_file(path: Path, data: bytes) -> None:
    blocks = parse_blocks(data)
    print(f"{path.name}: {len(data)} bytes")
    print(
        f"  header: magic={MAGIC.hex(' ')} signature={SIGNATURE.decode()} "
        f"({HEADER_SIZE} bytes, preserved verbatim)"
    )
    if not blocks:
        print("  no record blocks found")
    for block in blocks:
        print(f"  {block.describe()}")
        for index, record in enumerate(block.records):
            print(f"    [{index}] @0x{record.start:x} {record.describe()}")


def write_atomically(path: Path, data: bytes) -> None:
    directory = path.parent
    handle, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=directory)
    try:
        with os.fdopen(handle, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        shutil.copystat(path, temporary)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Rename the app-icon record key in the checked-in DMG .DS_Store.",
    )
    parser.add_argument(
        "file",
        nargs="?",
        type=Path,
        default=DEFAULT_FILE,
        help=f"layout file to patch (default: {DEFAULT_FILE})",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if the stale '.app' key is still present, 0 if current",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the planned change without writing the file",
    )
    args = parser.parse_args(argv)

    path: Path = args.file
    try:
        data = path.read_bytes()
        parse_header(data)
        blocks = parse_blocks(data)
        block, index = find_app_record(blocks)
        record = block.records[index]
        current = record.key == APP_KEY

        if args.check:
            if current:
                print(f"{path.name}: up to date ({APP_KEY})")
                return EXIT_OK
            print(f"{path.name}: stale key {record.key!r} (expected {APP_KEY!r})")
            return EXIT_STALE

        describe_file(path, data)
        print(
            f"  app record: {record.key!r}, {record.key_units} code units "
            f"({record.key_units * 2} key bytes)"
        )
        if current:
            print(f"{path.name}: already up to date, nothing to do")
            return EXIT_OK

        new_data = plan_rename(data, block, index)
        grown = len(APP_KEY) - record.key_units
        print(
            f"  rename: {record.key!r} -> {APP_KEY!r} "
            f"({record.key_units} -> {len(APP_KEY)} code units, {grown:+d} units = "
            f"{2 * grown:+d} bytes)"
        )
        print(
            f"  block @0x{block.start:x}: grows from {record.end - block.start} to "
            f"{record.end - block.start + 2 * grown} used bytes of {BLOCK_SIZE}; "
            "no block moves, no stored pointer refers to the edited range"
        )
        if args.dry_run:
            print(f"{path.name}: dry run, not written")
            return EXIT_OK

        write_atomically(path, new_data)
        written = path.read_bytes()
        verify_patch(data, written)
        print(f"{path.name}: wrote {len(written)} bytes; re-parsed and verified")
        return EXIT_OK
    except LayoutError as exc:
        print(f"error: {path}: {exc}", file=sys.stderr)
        return EXIT_ERROR
    except OSError as exc:
        print(f"error: {path}: {exc}", file=sys.stderr)
        return EXIT_ERROR


if __name__ == "__main__":
    raise SystemExit(main())
