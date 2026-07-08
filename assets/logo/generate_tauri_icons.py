from PIL import Image
import os

src = Image.open("d:/0 code/ocg-manager/design/logo-concepts/ocg_logo_final_transparent.png").convert("RGBA")

max_dim = max(src.width, src.height)
square = Image.new("RGBA", (max_dim, max_dim), (0, 0, 0, 0))
x = (max_dim - src.width) // 2
y = (max_dim - src.height) // 2
square.paste(src, (x, y), src)

icons_dir = "d:/0 code/ocg-manager/src-tauri/icons"
os.makedirs(icons_dir, exist_ok=True)

sizes = [32, 128, 256, 512]
for size in sizes:
    resized = square.resize((size, size), Image.LANCZOS)
    resized.save(os.path.join(icons_dir, f"{size}x{size}.png"))
    print(f"Saved {size}x{size}.png")

ico_sizes = [16, 24, 32, 48, 64, 128, 256]
frames = [square.resize((s, s), Image.LANCZOS) for s in ico_sizes]
frames[0].save(
    os.path.join(icons_dir, "icon.ico"),
    save_all=True,
    append_images=frames[1:],
    sizes=[(s, s) for s in ico_sizes]
)
print("Saved icon.ico")
