#!/usr/bin/env python3
"""Generated Dragon Slayer floppies, translated by the original dsretrack.py, for the
Rust port's test (src/dsretrack.rs): both must give the same bytes (or both nothing).

    python tests/dsretrack/compare.py <patch archive.zip> <work dir> [count]
    DSRETRACK_CASES=<work dir> cargo test --release -- --ignored same_bytes

Each floppy holds the game's real English texts at random places of the data
tracks, pointed at from the program tracks the way the three versions do
(chains, movable lines, pointers inside texts), with random gaps (merged spaces
or not) and random noise.
"""
import json
import random
import shutil
import struct
import subprocess
import sys
import zipfile
from pathlib import Path

FORMATS = {
    "1.1": dict(chain=b"\xcd\x2c\x96", posi=b"\xcd\x09\x9a", arg=[b"\x11"], program=range(12, 14), data=[3], offset=0, sectors=(0, 5), inside=b"\xed\x53\xdf\xb9"),
    "2.0": dict(chain=b"\xcd\x0b\x13", posi=None, arg=[b"\xdd\x21", b"\xfd\x21"], program=range(3, 7), data=[8, 11], offset=0x5000, sectors=(1, 5), inside=None),
    "login": dict(chain=b"\xcd\x79\x0d", posi=None, arg=[b"\xdd\x21", b"\xfd\x21", b"\x32", b"\x3a"], program=range(3, 4), data=[5, 6], offset=0x2000, sectors=(0, 5), inside=b"\xcd\x21\x0e"),
}
SECTOR = 256
SECTORS = 6


def main() -> int:
    archive, work = Path(sys.argv[1]), Path(sys.argv[2])
    count = int(sys.argv[3]) if len(sys.argv) > 3 else 60
    shutil.rmtree(work, ignore_errors=True)
    patcher, cases = work / "patcher", work / "cases"
    cases.mkdir(parents=True)
    with zipfile.ZipFile(archive) as zip_file:
        for name in zip_file.namelist():
            if name.startswith("d88_patcher_python/") and not name.endswith("/"):
                target = patcher / Path(name).name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(zip_file.read(name))
    shutil.copy(Path(__file__).with_name("subsetsum.py"), patcher / "subsetsum.py")
    sys.path.insert(0, str(patcher))
    import dsretrack  # the script's own encoders
    original = json.loads((patcher / "dragon_slayer_orig.json").read_text(encoding="utf-8"))
    chains = [text for text in original if isinstance(text, list)]
    lines = [text for text in original if isinstance(text, str)]

    random.seed(20261114)
    made = translated = 0
    for number in range(count):
        version = random.choice(list(FORMATS))
        form = FORMATS[version]
        start, end = form["sectors"]
        data = bytearray(len(form["data"]) * (end - start) * SECTOR)
        code = bytearray(len(form["program"]) * (end - start) * SECTOR)
        cursor, placed = random.randint(0, 40), []
        texts = random.sample(chains, random.randint(1, len(chains)))
        if form["posi"]:
            texts += random.sample(lines, random.randint(0, len(lines)))
        random.shuffle(texts)
        for text in texts:
            encoded = dsretrack.chain_to_bytes(text) if isinstance(text, list) else dsretrack.to_bytes(text)
            if cursor + len(encoded) > len(data) - 8:
                break
            data[cursor:cursor + len(encoded)] = encoded
            placed.append((text, cursor, len(encoded)))
            cursor += len(encoded) + random.choice([0, 1, 2, 3, 3, 3, 6, 12])
        spot = random.randint(0, 30)

        def point(signature, address):
            nonlocal spot
            arg = random.choice(form["arg"])
            piece = arg + struct.pack("<H", (address + form["offset"]) & 0xFFFF) + bytes(random.randint(0, 20)) + signature
            if spot + len(piece) + 4 > len(code):
                return
            code[spot:spot + len(piece)] = piece
            spot += len(piece) + random.randint(2, 30)

        point(form["chain"], placed[0][1] if placed else 0)
        for text, address, length in placed:
            signature = form["chain"] if isinstance(text, list) else form["posi"]
            for _ in range(random.choice([1, 1, 2])):
                point(signature, address)
            if form["inside"] and isinstance(text, list) and random.random() < 0.4:
                point(form["inside"], address + 2)
        for _ in range(random.randint(0, 6)):  # noise
            at = random.randrange(len(code))
            code[at] = random.randrange(256)

        # The floppy: 80 tracks of SECTORS sectors, program and data tracks filled in.
        tracks = {}
        for track in range(80):
            tracks[track] = [bytes([random.randrange(256) for _ in range(SECTOR)]) for _ in range(SECTORS)]
        for index, track in enumerate(form["program"]):
            for sector in range(start, end):
                at = (index * (end - start) + sector - start) * SECTOR
                tracks[track][sector] = bytes(code[at:at + SECTOR])
        for index, track in enumerate(form["data"]):
            for sector in range(start, end):
                at = (index * (end - start) + sector - start) * SECTOR
                tracks[track][sector] = bytes(data[at:at + SECTOR])
        image = bytearray(0x2B0)
        offsets = []
        for track in range(80):
            offsets.append(len(image))
            for sector, content in enumerate(tracks[track]):
                image += bytes([track // 2, track % 2, sector + 1, 1]) + bytes(12) + content
        for track, address in enumerate(offsets):
            struct.pack_into("<I", image, 0x20 + 4 * track, address)
        struct.pack_into("<I", image, 0x1C, len(image))
        disk = cases / f"cas{number:03d}-{version}.d88"
        disk.write_bytes(image)
        made += 1
        subprocess.run([sys.executable, str(patcher / "dsretrack.py"), str(disk)], cwd=patcher, capture_output=True)
        translated += (cases / f"cas{number:03d}-{version} [FR].d88").exists()
    print(f"{made} disquettes, {translated} traduites par le script Python")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
