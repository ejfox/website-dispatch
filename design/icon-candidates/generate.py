#!/usr/bin/env python3
"""Generate the Dispatch app-icon candidates as 1024x1024 SVGs.

macOS Big Sur+ grid: 1024 canvas, 824 body centred (100px gutter), continuous-
corner squircle, shadow baked in (28px blur, 12px down, black 50%; the Apple
template ships it in the artwork, the Dock adds nothing).

Run:  python3 design/icon-candidates/generate.py
Writes a-postmark.svg (+ a-postmark-small.svg for 16/32px), b-slip.svg,
c-outbox.svg next to this file.
"""
import math
import os
import random

HERE = os.path.dirname(os.path.abspath(__file__))
C, HALF = 512, 412  # canvas centre, half body size (824/2)


def squircle(cx=C, cy=C, a=HALF, n=5.2, steps=360):
    """Superellipse |x|^n + |y|^n = a^n, the usual stand-in for Apple's
    continuous-curvature corner (visually ~ rounded rect r=185 with smoothing)."""
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        ct, st = math.cos(t), math.sin(t)
        x = cx + a * math.copysign(abs(ct) ** (2 / n), ct)
        y = cy + a * math.copysign(abs(st) ** (2 / n), st)
        pts.append(f"{x:.1f},{y:.1f}")
    return "M" + " L".join(pts) + " Z"


BODY = squircle()


def head(dark: bool) -> str:
    if dark:
        top, bot, rim, grain = "#2c2c2c", "#0a0a0a", "rgba(255,255,255,0.16)", 0.05
    else:
        top, bot, rim, grain = "#f6f3ec", "#e2ddd1", "rgba(0,0,0,0.10)", 0.07
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <linearGradient id="body" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{top}"/>
      <stop offset="1" stop-color="{bot}"/>
    </linearGradient>
    <filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur in="SourceAlpha" stdDeviation="14"/>
      <feOffset dy="12" result="b"/>
      <feComponentTransfer><feFuncA type="linear" slope="0.5"/></feComponentTransfer>
      <feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
    <filter id="soft" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur in="SourceAlpha" stdDeviation="10"/>
      <feOffset dy="10"/>
      <feComponentTransfer><feFuncA type="linear" slope="0.35"/></feComponentTransfer>
      <feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
    <filter id="grain" x="0" y="0" width="100%" height="100%">
      <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="8" stitchTiles="stitch"/>
      <feColorMatrix type="saturate" values="0"/>
    </filter>
    <clipPath id="clip"><path d="{BODY}"/></clipPath>
  </defs>
  <!-- shadow drawn as its own layer: running the gradient body through the
       filter made cairo quantise it into ~100px bands -->
  <path d="{BODY}" fill="#000" filter="url(#shadow)"/>
  <path d="{BODY}" fill="url(#body)"/>
  <!-- faint grain: dithers the gradient (no 8-bit banding) and gives paper tooth -->
  <rect width="1024" height="1024" filter="url(#grain)" opacity="{grain}" clip-path="url(#clip)"/>
  <!-- inner rim: 3px inside the edge so the squircle reads on a black Dock -->
  <path d="{BODY}" fill="none" stroke="{rim}" stroke-width="6" clip-path="url(#clip)"/>
  <g clip-path="url(#clip)">
