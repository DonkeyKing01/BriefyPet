from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageEnhance, ImageFilter


ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIR = ROOT / "public" / "pets" / "briefy-ip" / "source"
MAIN_DIR = ROOT / "public" / "pets" / "briefy-ip" / "main"
MINI_DIR = ROOT / "public" / "pets" / "briefy-ip" / "mini"

CANVAS = (220, 220)
FRAME_COUNT = 12
FRAME_DURATION = 95
PIXEL_SCALE = 4

TEAL = (20, 95, 117, 255)
TEAL_DARK = (14, 72, 89, 255)
TEAL_LIGHT = (111, 223, 235, 255)
CREAM = (246, 239, 222, 255)
INK = (31, 34, 35, 255)
RED = (230, 73, 69, 255)
SOFT_GRAY = (124, 136, 142, 255)


def isolate_subject(path: Path, threshold: int = 244) -> Image.Image:
    image = Image.open(path).convert("RGBA")
    pixels = image.load()
    for y in range(image.height):
        for x in range(image.width):
            r, g, b, a = pixels[x, y]
            if r >= threshold and g >= threshold and b >= threshold:
                pixels[x, y] = (255, 255, 255, 0)

    bbox = image.getbbox()
    if not bbox:
        return image

    cropped = image.crop(bbox)
    alpha = cropped.getchannel("A")
    alpha = ImageEnhance.Contrast(alpha).enhance(1.15)
    cropped.putalpha(alpha)
    return cropped


def quantize_subject(subject: Image.Image) -> Image.Image:
    reduced = subject.convert("P", palette=Image.Palette.ADAPTIVE, colors=6)
    return reduced.convert("RGBA")


def step(value: float, unit: float = 1.0) -> float:
    return round(value / unit) * unit


def compose_subject(
    subject: Image.Image,
    *,
    x: float = 0.0,
    y: float = 0.0,
    scale: float = 1.0,
    rotate_deg: float = 0.0,
    max_ratio: float = 0.67,
) -> Image.Image:
    frame = Image.new("RGBA", CANVAS, (0, 0, 0, 0))
    sprite = subject.resize(
        (max(1, int(subject.width * scale)), max(1, int(subject.height * scale))),
        Image.Resampling.LANCZOS,
    )

    limit = (int(CANVAS[0] * max_ratio), int(CANVAS[1] * max_ratio))
    if sprite.width > limit[0] or sprite.height > limit[1]:
        sprite.thumbnail(limit, Image.Resampling.LANCZOS)

    if rotate_deg:
        sprite = sprite.rotate(rotate_deg, resample=Image.Resampling.BICUBIC, expand=True)

    left = int((CANVAS[0] - sprite.width) / 2 + x)
    top = int((CANVAS[1] - sprite.height) / 2 + y)
    frame.alpha_composite(sprite, (left, top))
    return frame


