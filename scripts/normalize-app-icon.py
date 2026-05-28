#!/usr/bin/env python3
"""Normalize a PNG app icon source and remove common checkerboard mattes."""

from __future__ import annotations

import struct
import sys
import zlib
from pathlib import Path


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def read_png(path: Path) -> tuple[int, int, list[bytearray]]:
    data = path.read_bytes()
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError("input is not a PNG")

    offset = len(PNG_SIGNATURE)
    width = height = color_type = bit_depth = interlace = None
    idat = bytearray()
    while offset < len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            width, height, bit_depth, color_type, _compression, _filter, interlace = struct.unpack(
                ">IIBBBBB",
                payload,
            )
        elif kind == b"IDAT":
            idat.extend(payload)
        elif kind == b"IEND":
            break

    if width is None or height is None or color_type is None or bit_depth is None or interlace is None:
        raise ValueError("missing PNG header")
    if bit_depth != 8 or color_type not in (2, 6) or interlace != 0:
        raise ValueError("only non-interlaced 8-bit RGB/RGBA PNGs are supported")

    channels = 4 if color_type == 6 else 3
    row_bytes = width * channels
    raw = zlib.decompress(bytes(idat))
    rows: list[bytearray] = []
    previous = bytearray(row_bytes)
    cursor = 0
    for _ in range(height):
        filter_type = raw[cursor]
        cursor += 1
        row = bytearray(raw[cursor : cursor + row_bytes])
        cursor += row_bytes
        for index, value in enumerate(row):
            left = row[index - channels] if index >= channels else 0
            up = previous[index]
            up_left = previous[index - channels] if index >= channels else 0
            if filter_type == 1:
                row[index] = (value + left) & 0xFF
            elif filter_type == 2:
                row[index] = (value + up) & 0xFF
            elif filter_type == 3:
                row[index] = (value + ((left + up) // 2)) & 0xFF
            elif filter_type == 4:
                predictor = paeth(left, up, up_left)
                row[index] = (value + predictor) & 0xFF
            elif filter_type != 0:
                raise ValueError(f"unsupported PNG filter: {filter_type}")
        previous = row
        if channels == 3:
            rgba = bytearray()
            for index in range(0, len(row), 3):
                rgba.extend((row[index], row[index + 1], row[index + 2], 255))
            rows.append(rgba)
        else:
            rows.append(row)
    return width, height, rows


def paeth(left: int, up: int, up_left: int) -> int:
    estimate = left + up - up_left
    left_distance = abs(estimate - left)
    up_distance = abs(estimate - up)
    up_left_distance = abs(estimate - up_left)
    if left_distance <= up_distance and left_distance <= up_left_distance:
        return left
    if up_distance <= up_left_distance:
        return up
    return up_left


def is_checker_pixel(r: int, g: int, b: int) -> bool:
    return min(r, g, b) >= 232 and max(r, g, b) - min(r, g, b) <= 14


def remove_checkerboard(width: int, height: int, rows: list[bytearray]) -> None:
    for y in range(height):
        for x in range(width):
            offset = x * 4
            r, g, b, alpha = rows[y][offset : offset + 4]
            if alpha > 0 and is_checker_pixel(r, g, b):
                rows[y][offset : offset + 4] = b"\xff\xff\xff\x00"


def write_png(path: Path, width: int, height: int, rows: list[bytearray]) -> None:
    raw = b"".join(bytes([0]) + bytes(row) for row in rows)

    def chunk(kind: bytes, payload: bytes) -> bytes:
        checksum = zlib.crc32(kind + payload) & 0xFFFFFFFF
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", checksum)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    path.write_bytes(PNG_SIGNATURE + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <source.png> <output.png>", file=sys.stderr)
        return 2

    source = Path(sys.argv[1])
    output = Path(sys.argv[2])
    width, height, rows = read_png(source)
    remove_checkerboard(width, height, rows)
    output.parent.mkdir(parents=True, exist_ok=True)
    write_png(output, width, height, rows)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
