# Un TAC dental (CBCT) inventado para las capturas de VeodarDental: los dos
# maxilares con sus dientes, raíces, conductos, el conducto dentario inferior, el
# paladar, los senos maxilares, un implante en el 46, el hueco del 36, una
# endodoncia en el 45, una amalgama en el 47 y una caries en el 17 (como el
# paciente de ejemplo, Carlos Martínez Sanz). DICOM Parte 10 sin comprimir
# (explicit VR little endian), un fichero multicorte.
#   python3 -I tools/cbct_ejemplo.py /tmp/cbct-ejemplo.dcm      (necesita numpy)
# Para las capturas de la web (el visor en sus tres vistas, con el paciente de ejemplo):
#   tools/aislado.sh ./target/release/veodardental --screenshot tac.png --user dir \
#     --size 1600x1000 --screen pacientes --patient 101 --tab imagenes \
#     --estudio /tmp/cbct-ejemplo.dcm --pieza 36 --vista axial:0.39 --bajar 520
#   (coronal:0.506 corta por el implante del 46 y el hueco del 36; sagital:0.234, por el 46)
import struct
import sys

import numpy as np

SP = 0.3  # mm por vóxel
NX, NY, NZ = 300, 260, 220  # x: derecha→izquierda del paciente; y: delante→detrás; z: arriba→abajo
Z0 = 27.0  # plano oclusal, en mm desde el corte de arriba
rng = np.random.default_rng(46)

# ── la arcada en U, vista desde arriba (mm), con su longitud de arco ──
t = np.linspace(-1.45, 1.45, 6001)
ax = 45.0 + 27.0 * t
ay = 11.0 + 38.0 * np.abs(t) ** 2.4
arc = np.concatenate([[0.0], np.cumsum(np.hypot(np.diff(ax), np.diff(ay)))])
arc -= np.interp(0.0, t, arc)  # s = 0 en la línea media; s < 0, derecha del paciente (izquierda de la imagen)

xs = (np.arange(NX) + 0.5) * SP
ys = (np.arange(NY) + 0.5) * SP
X, Y = np.meshgrid(xs, ys)
S = np.empty(X.size)
U = np.empty(X.size)
px, py = X.ravel(), Y.ravel()
for i in range(0, px.size, 4000):
    dx = px[i : i + 4000, None] - ax[None, :]
    dy = py[i : i + 4000, None] - ay[None, :]
    d2 = dx * dx + dy * dy
    k = d2.argmin(axis=1)
    r = np.arange(k.size)
    S[i : i + 4000] = arc[k]
    # hacia dentro de la U (lingual, palatino) es negativo
    tx, ty = np.gradient(ax)[k], np.gradient(ay)[k]
    cross = tx * dy[r, k] - ty * dx[r, k]
    inner = cross > 0  # el giro de la U es siempre hacia el mismo lado
    U[i : i + 4000] = np.where(inner, -1.0, 1.0) * np.sqrt(d2[r, k])
S = S.reshape(X.shape)
U = U.reshape(X.shape)


def arch_xy(s):
    tt = np.interp(s, arc, t)
    return 45.0 + 27.0 * tt, 11.0 + 38.0 * abs(tt) ** 2.4


# ── dientes: ancho mesiodistal, vestibulolingual, alto de corona, largo de raíz, raíces ──
LOWER = {1: (5.2, 6.0, 9.0, 12.5, 1), 2: (5.8, 6.2, 9.5, 14.0, 1), 3: (7.0, 7.6, 11.0, 16.0, 1), 4: (7.0, 7.6, 8.5, 14.0, 1), 5: (7.2, 8.2, 8.0, 14.5, 1), 6: (11.2, 10.4, 7.5, 14.0, 2), 7: (10.6, 10.0, 7.0, 13.0, 2)}
UPPER = {1: (8.6, 7.0, 10.5, 13.0, 1), 2: (6.6, 6.0, 9.0, 13.0, 1), 3: (7.6, 8.0, 10.0, 17.0, 1), 4: (7.0, 9.0, 8.5, 14.0, 1), 5: (6.8, 9.0, 7.8, 14.0, 1), 6: (10.2, 11.0, 7.5, 12.5, 2), 7: (9.2, 10.8, 7.2, 12.0, 2)}
teeth = []
for upper, table, quads in ((False, LOWER, ((1, 3), (-1, 4))), (True, UPPER, ((1, 2), (-1, 1)))):
    for side, quad in quads:
        pos = 0.4
        for n in range(1, 8):
            md, bl, ch, rl, roots = table[n]
            teeth.append(dict(num=quad * 10 + n, s=side * (pos + md / 2), md=md, bl=bl, ch=ch, rl=rl, roots=roots, upper=upper))
            pos += md + 0.3
