#!/usr/bin/env python3
"""Draw the Rusty Z icon (a ringed planet on a night-sky tile) as a 1024x1024
PNG, standard library only. Usage: make-icon.py OUT.png"""

import math
import struct
import sys
import zlib

N = 1024


def smooth(d):
    """Coverage of an edge at signed distance d (pixels, negative inside)."""
    return min(1.0, max(0.0, 0.5 - d))


def mix(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def rounded_rect_dist(x, y, cx, cy, half, r):
    qx, qy = abs(x - cx) - (half - r), abs(y - cy) - (half - r)
    outside = math.hypot(max(qx, 0), max(qy, 0))
    return outside + min(max(qx, qy), 0) - r


STARS = [(250, 260, 5), (720, 210, 4), (820, 380, 3), (300, 760, 4), (210, 520, 3), (780, 800, 5), (560, 170, 3)]

# Planet and a ring tilted by TILT, squashed vertically by SQUASH.
PX, PY, PR = 512, 520, 230
TILT, SQUASH = math.radians(-18), 0.28
RING_IN, RING_OUT = 290, 400
COS, SIN = math.cos(TILT), math.sin(TILT)


def ring(x, y):
    """(coverage, is_front) of the ring at a pixel."""
    dx, dy = x - PX, y - PY
    u = dx * COS + dy * SIN
    v = (-dx * SIN + dy * COS) / SQUASH
    r = math.hypot(u, v)
    scale = SQUASH  # distances shrink across the ring's thin direction
    cov = smooth((RING_IN - r) * scale) * smooth((r - RING_OUT) * scale)
    band = 0.8 + 0.2 * math.cos((r - RING_IN) / 9.0)
    return cov * band, v > 0


def pixel(x, y):
    tile = smooth(rounded_rect_dist(x, y, 512, 512, 412, 185))
    if tile == 0:
        return (0, 0, 0, 0)
    sky = mix((18, 24, 54), (6, 8, 20), y / N)
    col = sky
    for sx, sy, sr in STARS:
        col = mix(col, (230, 235, 255), smooth(math.hypot(x - sx, y - sy) - sr))

    ring_cov, front = ring(x, y)
    ring_col = (226, 180, 110)
    if not front:
        col = mix(col, ring_col, ring_cov)

    d = math.hypot(x - PX, y - PY)
    planet = smooth(d - PR)
    if planet > 0:
        # Lit from the upper left, with soft bands.
        nx, ny = (x - PX) / PR, (y - PY) / PR
        nz = math.sqrt(max(0.0, 1 - nx * nx - ny * ny))
        light = max(0.0, -0.5 * nx - 0.6 * ny + 0.62 * nz)
        band = 0.5 + 0.5 * math.sin(ny * 11 + 0.8 * math.sin(nx * 3))
        base = mix((183, 65, 14), (214, 112, 42), band)
        shade = mix((40, 12, 8), base, min(1.0, 0.15 + light))
        col = mix(col, shade, planet)

    if front:
        col = mix(col, ring_col, ring_cov)
    return (*(round(c) for c in col), round(255 * tile))


def main():
    rows = bytearray()
    for y in range(N):
        rows.append(0)
        for x in range(N):
            rows.extend(pixel(x + 0.5, y + 0.5))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(rows), 9))
    png += chunk(b"IEND", b"")
    with open(sys.argv[1], "wb") as f:
        f.write(png)


if __name__ == "__main__":
    main()