'''


TAIL = "  </g>\n</svg>\n"


def wavy(x0, x1, y, amp, period, phase, rnd, jitter=3.0):
    """Hand-inked cancellation line: a sine with a little wobble."""
    pts = []
    steps = int((x1 - x0) / 6)
    for i in range(steps + 1):
        x = x0 + (x1 - x0) * i / steps
        a = amp * (1 + 0.15 * math.sin(i * 0.7 + phase))
        yy = y + a * math.sin(2 * math.pi * (x - x0) / period + phase) + rnd.uniform(-jitter, jitter) * 0.3
        pts.append(f"{x:.1f},{yy:.1f}")
    return "M" + " L".join(pts)


def star(cx, cy, r_out, r_in, points=5, rot=-90):
    pts = []
    for i in range(points * 2):
        r = r_out if i % 2 == 0 else r_in
        t = math.radians(rot + 180 * i / points)
        pts.append(f"{cx + r * math.cos(t):.1f},{cy + r * math.sin(t):.1f}")
    return "M" + " L".join(pts) + " Z"


def postmark(small=False):
    """A: a cancellation postmark - ring + wavy lines. Black body.

    small=True is the hand-simplified 16/32px master (Apple redraws small
    sizes too): heavier ring, three straighter lines, no inner ring.
    """
    rnd = random.Random(82)
    ink = "#f4f1ea"
    cx, cy = 400, 512
    s = head(dark=True)
    if not small:
        # a light displacement so the stamp looks inked, not vector-perfect
        s = s.replace("    <clipPath id=\"clip\">", """    <filter id="ink" x="-5%" y="-5%" width="110%" height="110%">
      <feTurbulence type="fractalNoise" baseFrequency="0.035" numOctaves="2" seed="3"/>
      <feDisplacementMap in="SourceGraphic" scale="9" xChannelSelector="R" yChannelSelector="G"/>
    </filter>
    <clipPath id="clip">""")
        s += '    <g filter="url(#ink)">\n'
        s += f'    <circle cx="{cx}" cy="{cy}" r="196" fill="none" stroke="{ink}" stroke-width="40"/>\n'
        s += f'    <circle cx="{cx}" cy="{cy}" r="146" fill="none" stroke="{ink}" stroke-width="10"/>\n'
        s += f'    <path d="{star(cx, cy, 82, 34)}" fill="{ink}"/>\n'
        for k, y in enumerate((392, 472, 552, 632)):
            d = wavy(560, 1010, y, 15, 170, k * 0.6, rnd)
            s += f'    <path d="{d}" fill="none" stroke="{ink}" stroke-width="30" stroke-linecap="round"/>\n'
        s += "    </g>\n"
    else:
        s += f'    <circle cx="{cx}" cy="{cy}" r="186" fill="none" stroke="{ink}" stroke-width="72"/>\n'
        s += f'    <path d="{star(cx, cy, 92, 40)}" fill="{ink}"/>\n'
        for k, y in enumerate((412, 512, 612)):
            d = wavy(590, 1010, y, 10, 220, k * 0.6, rnd, jitter=0)
            s += f'    <path d="{d}" fill="none" stroke="{ink}" stroke-width="50" stroke-linecap="round"/>\n'
    return s + TAIL


def slip():
    """B: an ink-black dispatch slip in motion over a paper body."""
    s = head(dark=False)
    ink = "#111111"
    # motion lines trailing the slip
    for x0, x1, y in ((170, 330, 448), (130, 300, 528), (200, 320, 608)):
        s += f'    <path d="M{x0},{y} L{x1},{y}" stroke="{ink}" stroke-width="30" stroke-linecap="round"/>\n'
    s += '    <g transform="rotate(-9 590 520)" filter="url(#soft)">\n'
    s += f'      <rect x="365" y="360" width="460" height="320" rx="22" fill="{ink}"/>\n'
    # stamp: perforated square, top-right of slip
    perf = []
    sx, sy, sw = 690, 392, 100
    for i in range(6):
        o = 8 + i * (sw - 16) / 5
        perf += [(sx + o, sy), (sx + o, sy + sw), (sx, sy + o), (sx + sw, sy + o)]
    s += f'      <rect x="{sx}" y="{sy}" width="{sw}" height="{sw}" fill="#f4f1ea"/>\n'
    s += "".join(f'      <circle cx="{x:.1f}" cy="{y:.1f}" r="6" fill="{ink}"/>\n' for x, y in perf)
    s += f'      <circle cx="{sx + sw / 2}" cy="{sy + sw / 2}" r="22" fill="{ink}"/>\n'
    # "type": a heavy headline bar + mono body lines
    s += '      <rect x="405" y="410" width="230" height="34" rx="6" fill="#f4f1ea"/>\n'
    for y, w in ((520, 380), (566, 340), (612, 260)):
        s += f'      <rect x="405" y="{y}" width="{w}" height="16" rx="8" fill="#f4f1ea" opacity="0.8"/>\n'
    s += "    </g>\n"
    return s + TAIL


def outbox():
    """C: a page with an arrow breaking out of it. Black body."""
    paper, ink = "#f4f1ea", "#141414"
    s = head(dark=True)
    # page with a dog-eared top-right corner
    x0, y0, x1, y1, ear = 236, 300, 612, 800, 92
    s += (f'    <path d="M{x0},{y0} L{x1 - ear},{y0} L{x1},{y0 + ear} L{x1},{y1} L{x0},{y1} Z" '
          f'fill="{paper}"/>\n')
    s += f'    <path d="M{x1 - ear},{y0} L{x1 - ear},{y0 + ear} L{x1},{y0 + ear} Z" fill="#bdb8ad"/>\n'
    # type lines
    s += f'    <rect x="282" y="356" width="190" height="30" rx="6" fill="{ink}"/>\n'
    for y, w in ((432, 270), (476, 230), (520, 250), (564, 170)):
        s += f'    <rect x="282" y="{y}" width="{w}" height="14" rx="7" fill="{ink}" opacity="0.55"/>\n'
    # the arrow leaves the page: knockout stroke first, then the arrow
    arrow = "M430,700 L700,430"
    head_pts = "M590,370 L770,360 L760,540"
    for stroke, w in ((ink, 120), (paper, 64)):
        s += f'    <path d="{arrow}" stroke="{stroke}" stroke-width="{w}" stroke-linecap="round"/>\n'
        s += (f'    <path d="{head_pts}" fill="none" stroke="{stroke}" stroke-width="{w}" '
              f'stroke-linecap="round" stroke-linejoin="round"/>\n')
    return s + TAIL


if __name__ == "__main__":
    for name, fn in (("a-postmark", postmark), ("a-postmark-small", lambda: postmark(small=True)),
                     ("b-slip", slip), ("c-outbox", outbox)):
        with open(os.path.join(HERE, f"{name}.svg"), "w") as f:
            f.write(fn())
        print("wrote", name)