by_num = {d["num"]: d for d in teeth}
MISSING, ENDO, AMALGAM, CARIES = {36}, {45}, {47}, {17}
IMPLANT = by_num[46]
UP_SHIFT = 1.7  # la arcada de arriba cae por fuera de la de abajo

HU = dict(aire=-1000, blando=40, cortical=1550, trabecular=520, esmalte=2900, dentina=1750, pulpa=260, lpd=230, lamina=1600, metal=4000, gutapercha=3300, zirconio=2700)


def conv(a, k, axis):
    a = np.moveaxis(a, axis, -1)
    pad = len(k) // 2
    p = np.pad(a, [(0, 0)] * (a.ndim - 1) + [(pad, pad)], mode="edge")
    out = sum(k[i] * p[..., i : i + a.shape[-1]] for i in range(len(k)))
    return np.moveaxis(out, -1, axis)


# trabéculas: las crestas de un ruido suavizado
noise = rng.standard_normal((NZ, NY, NX)).astype(np.float32)
k5 = np.array([1, 4, 6, 4, 1], np.float32) / 16
for _ in range(3):
    for axis in range(3):
        noise = conv(noise, k5, axis)
noise /= noise.std()
trab = np.clip(1.0 - np.abs(noise) * 1.1, 0, 1) ** 1.5

fov = (X - 45.0) ** 2 + (Y - 39.0) ** 2 <= 44.5**2
face = ((X - 45.0) / 43.5) ** 2 + ((Y - 42.0) / 41.0) ** 2 <= 1.0
aS = np.abs(S)


def jaw(sl, k, h, Uj, crest, base, W):
    """Hueso de un maxilar: sección a lo largo de la arcada. h = distancia al plano oclusal."""
    zc = (crest + base) / 2
    H = (base - crest) / 2
    w = W * (0.55 + 0.45 * np.sqrt(np.clip((h - crest) / 6.0, 0, 1)))
    uc = Uj + 0.6
    with np.errstate(divide="ignore", invalid="ignore"):
        e_out = (uc / np.maximum(w, 1e-3)) ** 2 + ((h - zc) / H) ** 6
        e_in = (uc / np.maximum(w - 1.5, 1e-3)) ** 2 + ((h - zc) / np.maximum(H - 1.9, 1e-3)) ** 6
    bone = (e_out <= 1) & (w > 0.3) & (h >= crest)
    core = bone & (e_in <= 1) & (w > 1.8)
    sl[bone] = HU["cortical"]
    tb = HU["trabecular"] - 200 + 560 * trab[k]
    sl[core] = tb[core]
    return bone


def tooth(sl, d, h, Uj, bone):
    if d["num"] in MISSING:
        return
    m = S - d["s"]
    u = Uj
    md, bl, rl = d["md"], d["bl"], d["rl"]
    cej = d["ch"]
    near = (np.abs(m) < md) & (np.abs(u) < bl)
    if not near.any():
        return
    if d is not IMPLANT and cej - 0.6 <= h <= cej + rl:
        kk = np.clip((h - cej) / rl, 0, 1)
        sc = 1 - 0.78 * kk**1.4
        if d["roots"] == 1:
            roots = [(0.0, 0.29 * md, 0.40 * bl)]
        else:
            spread = 0.27 * md + 0.07 * max(h - cej, 0)
            roots = [(-spread, 0.17 * md, 0.37 * bl), (spread, 0.16 * md, 0.35 * bl)]
        for m0, rm, ru in roots:
            q = lambda extra: ((m - m0) / (rm * sc + extra)) ** 2 + (u / (ru * sc + extra)) ** 2
            q0 = q(0.0)
            sl[near & bone & (q(0.6) <= 1) & (q(0.28) > 1)] = HU["lamina"]
            sl[near & bone & (q(0.28) <= 1) & (q0 > 1)] = HU["lpd"]
            sl[near & (q0 <= 1)] = HU["dentina"]
            canal = np.sqrt((m - m0) ** 2 + u**2) < 0.42 * (1 - 0.55 * kk)
            sl[near & (q0 <= 1) & canal] = HU["gutapercha"] if d["num"] in ENDO else HU["pulpa"]
    if -0.2 <= h <= cej + 0.2:
        hh = np.clip(h / cej, 0, 1)
        f = 0.82 + 0.18 * np.sin(np.pi * hh * 0.92)
        anterior = d["num"] % 10 <= 3
        a = md / 2 * f * (0.95 if anterior else 1.0)
        b = bl / 2 * f * ((0.28 + 0.72 * min(hh / 0.55, 1)) if anterior else 1.0)
        p = 2.6
        outer = (np.abs(m) / a) ** p + (np.abs(u) / b) ** p
        if anterior:
            top = 0.4 + 1.2 * (np.abs(m) / (md / 2)) ** 4
        else:
            top = 1.3 * np.exp(-((u / (0.17 * bl)) ** 2)) + 0.5 * np.exp(-((m / (0.12 * md)) ** 2)) + 1.6 * np.clip(outer, 0, 1) ** 3
        crown = near & (outer <= 1) & (h >= top) & (h <= cej)
        if d is IMPLANT:
            sl[crown] = HU["zirconio"]
            return
        e = 1.15
        inner = (np.abs(m) / max(a - e, 0.5)) ** p + (np.abs(u) / max(b - e, 0.5)) ** p
        dent = crown & (inner <= 1) & (h >= top + 1.5)
        sl[crown] = HU["esmalte"]
        sl[dent] = HU["dentina"]
        pc = (m / (0.24 * md)) ** 2 + (u / (0.2 * bl)) ** 2 + ((h - 0.75 * cej) / (0.24 * cej)) ** 2
        sl[dent & (pc <= 1)] = HU["gutapercha"] if d["num"] in ENDO else HU["pulpa"]
        if d["num"] in AMALGAM:
            sl[crown & (np.abs(m) < 2.6) & (np.abs(u) < 1.9) & (h < top + 2.2)] = HU["metal"]
        if d["num"] in CARIES:
            cz = (m + 1.2) ** 2 / 1.6**2 + u**2 / 1.3**2 + (h - top - 1.6) ** 2 / 1.7**2
            sl[crown & (cz <= 1)] = HU["pulpa"] + 250


