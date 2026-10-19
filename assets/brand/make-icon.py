from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets" / "app-icon"
NAME = "jianyingcraft"
APP_ID = "ai.jianyingcraft"
W = 1024
top = (34, 211, 238)
bottom = (124, 92, 255)

def render(size: int) -> Image.Image:
    grad = Image.new("RGB", (W, W))
    px = grad.load()
    for y in range(W):
        for x in range(W):
            t = (x + y) / (2 * (W - 1))
            px[x, y] = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, W - 1], radius=224, fill=255)
    icon = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    icon.paste(grad, (0, 0), mask)
    d = ImageDraw.Draw(icon)
    ink = (12, 14, 18, 255)
    for x0, y0, x1, y1 in [(300, 592, 396, 760), (464, 300, 560, 760), (628, 452, 724, 760)]:
        d.rounded_rectangle([x0, y0, x1, y1], radius=28, fill=ink)
    d.rounded_rectangle([300, 800, 724, 836], radius=18, fill=ink)
    return icon if size == W else icon.resize((size, size), Image.LANCZOS)

master = render(W)
master.save(ASSETS / f"{NAME}-1024.png", "PNG", optimize=True)
master.resize((512, 512), Image.LANCZOS).save(ASSETS / f"{NAME}-512.png", "PNG", optimize=True)
master.save(ASSETS / f"{NAME}.ico", format="ICO", sizes=[(16, 16), (20, 20), (24, 24), (32, 32), (40, 40), (48, 48), (64, 64), (128, 128), (256, 256)])
master.save(ASSETS / f"{NAME}.icns", format="ICNS")
# Linux hicolor theme keeps the packaged app id (`ai.jianyingcraft`) until the
# packaging/bundle-id migration lands; only the artwork changes here.
for s in (16, 24, 32, 48, 64, 128, 256, 512):
    folder = ASSETS / "hicolor" / f"{s}x{s}" / "apps"
    folder.mkdir(parents=True, exist_ok=True)
    render(s).save(folder / "ai.jianyingcraft.png", "PNG", optimize=True)
scalable = ASSETS / "hicolor" / "scalable" / "apps"
scalable.mkdir(parents=True, exist_ok=True)
(scalable / "ai.jianyingcraft.svg").write_text(
    (ROOT / "assets" / "brand" / "jianyingcraft.svg").read_text(encoding="utf-8"), encoding="utf-8"
)
master.resize((128, 128), Image.LANCZOS).save(ROOT / "apps" / "filmcraft-web" / "web" / "favicon.png", "PNG", optimize=True)
print("icons written")