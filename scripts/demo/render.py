#!/usr/bin/env python3
"""Renders docs/images/demo.gif from demo.html.

Each frame is laid out by the page's render(t), screenshotted at 2x and
downsampled, then gifski encodes the lot. The last half second cross-fades
back into the first frame so the loop has no seam.

    pip install playwright pillow && playwright install chromium
    brew install gifski
    python3 scripts/demo/render.py
"""

import pathlib
import shutil
import subprocess
import sys
import tempfile

from PIL import Image
from playwright.sync_api import sync_playwright

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE.parent.parent / "docs" / "images" / "demo.gif"
FPS = 25
W, H = 960, 600
FADE = 0.5  # seconds of cross-fade back to frame 0


def main() -> None:
    if not shutil.which("gifski"):
        sys.exit("render.py: need gifski (brew install gifski)")

    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        tmp = pathlib.Path(tmp)
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": W, "height": H}, device_scale_factor=2)
        page.goto((HERE / "demo.html").as_uri())
        page.wait_for_load_state("networkidle")
        duration = page.evaluate("window.DURATION")

        frames = []
        total = int(round((duration + FADE) * FPS))
        first = None
        for i in range(total):
            t = i / FPS
            page.evaluate(f"render({min(t, duration)})")
            raw = tmp / "raw.png"
            page.screenshot(path=raw)
            img = Image.open(raw).convert("RGB").resize((W, H), Image.LANCZOS)
            if first is None:
                first = img
            if t > duration:
                img = Image.blend(img, first, min(1.0, (t - duration) / FADE))
            path = tmp / f"f{i:04d}.png"
            img.save(path)
            frames.append(str(path))
        browser.close()

        subprocess.run(
            ["gifski", "--fps", str(FPS), "--quality", "95", "--width", str(W),
             "-o", str(OUT), *frames],
            check=True,
        )
    print(f"{OUT} ({OUT.stat().st_size / 1e6:.1f} MB, {len(frames)} frames)")


if __name__ == "__main__":
    main()