vol = np.empty((NZ, NY, NX), np.float32)
for k in range(NZ):
    z = (k + 0.5) * SP
    sl = np.full((NY, NX), HU["aire"], np.float32)
    sl[face] = HU["blando"]
    if z >= Z0 - 0.5:  # ── mandíbula y dientes de abajo ──
        h = z - Z0
        crest = np.where(aS < 14, 10.8, 9.2) + np.where(np.abs(S - by_num[36]["s"]) < 5.0, 1.4, 0.0)
        W = (5.0 + 1.9 * np.clip((aS - 10) / 22, 0, 1)) * np.clip((66.0 - aS) / 7.0, 0, 1) ** 0.5
        bone = jaw(sl, k, h, U, crest, 37.0, W)
        for sign in (1, -1):  # conducto dentario y agujero mentoniano
            ss = sign * S
            f = np.clip((ss - 27.0) / 7.0, 0, 1)
            cu = -0.8 * f + 6.5 * (1 - f)
            cz = 27.5 * f + 22.0 * (1 - f)
            r = np.sqrt((U - cu) ** 2 + (h - cz) ** 2)
            along = (ss > 25.5) & (ss < 70)
            sl[along & (r < 1.95) & bone] = HU["lamina"] - 300
            sl[along & (r < 1.5)] = HU["pulpa"] - 60
        for d in teeth:
            if not d["upper"]:
                tooth(sl, d, h, U, bone)
        # implante en el 46: cuerpo roscado y pilar, con el artefacto del metal
        m = S - IMPLANT["s"]
        r = np.sqrt(m**2 + (U + 0.3) ** 2)
        cej = IMPLANT["ch"]
        top, bottom = cej + 0.8, cej + 12.0
        if cej - 1.5 <= h < top:
            sl[r < 2.1] = HU["metal"]
        if top <= h <= bottom:
            rad = 2.05 + 0.22 * np.sin(2 * np.pi * h / 0.8)
            rad = rad * (1 - 0.35 * max((h - (bottom - 2.5)) / 2.5, 0))
            th = np.arctan2(U + 0.3, m)
            ring = (r > rad) & (r < 14) & face
            streak = (260 * np.cos(7 * th) - 140) * np.exp(-np.maximum(r - rad, 0) / 4.5)
            sl[ring] += streak[ring]
            sl[r < rad] = HU["metal"]
    if z <= Z0 + 0.5:  # ── maxilar, paladar, senos y dientes de arriba ──
        h = Z0 - z
        Uu = U - UP_SHIFT
        crest = np.where(aS < 16, 12.2, 10.0)
        W = (5.6 + 2.2 * np.clip((aS - 10) / 22, 0, 1)) * np.clip((68.0 - aS) / 7.0, 0, 1) ** 0.5
        bone = jaw(sl, k, h, Uu, crest, 60.0, W)
        # paladar duro: una lámina de hueso que cierra la U por arriba
        vault = 12.5 + 7.0 * (1 - np.exp(np.minimum(Uu + 2.0, 0) / 5.0))
        pal = (Uu < -2.0) & (np.abs(h - vault) < 1.3) & (Y < 62) & face
        sl[pal] = HU["cortical"] - 150
        # senos maxilares: aire encima de las raíces de los molares, con su suelo fino
        for sign in (1, -1):
            sx, sy = arch_xy(sign * 41.0)
            sx += -sign * 3.0
            sinus = ((X - sx) / 9.0) ** 2 + ((Y - sy) / 12.0) ** 2 + ((h - 29.0) / 9.0) ** 2
            sl[(sinus <= 1.25) & (sinus > 1.0)] = HU["cortical"] - 300
            sl[sinus <= 1.0] = HU["aire"] + 60
        for d in teeth:
            if d["upper"]:
                tooth(sl, d, h, Uu, bone)
    sl[~fov] = HU["aire"]
    vol[k] = sl

