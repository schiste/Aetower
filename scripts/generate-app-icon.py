#!/usr/bin/env python3
"""Generate Aetower's macOS iconset without external image dependencies."""

from __future__ import annotations

import math
import struct
import sys
import zlib
from pathlib import Path


ICON_SPECS = (
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
)


def lerp(a: int, b: int, t: float) -> int:
    return int(a + (b - a) * max(0.0, min(1.0, t)))


def distance_to_segment(px: float, py: float, ax: float, ay: float, bx: float, by: float) -> float:
    vx = bx - ax
    vy = by - ay
    wx = px - ax
    wy = py - ay
    segment_len = vx * vx + vy * vy
    if segment_len == 0:
        return math.hypot(px - ax, py - ay)
    t = max(0.0, min(1.0, (wx * vx + wy * vy) / segment_len))
    return math.hypot(px - (ax + t * vx), py - (ay + t * vy))


def rounded_square_alpha(x: float, y: float) -> float:
    margin = 0.035
    radius = 0.22
    inner_x = max(margin + radius - x, 0.0, x - (1.0 - margin - radius))
    inner_y = max(margin + radius - y, 0.0, y - (1.0 - margin - radius))
    distance = math.hypot(inner_x, inner_y)
    edge = radius - distance
    return max(0.0, min(1.0, edge / 0.012))


def blend(base: tuple[int, int, int], accent: tuple[int, int, int], amount: float) -> tuple[int, int, int]:
    return (
        lerp(base[0], accent[0], amount),
        lerp(base[1], accent[1], amount),
        lerp(base[2], accent[2], amount),
    )


def icon_pixel(size: int, x_index: int, y_index: int) -> tuple[int, int, int, int]:
    x = (x_index + 0.5) / size
    y = (y_index + 0.5) / size
    alpha = rounded_square_alpha(x, y)
    if alpha <= 0:
        return (0, 0, 0, 0)

    diagonal = (x + y) / 2.0
    base = blend((7, 17, 18), (23, 47, 52), 1.0 - diagonal)
    glow = math.exp(-((x - 0.68) ** 2 + (y - 0.30) ** 2) / 0.055)
    color = blend(base, (45, 205, 154), glow * 0.38)

    # Tower mast and braces.
    line_width = 0.018 if size >= 128 else 0.026
    tower_lines = (
        ((0.50, 0.20), (0.50, 0.77)),
        ((0.50, 0.20), (0.31, 0.77)),
        ((0.50, 0.20), (0.69, 0.77)),
        ((0.36, 0.53), (0.64, 0.53)),
    )
    tower_strength = 0.0
    for (ax, ay), (bx, by) in tower_lines:
        distance = distance_to_segment(x, y, ax, ay, bx, by)
        tower_strength = max(tower_strength, max(0.0, 1.0 - distance / line_width))
    color = blend(color, (221, 255, 238), tower_strength)

    # A small pressure trend at the bottom keeps the icon connected to the app.
    if 0.19 <= x <= 0.81:
        wave_y = 0.76 - math.sin((x - 0.19) / 0.62 * math.pi * 2.7) * 0.035
        wave_strength = max(0.0, 1.0 - abs(y - wave_y) / (line_width * 0.72))
        color = blend(color, (49, 213, 132), wave_strength)

    dot_distance = math.hypot(x - 0.50, y - 0.20)
    dot_strength = max(0.0, 1.0 - dot_distance / 0.055)
    color = blend(color, (255, 247, 191), dot_strength)

    return (color[0], color[1], color[2], int(255 * alpha))


def write_png(path: Path, size: int) -> None:
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            row.extend(icon_pixel(size, x, y))
        rows.append(bytes(row))

    raw = b"".join(rows)

    def chunk(kind: bytes, payload: bytes) -> bytes:
        checksum = zlib.crc32(kind + payload) & 0xFFFFFFFF
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", checksum)

    payload = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", payload) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    path.write_bytes(png)


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <output.iconset>", file=sys.stderr)
        return 2

    iconset = Path(sys.argv[1])
    iconset.mkdir(parents=True, exist_ok=True)
    for old_png in iconset.glob("*.png"):
        old_png.unlink()
    for filename, size in ICON_SPECS:
        write_png(iconset / filename, size)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