def draw_ground(draw: ImageDraw.ImageDraw, *, center_x: int = 110, width: int = 64, alpha: int = 32) -> None:
    draw.ellipse((center_x - width // 2, 188, center_x + width // 2, 200), fill=(0, 0, 0, alpha))


def draw_ping(
    draw: ImageDraw.ImageDraw,
    *,
    center: tuple[float, float],
    progress: float,
    radius: float = 12,
    color: tuple[int, int, int, int] = TEAL_LIGHT,
    arcs: int = 2,
) -> None:
    for index in range(arcs):
        r = radius + index * 10 + 4 * progress
        alpha = int(color[3] * max(0.2, 0.75 - index * 0.2))
        box = (center[0] - r, center[1] - r, center[0] + r, center[1] + r)
        draw.arc(box, start=-38, end=218, fill=(color[0], color[1], color[2], alpha), width=3)


def draw_feed_card(
    draw: ImageDraw.ImageDraw,
    *,
    x: int,
    y: int,
    width: int = 78,
    height: int = 52,
    beam_x: float | None = None,
    accent: tuple[int, int, int, int] = TEAL_LIGHT,
) -> None:
    draw.rounded_rectangle((x, y, x + width, y + height), radius=12, fill=(246, 249, 250, 230))
    draw.rounded_rectangle((x, y, x + width, y + height), radius=12, outline=(22, 95, 117, 90), width=2)
    draw.line((x + 12, y + 16, x + width - 14, y + 16), fill=(22, 95, 117, 170), width=4)
    draw.line((x + 12, y + 28, x + width - 20, y + 28), fill=(22, 95, 117, 120), width=4)
    draw.line((x + 12, y + 40, x + width - 28, y + 40), fill=(22, 95, 117, 90), width=4)
    if beam_x is not None:
        draw.rounded_rectangle(
            (beam_x - 8, y + 6, beam_x + 8, y + height - 6),
            radius=6,
            fill=(accent[0], accent[1], accent[2], 72),
        )


def draw_envelope(draw: ImageDraw.ImageDraw, *, x: int, y: int, pulse: float) -> None:
    lift = int(2 * pulse)
    draw.rounded_rectangle((x, y - lift, x + 36, y + 24 - lift), radius=8, fill=(250, 251, 252, 240))
    draw.rounded_rectangle((x, y - lift, x + 36, y + 24 - lift), radius=8, outline=(0, 0, 0, 80), width=2)
    draw.line((x + 4, y + 4 - lift, x + 18, y + 14 - lift), fill=RED, width=3)
    draw.line((x + 32, y + 4 - lift, x + 18, y + 14 - lift), fill=RED, width=3)


def draw_spinner(draw: ImageDraw.ImageDraw, *, center: tuple[int, int], progress: float) -> None:
    for index in range(8):
        angle = index * 45
        active = int((progress * 8 + index) % 8)
        alpha = 70 + ((7 - active) * 18)
        r1, r2 = 10, 18
        x1 = center[0] + r1 * math.cos(math.radians(angle))
        y1 = center[1] + r1 * math.sin(math.radians(angle))
        x2 = center[0] + r2 * math.cos(math.radians(angle))
        y2 = center[1] + r2 * math.sin(math.radians(angle))
        draw.line((x1, y1, x2, y2), fill=(TEAL_LIGHT[0], TEAL_LIGHT[1], TEAL_LIGHT[2], alpha), width=3)


def draw_sleep_marks(draw: ImageDraw.ImageDraw, *, progress: float) -> None:
    x = 150 + int(2 * math.sin(progress * math.tau))
    y = 58 - int(5 * progress)
    draw.text((x, y), "z", fill=SOFT_GRAY)
    draw.text((x + 10, y - 10), "z", fill=SOFT_GRAY)


def draw_broken_ring(draw: ImageDraw.ImageDraw, *, center: tuple[int, int], progress: float) -> None:
    wobble = int(2 * math.sin(progress * math.tau))
    r = 12
    draw.arc((center[0] - r, center[1] - r + wobble, center[0] + r, center[1] + r + wobble), 30, 155, fill=RED, width=3)
    draw.arc((center[0] - r, center[1] - r + wobble, center[0] + r, center[1] + r + wobble), 205, 330, fill=RED, width=3)
    draw.line((center[0] - 2, center[1] - 4 + wobble, center[0] + 6, center[1] + 6 + wobble), fill=RED, width=3)


def stylize(frame: Image.Image) -> Image.Image:
    softened = frame.filter(ImageFilter.GaussianBlur(0.15))
    reduced = softened.convert("P", palette=Image.Palette.ADAPTIVE, colors=10).convert("RGBA")
    return reduced


def render_main_frames(subject: Image.Image, mode: str) -> list[Image.Image]:
    frames: list[Image.Image] = []
    for index in range(FRAME_COUNT):
        t = index / FRAME_COUNT
        pulse = 0.5 + 0.5 * math.sin(t * math.tau)
        bob = step(2.5 * math.sin(t * math.tau), 0.8)
        overlay = Image.new("RGBA", CANVAS, (0, 0, 0, 0))
        draw = ImageDraw.Draw(overlay)

        x = 0.0
        y = bob
        rotate = 0.0
        scale = 1.0
        ground_x = 110

        if mode == "idle":
            x = 42
            draw_sleep_marks(draw, progress=t)
            draw_ping(draw, center=(145, 50), progress=pulse * 0.6, radius=9, arcs=1, color=(111, 223, 235, 120))
            ground_x = 152
        elif mode == "polling":
            x = 30
            card_y = 94 + int(2 * math.sin(t * math.tau))
            draw_feed_card(draw, x=28, y=card_y, width=76, height=50)
            draw_ping(draw, center=(143, 48), progress=pulse, radius=9, arcs=2)
            ground_x = 144
        elif mode == "scanning":
            x = -18
            beam = 128 + 48 * t
            draw_feed_card(draw, x=118, y=84, width=78, height=58, beam_x=beam)
            draw.line((112, 98, 122, 98), fill=TEAL_LIGHT, width=3)
            draw_ping(draw, center=(94, 50), progress=pulse, radius=11, arcs=2, color=(111, 223, 235, 170))
            ground_x = 92
        elif mode == "loading":
            y = bob - 2
            draw_spinner(draw, center=(110, 186), progress=t)
        elif mode == "needs-config":
            x = 60
            y = bob + 10
            rotate = -4
            draw_broken_ring(draw, center=(154, 44), progress=t)
            draw.rounded_rectangle((126, 68, 160, 100), radius=11, fill=(247, 247, 247, 238))
            draw.text((138, 74), "?", fill=SOFT_GRAY)
            ground_x = 164
        elif mode == "new-info":
            x = -6
            draw_ping(draw, center=(110, 42), progress=pulse, radius=12, arcs=3, color=(111, 223, 235, 190))
            draw_envelope(draw, x=148, y=82, pulse=pulse)
            badge_r = 7 + int(2 * pulse)
            draw.ellipse((112 - badge_r, 30 - badge_r, 112 + badge_r, 30 + badge_r), fill=RED)
            ground_x = 108

        draw_ground(draw, center_x=ground_x)

        frame = compose_subject(subject, x=x, y=y, scale=scale, rotate_deg=rotate)
        frame = Image.alpha_composite(overlay, frame)
        frames.append(stylize(frame))
    return frames


def pixelate(frame: Image.Image) -> Image.Image:
    mini = frame.resize((CANVAS[0] // PIXEL_SCALE, CANVAS[1] // PIXEL_SCALE), Image.Resampling.NEAREST)
    return mini.resize(CANVAS, Image.Resampling.NEAREST)


def render_mini_frames(subject: Image.Image, mode: str) -> list[Image.Image]:
    frames: list[Image.Image] = []
    for index in range(FRAME_COUNT):
        t = index / FRAME_COUNT
        overlay = Image.new("RGBA", CANVAS, (0, 0, 0, 0))
        draw = ImageDraw.Draw(overlay)
        x = 40 if mode == "idle" else 18
        y = step(2.0 * math.sin(t * math.tau), 1.0)
        if mode == "idle":
            draw_sleep_marks(draw, progress=t)
        else:
            draw_ping(draw, center=(108, 42), progress=0.5 + 0.5 * math.sin(t * math.tau), radius=10, arcs=2, color=(111, 223, 235, 170))
            draw_envelope(draw, x=148, y=92, pulse=0.5 + 0.5 * math.sin(t * math.tau))
            draw.ellipse((112, 32, 124, 44), fill=RED)
        draw_ground(draw, center_x=144 if mode == "idle" else 116, width=54, alpha=26)
        frame = compose_subject(subject, x=x, y=y, max_ratio=0.5)
        frame = Image.alpha_composite(overlay, frame)
        frames.append(pixelate(stylize(frame)))
    return frames


def to_gif_frame(frame: Image.Image) -> Image.Image:
    palette_frame = frame.convert("P", palette=Image.Palette.ADAPTIVE, colors=255)
    alpha = frame.getchannel("A")
    mask = Image.eval(alpha, lambda px: 255 if px <= 8 else 0)
    palette_frame.paste(255, mask)
    palette = palette_frame.getpalette()
    if palette is not None:
        palette[255 * 3 : 255 * 3 + 3] = [0, 0, 0]
        palette_frame.putpalette(palette)
    palette_frame.info["transparency"] = 255
    palette_frame.info["disposal"] = 2
    return palette_frame


def save_gif(frames: list[Image.Image], output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    converted = [to_gif_frame(frame) for frame in frames]
    converted[0].save(
        output_path,
        save_all=True,
        append_images=converted[1:],
        loop=0,
        duration=FRAME_DURATION,
        optimize=False,
        transparency=255,
        disposal=2,
    )


def export_main(subject: Image.Image) -> None:
    for mode in ("loading", "needs-config", "polling", "scanning", "idle", "new-info"):
        save_gif(render_main_frames(subject, mode), MAIN_DIR / f"briefy-{mode}.gif")


def export_mini(subject: Image.Image) -> None:
    save_gif(render_mini_frames(subject, "idle"), MINI_DIR / "briefy-mini-idle.gif")
    save_gif(render_mini_frames(subject, "new-info"), MINI_DIR / "briefy-mini-new-info.gif")


def main() -> None:
    subject = quantize_subject(isolate_subject(SOURCE_DIR / "briefy-core.png"))
    export_main(subject)
    export_mini(subject)


if __name__ == "__main__":
    main()
