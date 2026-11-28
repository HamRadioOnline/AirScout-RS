import hashlib
import math
import re
import struct
import unittest
import zlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


ICONS = ROOT / "packaging/icons"
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
SIZES = {16, 32, 48, 64, 128, 256}
APPLICATION_ICONS = (
    ("AirScout-RS", "hicolor", "airscout", "bottom-right"),
    ("AirScout-RS-PlaneServer", "hicolor-planeserver", "airscout-planeserver", "top-left"),
)


def ico_frames(data):
    reserved, kind, count = struct.unpack_from("<HHH", data)
    if (reserved, kind) != (0, 1):
        raise ValueError("Invalid ICO header")

    frames = []
    directory_end = 6 + count * 16
    for step in range(6, directory_end, 16):
        width, height, _, _, _, _, length, offset = struct.unpack_from("<BBBBHHII", data, step)
        if not length or offset < directory_end or offset + length > len(data):
            raise ValueError("Invalid ICO frame bounds")
        frames.append((width or 256, height or 256, data[offset:offset + length]))
    return frames


def rgba_png(data):
    # These fixtures and the generated icons are non-interlaced 8-bit RGBA.
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError("Invalid PNG signature")
    width, height, depth, color, compression, filtering, interlace = struct.unpack_from(
        ">IIBBBBB", data, 16
    )
    if (depth, color, compression, filtering, interlace) != (8, 6, 0, 0, 0):
        raise ValueError("Expected an 8-bit RGBA PNG")

    compressed = bytearray()
    offset = len(PNG_SIGNATURE)
    while offset < len(data):
        length, kind = struct.unpack_from(">I4s", data, offset)
        if kind == b"IDAT":
            compressed.extend(data[offset + 8:offset + 8 + length])
        offset += length + 12

    raw = zlib.decompress(compressed)
    stride = width * 4
    if len(raw) != height * (stride + 1):
        raise ValueError("Unexpected PNG scanline length")
    pixels = bytearray()
    prior = bytearray(stride)
    for offset in range(0, len(raw), stride + 1):
        kind = raw[offset]
        row = bytearray(raw[offset + 1:offset + 1 + stride])
        if kind:
            for i in range(stride):
                left = row[i - 4] if i >= 4 else 0
                above = prior[i]
                upper_left = prior[i - 4] if i >= 4 else 0
                if kind == 1:
                    predictor = left
                elif kind == 2:
                    predictor = above
                elif kind == 3:
                    predictor = (left + above) // 2
                elif kind == 4:
                    estimate = left + above - upper_left
                    predictor = min((left, above, upper_left), key=lambda value: abs(estimate - value))
                else:
                    raise ValueError("Invalid PNG row filter")
                row[i] = (row[i] + predictor) & 255
        pixels.extend(row)
        prior = row
    return width, height, bytes(pixels)


def rgba_pixels(pixels):
    return zip(pixels[0::4], pixels[1::4], pixels[2::4], pixels[3::4])


def is_near_black(pixel):
    return max(pixel[:3]) < 40 and pixel[3] > 200


