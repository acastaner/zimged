#!/usr/bin/env python3
"""Rebuild the Windows and macOS icons from packaging/linux/zimged.svg (#42).

    packaging/make_icons.py

Writes packaging/windows/zimged.ico and packaging/macos/zimged.icns. Both are committed, so the
release jobs need no SVG renderer; run this again whenever the SVG changes. Needs `rsvg-convert`
(librsvg) and Pillow, for instance in a container:

    docker run --rm -v "$PWD:/w" -w /w debian:bookworm-slim sh -c \
      'apt-get update && apt-get install -y librsvg2-bin python3-pil && python3 packaging/make_icons.py'
"""

import io
import subprocess
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent
SVG = ROOT / "linux" / "zimged.svg"
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICNS_SIZES = [16, 32, 64, 128, 256, 512, 1024]


def render(size: int) -> Image.Image:
    png = subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), str(SVG)],
        check=True,
        capture_output=True,
    ).stdout
    return Image.open(io.BytesIO(png)).convert("RGBA")


def main() -> None:
    # Each size is rendered from the SVG rather than scaled down from the largest one.
    ico = [render(size) for size in ICO_SIZES]
    out = ROOT / "windows" / "zimged.ico"
    out.parent.mkdir(exist_ok=True)
    ico[-1].save(out, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=ico[:-1])

    icns = [render(size) for size in ICNS_SIZES]
    out = ROOT / "macos" / "zimged.icns"
    out.parent.mkdir(exist_ok=True)
    icns[-1].save(out, format="ICNS", append_images=icns[:-1])


if __name__ == "__main__":
    main()
