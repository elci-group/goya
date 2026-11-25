"""Derive Arachnid's launcher icons from assets/arachnid-logo.png (stdlib only).

    python3 -I scripts/arachnid-icons.py assets/arachnid-logo.png OUT_DIR

Writes icon-80.png, LG's required 80x80 launcher icon. The logo is cropped to a centred square and
area-averaged down, so the output is deterministic.
"""
import os, struct, sys, zlib

def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    i, idat, w = 8, b"", None
    while i < len(data):
        n, kind = struct.unpack(">I4s", data[i:i + 8])
        body = data[i + 8:i + 8 + n]
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert depth == 8 and ctype in (2, 6) and interlace == 0, "8-bit RGB/RGBA only"
            bpp = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat += body
        i += 12 + n
    raw, stride, rows, prev = zlib.decompress(idat), w * bpp, [], bytearray(w * bpp)
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b, c = prev[x], prev[x - bpp] if x >= bpp else 0
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + b) & 255
            elif f == 3: line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line); prev = line
    return w, h, bpp, rows

def resize(w, h, bpp, rows, size):
    side = min(w, h); ox, oy = (w - side) // 2, (h - side) // 2
    out = []
    for ty in range(size):
        y0, y1 = oy + ty * side // size, oy + (ty + 1) * side // size
        line = bytearray()
        for tx in range(size):
            x0, x1 = ox + tx * side // size, ox + (tx + 1) * side // size
            acc, n = [0, 0, 0], 0
            for y in range(y0, y1):
                r = rows[y]
                for x in range(x0, x1):
                    for k in range(3): acc[k] += r[x * bpp + k]
                    n += 1
            line.extend((v + n // 2) // n for v in acc)
        out.append(line)
    return out

def write_png(path, rows):
    size = len(rows)
    def chunk(kind, body):
        c = kind + body
        return struct.pack(">I", len(body)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)
    raw = b"".join(b"\x00" + bytes(r) for r in rows)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)) \
        + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)

src, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
w, h, bpp, rows = read_png(src)
for size in (80,):
    write_png(os.path.join(out, f"icon-{size}.png"), resize(w, h, bpp, rows, size))
print(f"{src} {w}x{h} -> {out}/icon-80.png")
