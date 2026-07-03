#!/usr/bin/env python3
"""Generate the Soroban app icon — the script is the single source of truth;
the PNGs in AppIcon.appiconset are derived artifacts (gitignored), regenerated
by the XcodeGen pre-build phase.

Design: macOS rounded-rect tile, deep purple vertical gradient (the Dracula
theme's purple family), the kanji 算盤 ("soroban", the Japanese abacus the app
is named for) in white Hiragino Sans, with a faint abacus beam-and-beads motif
behind.

Idempotent: same inputs -> same outputs. Usage:

    python3 scripts/generate_icon.py            # writes the full appiconset
    python3 scripts/generate_icon.py out.png    # just the 1024 master
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO = Path(__file__).resolve().parent.parent
ICONSET = REPO / "swift/App/Resources/Assets.xcassets/AppIcon.appiconset"

# Apple's macOS icon grid: 1024 canvas, 824x824 tile centered (100px margin).
CANVAS = 1024
TILE = 824
MARGIN = (CANVAS - TILE) // 2
RADIUS = 186  # Big Sur rounded-rect radius at 1024

# Deep purple — the Dracula theme's purple family (#BD93F9), darkened so the
# white kanji keeps contrast at Dock sizes.
GRADIENT_TOP = (139, 98, 217)
GRADIENT_BOTTOM = (91, 60, 153)
TEXT_COLOR = (255, 255, 255, 255)
SHADOW_COLOR = (0, 0, 0, 110)
BEAM_COLOR = (255, 255, 255, 90)
BEAD_COLOR = (255, 255, 255, 70)

HIRAGINO = "/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc"


def hiragino(size: int) -> ImageFont.FreeTypeFont:
    """Load Hiragino Sans W6 from the .ttc, whichever face index it is."""
    for index in range(8):
        try:
            font = ImageFont.truetype(HIRAGINO, size, index=index)
        except OSError:
            break
        family, _ = font.getname()
        if "W6" in family or "Hiragino Sans" in family:
            return font
    # Fall back to face 0 — still Hiragino, worst case a sibling weight.
    return ImageFont.truetype(HIRAGINO, size, index=0)


def draw_master() -> Image.Image:
    img = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))

    # Tile: vertical gradient clipped to the rounded rect.
    gradient = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    gdraw = ImageDraw.Draw(gradient)
    for y in range(TILE):
        t = y / (TILE - 1)
        color = tuple(
            round(GRADIENT_TOP[c] + (GRADIENT_BOTTOM[c] - GRADIENT_TOP[c]) * t)
            for c in range(3)
        )
        gdraw.line(
            [(MARGIN, MARGIN + y), (MARGIN + TILE, MARGIN + y)],
            fill=color + (255,),
        )

    mask = Image.new("L", (CANVAS, CANVAS), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [MARGIN, MARGIN, MARGIN + TILE, MARGIN + TILE],
        radius=RADIUS,
        fill=255,
    )
    img.paste(gradient, (0, 0), mask)

    # Faint abacus motif: a horizontal reckoning beam with three beads,
    # echoing the favicon. Deliberately subtle so the kanji carries the icon.
    # Drawn on its own layer and alpha-composited — drawing translucent fills
    # straight onto an RGBA image OVERWRITES pixels (punching see-through
    # holes in the tile) instead of blending.
    motif = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    mdraw = ImageDraw.Draw(motif)
    beam_y = MARGIN + int(TILE * 0.30)
    mdraw.rounded_rectangle(
        [MARGIN + 110, beam_y - 10, MARGIN + TILE - 110, beam_y + 10],
        radius=10,
        fill=BEAM_COLOR,
    )
    for i in range(3):
        cx = MARGIN + TILE // 2 + (i - 1) * 170
        mdraw.ellipse(
            [cx - 52, beam_y - 34, cx + 52, beam_y + 34],
            fill=BEAD_COLOR,
        )
    img = Image.alpha_composite(img, motif)

    draw = ImageDraw.Draw(img)

    # The kanji, centered slightly low (optically centered against the beam).
    font = hiragino(330)
    text = "算盤"
    bbox = draw.textbbox((0, 0), text, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    tx = (CANVAS - tw) // 2 - bbox[0]
    ty = MARGIN + int(TILE * 0.56) - th // 2 - bbox[1]

    # Soft shadow first, then the glyphs.
    shadow = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).text((tx, ty + 12), text, font=font, fill=SHADOW_COLOR)
    shadow = shadow.filter(ImageFilter.GaussianBlur(14))
    img = Image.alpha_composite(img, shadow)
    ImageDraw.Draw(img).text((tx, ty), text, font=font, fill=TEXT_COLOR)

    return img


# (filename, point size, scale) — the full macOS icon set.
SIZES = [
    ("icon_16.png", 16, 1),
    ("icon_16@2x.png", 16, 2),
    ("icon_32.png", 32, 1),
    ("icon_32@2x.png", 32, 2),
    ("icon_128.png", 128, 1),
    ("icon_128@2x.png", 128, 2),
    ("icon_256.png", 256, 1),
    ("icon_256@2x.png", 256, 2),
    ("icon_512.png", 512, 1),
    ("icon_512@2x.png", 512, 2),
]


def main() -> None:
    master = draw_master()

    if len(sys.argv) > 1:  # single-file mode for previewing
        master.save(sys.argv[1])
        print(f"wrote {sys.argv[1]}")
        return

    ICONSET.mkdir(parents=True, exist_ok=True)
    for name, points, scale in SIZES:
        px = points * scale
        master.resize((px, px), Image.LANCZOS).save(ICONSET / name)
    print(f"wrote {len(SIZES)} icons to {ICONSET}")


if __name__ == "__main__":
    main()