class IconTests(unittest.TestCase):
    def test_no_upstream_art(self):
        for name in ("Main.ico", "PlaneServer.ico"):
            with self.subTest(name=name):
                self.assertFalse((ICONS / name).exists())

        notice = (ROOT / "NOTICE").read_text(encoding="utf-8")
        for obsolete in ("AirScout/Main.ico", "PlaneServer.ico", "IconBlock"):
            with self.subTest(obsolete=obsolete):
                self.assertNotIn(obsolete, notice)

    def test_icon_sources_present(self):
        for name in ("airscout", "airscout-small", "planeserver", "planeserver-small"):
            with self.subTest(name=name):
                self.assertTrue((ICONS / "src" / f"{name}.svg").is_file())
        self.assertTrue((ICONS / "rust-logo.svg").is_file())

    def test_logo_checksum(self):
        script = (ICONS / "generate.sh").read_text(encoding="utf-8")
        pin = re.search(r"^rust_logo_sha256=([a-f0-9]{64})$", script, re.MULTILINE)
        self.assertIsNotNone(pin)
        self.assertEqual(hashlib.sha256((ICONS / "rust-logo.svg").read_bytes()).hexdigest(), pin[1])

    def test_badged_icos(self):
        for name, png_dir, _, _ in APPLICATION_ICONS:
            with self.subTest(name=name):
                frames = ico_frames((ICONS / f"{name}.ico").read_bytes())
                self.assertCountEqual([(w, h) for w, h, _ in frames], [(size, size) for size in SIZES])
                png = next(frame for w, _, frame in frames if w == 256)
                self.assertTrue(png.startswith(PNG_SIGNATURE))
                self.assertEqual(png, (ICONS / png_dir / "256.png").read_bytes())

    def test_badge_presence(self):
        for name, png_dir, _, corner in APPLICATION_ICONS:
            with self.subTest(name=name):
                width, height, pixels = rgba_png((ICONS / png_dir / "256.png").read_bytes())
                self.assertEqual((width, height), (256, 256))
                span = round(width * 0.46)
                start = width - span if corner == "bottom-right" else 0
                badge = [
                    pixel
                    for row in range(start, start + span)
                    for pixel in rgba_pixels(
                        pixels[(row * width + start) * 4:(row * width + start + span) * 4]
                    )
                ]
                self.assertGreater(sum(map(is_near_black, badge)), len(badge) * 0.02)
                self.assertIn((255, 255, 255, 255), badge)

    def test_badge_has_five_holes(self):
        for name, png_dir, _, corner in APPLICATION_ICONS:
            width, _, pixels = rgba_png((ICONS / png_dir / "256.png").read_bytes())
            span = round(width * 0.46)
            outline = max(1, round(width * 0.01))
            start = width - span - outline if corner == "bottom-right" else outline
            center = start + span / 2
            scale = span / 106

            def logo_pixel(x, y):
                column = int(center + x * scale)
                row = int(center + y * scale)
                offset = (row * width + column) * 4
                return pixels[offset:offset + 4]

            # Use the original Rust SVG's centered 106 × 106 coordinate system.
            for degrees in range(0, 360, 72):
                angle = math.radians(degrees)
                with self.subTest(name=name, hole_degrees=degrees):
                    pixel = logo_pixel(40 * math.sin(angle), -40 * math.cos(angle))
                    self.assertFalse(is_near_black(pixel), tuple(pixel))

            angle = math.radians(36)
            with self.subTest(name=name, feature="gear ring"):
                self.assertTrue(is_near_black(logo_pixel(43 * math.sin(angle), -43 * math.cos(angle))))
            with self.subTest(name=name, feature="R stem"):
                self.assertTrue(is_near_black(logo_pixel(-18, -6)))

    def test_small_sizes_no_badge(self):
        for name, png_dir, crate, _ in APPLICATION_ICONS:
            for size in (16, 32):
                with self.subTest(name=name, size=size):
                    width, height, pixels = rgba_png((ICONS / png_dir / f"{size}.png").read_bytes())
                    self.assertEqual((width, height), (size, size))
                    self.assertLess(sum(map(is_near_black, rgba_pixels(pixels))), size * size * 0.02)

            with self.subTest(crate=crate):
                rgba = (ROOT / "crates" / crate / "assets/app-icon.rgba").read_bytes()
                self.assertLess(sum(map(is_near_black, rgba_pixels(rgba))), 32 * 32 * 0.02)

    def test_icon_assets(self):
        for name, png_dir, crate, _ in APPLICATION_ICONS:
            with self.subTest(crate=crate):
                rgba = (ROOT / "crates" / crate / "assets/app-icon.rgba").read_bytes()
                alpha = rgba[3::4]
                self.assertEqual(len(rgba), 32 * 32 * 4)
                self.assertIn(0, alpha)
                self.assertTrue(any(alpha))

            with self.subTest(png_dir=png_dir):
                png_sizes = set()
                for path in (ICONS / png_dir).glob("*.png"):
                    with path.open("rb") as png:
                        self.assertEqual(png.read(16), PNG_SIGNATURE + b"\x00\x00\x00\x0dIHDR")
                        width, height = struct.unpack(">II", png.read(8))
                    self.assertEqual((width, height), (int(path.stem), int(path.stem)))
                    png_sizes.add(width)
                self.assertEqual(png_sizes, SIZES)

            with self.subTest(name=name):
                self.assertTrue((ICONS / f"{name}.icns").read_bytes().startswith(b"icns"))

    def test_rust_logo_notice(self):
        notice = (ROOT / "NOTICE").read_text(encoding="utf-8")
        self.assertIn("rust-lang/rust-artwork", notice)
        self.assertIn("CC-BY 4.0", notice)
        self.assertIn(
            "Rust and the Rust logo are trademarks of the Rust Foundation; the logo is used unmodified "
            "to indicate that this program is written in Rust.",
            " ".join(notice.split()),
        )

    def test_own_art_notice(self):
        notice = (ROOT / "NOTICE").read_text(encoding="utf-8")
        self.assertIn(
            "The AirScout-RS and PlaneServer application icons are original artwork of the "
            "AirScout-RS project (HamRadioOnline) and are distributed under GPL-3.0-only. "
            "The SVG sources are in packaging/icons/src.",
            " ".join(notice.split()),
        )