k3 = np.array([0.25, 0.5, 0.25], np.float32)
for axis in range(3):
    vol = conv(vol, k3, axis)
vol += rng.normal(0, 55, vol.shape).astype(np.float32)
vol[:, ~fov] = HU["aire"]
pix = np.clip(np.rint(vol), -1024, 4095).astype("<i2")
# DICOM: z crece hacia la cabeza, así que el primer corte del fichero es el de abajo
pix = np.ascontiguousarray(pix[::-1])

for n in (46, 36, 31, 17):
    x, y = arch_xy(by_num[n]["s"])
    print(f"diente {n}: x={x:.1f} mm ({x / (NX * SP):.3f}), y={y:.1f} mm ({y / (NY * SP):.3f})")
print(f"plano oclusal: corte {Z0 / SP:.0f} de {NZ} ({Z0 / (NZ * SP):.3f})")


def el(g, e, vr, val):
    if isinstance(val, str):
        val = val.encode("ascii")
        if len(val) % 2:
            val += b"\0" if vr == "UI" else b" "
    elif len(val) % 2:
        val += b"\0"
    if vr in ("OB", "OW", "OF", "SQ", "UT", "UN"):
        return struct.pack("<HH2sHI", g, e, vr.encode(), 0, len(val)) + val
    return struct.pack("<HH2sH", g, e, vr.encode(), len(val)) + val


root = "1.2.826.0.1.3680043.8.498."
sop_class = "1.2.840.10008.5.1.4.1.1.2"
sop = root + "4611202611150001"
meta = el(2, 1, "OB", b"\0\1") + el(2, 2, "UI", sop_class) + el(2, 3, "UI", sop) + el(2, 0x10, "UI", "1.2.840.10008.1.2.1") + el(2, 0x12, "UI", root + "1")
meta = el(2, 0, "UL", struct.pack("<I", len(meta))) + meta
us = lambda v: struct.pack("<H", v)
ds = (
    el(8, 0x16, "UI", sop_class)
    + el(8, 0x18, "UI", sop)
    + el(8, 0x20, "DA", "20261115")
    + el(8, 0x60, "CS", "CT")
    + el(8, 0x103E, "LO", "CBCT de ejemplo (sintetico)")
    + el(0x10, 0x10, "PN", "MARTINEZ SANZ^CARLOS")
    + el(0x18, 0x50, "DS", f"{SP}")
    + el(0x20, 0x13, "IS", "1")
    + el(0x20, 0x32, "DS", "0\\0\\0")
    + el(0x20, 0x37, "DS", "1\\0\\0\\0\\1\\0")
    + el(0x28, 2, "US", us(1))
    + el(0x28, 4, "CS", "MONOCHROME2")
    + el(0x28, 8, "IS", str(NZ))
    + el(0x28, 0x10, "US", us(NY))
    + el(0x28, 0x11, "US", us(NX))
    + el(0x28, 0x30, "DS", f"{SP}\\{SP}")
    + el(0x28, 0x100, "US", us(16))
    + el(0x28, 0x101, "US", us(16))
    + el(0x28, 0x102, "US", us(15))
    + el(0x28, 0x103, "US", us(1))
    + el(0x28, 0x1050, "DS", "900")
    + el(0x28, 0x1051, "DS", "3600")
    + el(0x28, 0x1052, "DS", "0")
    + el(0x28, 0x1053, "DS", "1")
    + el(0x7FE0, 0x10, "OW", pix.tobytes())
)
with open(sys.argv[1], "wb") as fh:
    fh.write(b"\0" * 128 + b"DICM" + meta + ds)
print(sys.argv[1], f"{NX}x{NY}x{NZ}", f"{pix.nbytes / 1e6:.1f} MB")
