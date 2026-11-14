#!/usr/bin/env python3
"""Packs a game's Japanese voices for the installer: zips of at most 1.5 GB
(GitHub refuses release files over 2 GB), stored without compression (the .ogg
files already are), unpacked as-is into the game folder.

    python scripts/pack_voices.py --voice "C:/.../Trails in the Sky FC/voice" --dsound dsound.dll --out voix-fc

The first zip holds dsound.dll (the loader) and voice/ without its ogg/ folder;
the next ones the .ogg files. Then publish them as the "voix" release of the
game's repository (its title is the version players see):

    gh release create voix --repo Le-Liberl-News/PatchFC --title "1.0" --notes "Voix japonaises" voix-fc/*.zip
"""
from __future__ import annotations

import argparse
import zipfile
from pathlib import Path

LIMIT = 1_500_000_000


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--voice", required=True, type=Path, help="the voice/ folder (ed_voice.dll, ed_voice.ini, dll/, ogg/)")
    parser.add_argument("--dsound", required=True, type=Path, help="the dsound.dll loader")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--name", default="voix", help="prefix of the zips")
    arguments = parser.parse_args()
    voice = arguments.voice
    if not (voice / "ed_voice.dll").is_file():
        parser.error(f"{voice} n'a pas de ed_voice.dll")
    arguments.out.mkdir(parents=True, exist_ok=True)

    loader = [(arguments.dsound, "dsound.dll")] + [
        (path, "voice/" + path.relative_to(voice).as_posix())
        for path in sorted(voice.rglob("*"))
        if path.is_file() and path.relative_to(voice).parts[0] != "ogg"
    ]
    oggs = [(path, "voice/" + path.relative_to(voice).as_posix()) for path in sorted((voice / "ogg").rglob("*")) if path.is_file()]

    parts: list[list[tuple[Path, str]]] = [loader]
    size = sum(path.stat().st_size for path, _ in loader)
    for item in oggs:
        weight = item[0].stat().st_size
        if size + weight > LIMIT:
            parts.append([])
            size = 0
        parts[-1].append(item)
        size += weight

    for number, part in enumerate(parts, 1):
        target = arguments.out / f"{arguments.name}-{number}.zip"
        with zipfile.ZipFile(target, "w", zipfile.ZIP_STORED, allowZip64=True) as archive:
            for path, name in part:
                archive.write(path, name)
        print(f"{target.name} : {len(part)} fichiers, {target.stat().st_size / 1e9:.2f} Go")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
