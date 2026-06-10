"""Regenerate VoxType app icons as square squircle tiles."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "app" / "src-tauri" / "icons"
SOURCE_IN = ICONS / "icon-source.png"
LEGACY_SOURCE = ICONS / "icon-legacy-wide.png"
MASTER_SOURCE = ICONS / "icon-source.png"

MASTER_SIZE = 1024
CORNER_RADIUS_RATIO = 0.223
BG_RGBA = (11, 15, 26, 255)
GRAPHIC_SCALE = 0.70  # mic graphic size relative to master canvas


def _alpha_bbox(im: Image.Image) -> tuple[int, int, int, int]:
    bbox = im.split()[3].getbbox()
    if bbox is None:
        raise RuntimeError("icon source has no visible content")
    return bbox


def _square_crop_center(im: Image.Image, side: int) -> Image.Image:
    w, h = im.size
    left = max(0, (w - side) // 2)
    top = max(0, (h - side) // 2)
    return im.crop((left, top, left + side, top + side))


def _draw_squircle(size: int, radius: int, fill: tuple[int, int, int, int]) -> Image.Image:
    tile = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(tile)
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=fill)
    return tile


def compose_square_icon(legacy: Image.Image, size: int = MASTER_SIZE) -> Image.Image:
    """Square squircle background with centered mic graphic (no wide pill letterboxing)."""
    x0, y0, x1, y1 = _alpha_bbox(legacy)
    pill_h = y1 - y0
    # Center square inside the legacy pill captures the mic without horizontal end caps.
    crop_side = min(pill_h, legacy.size[0])
    graphic = _square_crop_center(legacy, crop_side)

    radius = max(8, int(size * CORNER_RADIUS_RATIO))
    tile = _draw_squircle(size, radius, BG_RGBA)

    target = max(32, int(size * GRAPHIC_SCALE))
    graphic = graphic.resize((target, target), Image.Resampling.LANCZOS)
    offset = ((size - target) // 2, (size - target) // 2)
    tile.alpha_composite(graphic, offset)
    return tile


def write_png(path: Path, image: Image.Image, size: int) -> None:
    resized = image if image.size == (size, size) else image.resize(
        (size, size), Image.Resampling.LANCZOS
    )
    resized.save(path, format="PNG", optimize=True)


def write_ico(path: Path, image: Image.Image) -> None:
    sizes = [16, 24, 32, 48, 64, 128, 256]
    frames = [image.resize((s, s), Image.Resampling.LANCZOS) for s in sizes]
    frames[0].save(
        path,
        format="ICO",
        sizes=[(s, s) for s in sizes],
        append_images=frames[1:],
    )


def write_icns(path: Path, image: Image.Image) -> None:
    try:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            iconset = Path(tmp) / "AppIcon.iconset"
            iconset.mkdir()
            mapping = {
                "icon_16x16.png": 16,
                "icon_16x16@2x.png": 32,
                "icon_32x32.png": 32,
                "icon_32x32@2x.png": 64,
                "icon_128x128.png": 128,
                "icon_128x128@2x.png": 256,
                "icon_256x256.png": 256,
                "icon_256x256@2x.png": 512,
                "icon_512x512.png": 512,
                "icon_512x512@2x.png": 1024,
            }
            for name, sz in mapping.items():
                write_png(iconset / name, image, sz)
            subprocess.run(
                ["iconutil", "-c", "icns", str(iconset), "-o", str(path)],
                check=True,
                capture_output=True,
            )
    except (FileNotFoundError, subprocess.CalledProcessError):
        pass


def write_rgba_sizes(image: Image.Image) -> None:
    targets = {
        "icon.png": MASTER_SIZE,
        "32x32.png": 32,
        "64x64.png": 64,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "Square30x30Logo.png": 30,
        "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71,
        "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142,
        "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284,
        "Square310x310Logo.png": 310,
        "StoreLogo.png": 50,
    }
    for name, sz in targets.items():
        write_png(ICONS / name, image, sz)
    write_ico(ICONS / "icon.ico", image)
    write_icns(ICONS / "icon.icns", image)


def main() -> int:
    if LEGACY_SOURCE.is_file():
        legacy = Image.open(LEGACY_SOURCE).convert("RGBA")
    elif SOURCE_IN.is_file():
        legacy = Image.open(SOURCE_IN).convert("RGBA")
        legacy.save(LEGACY_SOURCE, format="PNG", optimize=True)
    else:
        print(f"Missing source icon: {LEGACY_SOURCE} or {SOURCE_IN}", file=sys.stderr)
        return 1

    master = compose_square_icon(legacy, MASTER_SIZE)
    master.save(MASTER_SOURCE, format="PNG", optimize=True)
    write_rgba_sizes(master)
    print(f"Wrote square squircle icons under {ICONS}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
