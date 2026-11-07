"""Generate Memex app icon (256×256 RGBA PNG + multi-size ICO).

Run:  python assets/gen_icon.py
Output: assets/memex_icon.png, assets/memex_icon.ico
"""
from PIL import Image, ImageDraw
import math

W, H = 256, 256
img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# Rounded rectangle background — indigo (#4F46E5)
d.rounded_rectangle([4, 4, W - 5, H - 5], radius=48, fill=(79, 70, 229, 255))

# Document shape (white, centered)
dx1, dy1, dx2, dy2 = 72, 56, 184, 200
d.rounded_rectangle([dx1, dy1, dx2, dy2 - 8], radius=8, fill=(255, 255, 255, 255))
# Folded corner
fold = 28
d.polygon([(dx2 - fold, dy1), (dx2, dy1 + fold), (dx2 - fold, dy1 + fold)],
          fill=(199, 210, 254, 255))

# Text lines on document
lc = (129, 140, 248, 200)
for i, y in enumerate([dy1 + 52, dy1 + 72, dy1 + 92, dy1 + 112]):
    w = [80, 70, 90, 60][i]
    d.rounded_rectangle([dx1 + 16, y, dx1 + 16 + w, y + 5], radius=2, fill=lc)

# Magnifying glass (bottom-right)
mcx, mcy, mr = 178, 178, 26
d.ellipse([mcx - mr, mcy - mr, mcx + mr, mcy + mr],
          fill=(255, 255, 255, 255), outline=(79, 70, 229, 255), width=5)
ha = math.radians(45)
d.line([mcx + (mr - 2) * math.cos(ha), mcy + (mr - 2) * math.sin(ha),
        mcx + (mr + 16) * math.cos(ha), mcy + (mr + 16) * math.sin(ha)],
       fill=(79, 70, 229, 255), width=7)

img.save("assets/memex_icon.png")
img.save("assets/memex_icon.ico", format="ICO",
         sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)])
print("Icon saved: PNG + ICO (6 sizes)")
