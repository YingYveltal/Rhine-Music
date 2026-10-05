#!/usr/bin/env python3
"""Create 16 synthetic albums for ordinary local-library rendering QA.

No downloads, user assets, app preferences, or production code changes. The new
output directory contains numbered PNG test patterns and one second of silence
per album so the normal music-folder scanner can import it.
"""
import argparse
import colorsys
import hashlib
import json
from pathlib import Path
import struct
import wave
import zlib

DIGITS = (
    ('11111', '10001', '10011', '10101', '11001', '10001', '11111'),
    ('00100', '01100', '00100', '00100', '00100', '00100', '01110'),
    ('11111', '00001', '00001', '11111', '10000', '10000', '11111'),
    ('11111', '00001', '00001', '01111', '00001', '00001', '11111'),
    ('10001', '10001', '10001', '11111', '00001', '00001', '00001'),
    ('11111', '10000', '10000', '11111', '00001', '00001', '11111'),
    ('11111', '10000', '10000', '11111', '10001', '10001', '11111'),
    ('11111', '00001', '00010', '00100', '01000', '01000', '01000'),
    ('11111', '10001', '10001', '11111', '10001', '10001', '11111'),
    ('11111', '10001', '10001', '11111', '00001', '00001', '11111'),
)
SHAPES = (('square', 600, 600), ('portrait', 450, 600), ('landscape', 600, 450))


def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))


def cover(number, width, height):
    color = bytes(round(v * 255) for v in colorsys.hsv_to_rgb((number - 1) / 16, .75, .85))
    pixels = bytearray(color * width * height)

    def rect(x, y, w, h, rgb):
        for row in range(y, y + h):
            at = (row * width + x) * 3
            pixels[at:at + w * 3] = bytes(rgb) * w

    white, black = (255, 255, 255), (12, 12, 12)
    for x, y, w, h in [(8, 8, width - 16, 5), (8, height - 13, width - 16, 5),
                        (8, 8, 5, height - 16), (width - 13, 8, 5, height - 16)]:
        rect(x, y, w, h, white)
    unit = min(width, height) // 20
    left, top = (width - 11 * unit) // 2, (height - 7 * unit) // 2
    rect(left - unit, top - unit, 13 * unit, 9 * unit, black)
    for digit_index, digit in enumerate(f'{number:02}'):
        for row, bits in enumerate(DIGITS[int(digit)]):
            for col, bit in enumerate(bits):
                if bit == '1':
                    rect(left + (6 * digit_index + col) * unit, top + row * unit, unit, unit, white)
    # Asymmetric orientation marks and five binary ID bars expose wrong flips,
    # cropping, or a stale cover independently of hue.
    rect(20, 20, 30, 30, white)
    rect(width - 50, 20, 30, 30, black)
    for bit in range(5):
        rect(24 + bit * 42, height - 48, 28, 20, white if number & (1 << bit) else black)
    rows = b''.join(b'\0' + pixels[y * width * 3:(y + 1) * width * 3] for y in range(height))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
    return png + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path, help='New directory; existing paths are never overwritten')
    root = parser.parse_args().output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    albums = []
    for number in range(1, 17):
        shape, width, height = SHAPES[(number - 1) % len(SHAPES)]
        title = f'QA {number:02} {shape}'
        folder = root / title
        folder.mkdir()
        png = cover(number, width, height)
        (folder / 'cover.png').write_bytes(png)
        with wave.open(str(folder / f'QA {number:02} silence.wav'), 'wb') as audio:
            audio.setnchannels(1)
            audio.setsampwidth(2)
            audio.setframerate(16000)
            audio.writeframes(bytes(32000))
        albums.append({'number': number, 'title': title, 'shape': shape,
                       'cover': f'{title}/cover.png', 'width': width, 'height': height,
                       'sha256': hashlib.sha256(png).hexdigest()})
    (root / 'fixture-manifest.json').write_text(json.dumps({'kind': 'synthetic-rendering-qa',
        'albums': albums, 'audio': '1 second of zero-valued mono PCM per album; no playback required'}, indent=2) + '\n')
    print('Created 16 numbered PNG covers and 16 silent WAV scanner fixtures in', root)


if __name__ == '__main__':
    main()
