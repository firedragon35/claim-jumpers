"""Draw the Claim Jumpers icon: the Mk I tug ("Second Mortgage") in its
class colours on a dark badge, as the game draws it (tug.rs hull outline).

Standard library only (no Pillow). Writes, into native/assets/icon/:
  claim-jumpers.ico         16, 32, 48 and 256 px (PNG-compressed entries)
  icon16.rgba icon32.rgba icon64.rgba   raw RGBA for the window icon
  claim-jumpers-256.png     a preview

Run from the native folder: python tools/make_icon.py
"""
import math
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "icon")

VOID = (0x07, 0x0A, 0x12)
HAZARD = (0xFF, 0xB0, 0x20)
HULL = (0x2A, 0x1D, 0x06)
CABLE = (0x4F, 0xE3, 0xFF)
STEEL = (0x9F, 0xB4, 0xCF)

# The tug hull in local units (tug.rs), nose along +x.
HULL_PTS = [(22, -6), (22, 6), (14, 13), (-14, 14), (-20, 8), (-20, -8), (-14, -14), (14, -13)]
ANGLE = -math.pi / 4  # nose to the upper right
SCALE = 0.032         # local units -> icon space (-1..1)


def inside_poly(x, y, pts):
    """Even-odd point-in-polygon."""
    c = False
    j = len(pts) - 1
    for i, (xi, yi) in enumerate(pts):
        xj, yj = pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            c = not c
        j = i
    return c


def scaled(pts, k):
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    return [(cx + (x - cx) * k, cy + (y - cy) * k) for x, y in pts]


INNER = scaled(HULL_PTS, 0.8)


def sample(u, v):
    """Colour and coverage at icon-space point (u, v) in -1..1 (y down)."""
    # Rounded-square badge with a hazard rim.
    ext, rad = 0.94, 0.22
    qx, qy = max(abs(u) - (ext - rad), 0.0), max(abs(v) - (ext - rad), 0.0)
    d = math.hypot(qx, qy)
    if d > rad:
        return None
    colour = HAZARD if d > rad - 0.07 else VOID
    # Into tug-local coordinates (undo the rotation and scale).
    c, s = math.cos(-ANGLE), math.sin(-ANGLE)
    x, y = (u * c - v * s) / SCALE, (u * s + v * c) / SCALE
    if 20 <= x <= 32 and -1.8 <= y <= 1.8:
        colour = STEEL  # the cutter barrel
    if inside_poly(x, y, HULL_PTS):
        colour = HAZARD  # the amber edge
        if inside_poly(x, y, INNER):
            colour = HULL
            if 6 <= x <= 15 and -5 <= y <= 5:
                colour = CABLE  # cab window
    return colour


def render(n, ss=4):
    px = bytearray()
    for j in range(n):
        for i in range(n):
            r = g = b = a = 0
            for sj in range(ss):
                for si in range(ss):
                    u = ((i + (si + 0.5) / ss) / n) * 2 - 1
                    v = ((j + (sj + 0.5) / ss) / n) * 2 - 1
                    col = sample(u, v)
                    if col:
                        r, g, b, a = r + col[0], g + col[1], b + col[2], a + 1
            k = ss * ss
            if a:
                px += bytes((r // a, g // a, b // a, round(255 * a / k)))
            else:
                px += bytes(4)
    return bytes(px)


def png(n, rgba):
    raw = b"".join(b"\x00" + rgba[y * n * 4:(y + 1) * n * 4] for y in range(n))

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def ico(images):
    """An .ico whose entries are PNGs (supported since Windows Vista)."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    dirs, blobs = b"", b""
    for n, data in images:
        dirs += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        blobs += data
    return head + dirs + blobs


def main():
    os.makedirs(OUT, exist_ok=True)
    pix = {n: render(n) for n in (16, 32, 48, 64, 256)}
    for n in (16, 32, 64):
        with open(os.path.join(OUT, f"icon{n}.rgba"), "wb") as f:
            f.write(pix[n])
    with open(os.path.join(OUT, "claim-jumpers.ico"), "wb") as f:
        f.write(ico([(n, png(n, pix[n])) for n in (16, 32, 48, 256)]))
    with open(os.path.join(OUT, "claim-jumpers-256.png"), "wb") as f:
        f.write(png(256, pix[256]))
    print("icon written to", os.path.normpath(OUT))


if __name__ == "__main__":
    main()
