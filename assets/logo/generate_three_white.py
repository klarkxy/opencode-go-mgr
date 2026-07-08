from PIL import Image
import os

W, H = 1200, 1200
BG = (18, 18, 18)

dark = Image.open("d:/0 code/ocg-manager/design/logo-concepts/opencode-dark.png").convert("RGBA")

scale = 380 / dark.width
new_w = int(dark.width * scale)
new_h = int(dark.height * scale)
card = dark.resize((new_w, new_h), Image.LANCZOS)

cx, cy = W // 2, H // 2
base_x = cx - new_w // 2
base_y = cy - new_h // 2

layers = [
    (-240, -180, 0.70),
    (-120, -90, 0.85),
    (0, 0, 1.00),
]

img = Image.new("RGBA", (W, H), BG + (255,))

for dx, dy, alpha in layers:
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    c = card.copy()
    if alpha < 1.0:
        r, g, b, a = c.split()
        a = a.point(lambda i: int(i * alpha))
        c = Image.merge("RGBA", (r, g, b, a))
    layer.paste(c, (base_x + dx, base_y + dy), c)
    img = Image.alpha_composite(img, layer)

out_path = "d:/0 code/ocg-manager/design/logo-concepts/ocg_logo_three_white.png"
img.save(out_path)
print(f"Saved: {out_path}")
