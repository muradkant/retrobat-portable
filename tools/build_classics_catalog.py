#!/usr/bin/env python3
"""Build the artwork-first commercial classics catalogue.

The input is Libretro's maintained No-Intro, Redump, and MAME metadata mirror.
One browse entry represents a title across regions/revisions while retaining all
known SHA-1 values for local-import identification. No game payload is fetched.
Artwork URLs are included only when the official Libretro thumbnail server
currently lists matching box, title-screen, or gameplay artwork.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import html
import gzip
import json
import re
import subprocess
import urllib.parse
import unicodedata
import xml.etree.ElementTree as ET
import zipfile
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

import requests
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "catalog" / "classics-library-v1.json.gz"
DEFAULT_DATABASE = Path.home() / ".cache/retroport/libretro-database"
DATABASE_URL = "https://github.com/libretro/libretro-database.git"
THUMBNAIL_ROOT = "https://thumbnails.libretro.com"
THUMBNAIL_CACHE = Path.home() / ".cache/retroport/thumbnail-indexes"
USER_AGENT = "RetroPort catalogue builder/0.1"
ARTWORK_KINDS = ("Named_Boxarts", "Named_Titles", "Named_Snaps")
LAUNCHBOX_IMAGE_ROOT = "https://images.launchbox-app.com"
LAUNCHBOX_PLATFORMS = {
    "atari2600": "Atari 2600",
    "atari5200": "Atari 5200",
    "atari7800": "Atari 7800",
    "jaguar": "Atari Jaguar",
    "jaguarcd": "Atari Jaguar CD",
    "lynx": "Atari Lynx",
    "colecovision": "ColecoVision",
    "vectrex": "GCE Vectrex",
    "intellivision": "Mattel Intellivision",
    "pcengine": "NEC TurboGrafx-16",
    "pcenginecd": "NEC TurboGrafx-CD",
    "supergrafx": "PC Engine SuperGrafx",
    "fds": "Nintendo Famicom Disk System",
    "gb": "Nintendo Game Boy",
    "gbc": "Nintendo Game Boy Color",
    "gba": "Nintendo Game Boy Advance",
    "nes": "Nintendo Entertainment System",
    "snes": "Super Nintendo Entertainment System",
    "n64": "Nintendo 64",
    "n64dd": "Nintendo 64DD",
    "nds": "Nintendo DS",
    "3ds": "Nintendo 3DS",
    "gamecube": "Nintendo GameCube",
    "wii": "Nintendo Wii",
    "wiiu": "Nintendo Wii U",
    "virtualboy": "Nintendo Virtual Boy",
    "sg1000": "Sega SG-1000",
    "mastersystem": "Sega Master System",
    "gamegear": "Sega Game Gear",
    "megadrive": "Sega Genesis",
    "sega32x": "Sega 32X",
    "megacd": "Sega CD",
    "saturn": "Sega Saturn",
    "dreamcast": "Sega Dreamcast",
    "ngp": "SNK Neo Geo Pocket",
    "ngpc": "SNK Neo Geo Pocket Color",
    "neogeocd": "SNK Neo Geo CD",
    "psx": "Sony Playstation",
    "ps2": "Sony Playstation 2",
    "psp": "Sony PSP",
    "ps3": "Sony Playstation 3",
    "psvita": "Sony Playstation Vita",
    "xbox": "Microsoft Xbox",
    "xbox360": "Microsoft Xbox 360",
    "3do": "3DO Interactive Multiplayer",
    "mame": "Arcade",
}
LAUNCHBOX_IMAGE_PRIORITY = {
    "Box - Front": 0,
    "Box - Front - Reconstructed": 1,
    "Screenshot - Gameplay": 2,
    "Screenshot - Game Title": 3,
    "Cart - Front": 4,
    "Fanart - Box - Front": 5,
    "Advertisement Flyer - Front": 6,
}


@dataclass(frozen=True)
class Platform:
    metadata_group: str
    database_name: str
    playlist_name: str
    retrobat_system: str


PLATFORMS = [
    Platform("no-intro", "Atari - 2600", "Atari - 2600", "atari2600"),
    Platform("no-intro", "Atari - 5200", "Atari - 5200", "atari5200"),
    Platform("no-intro", "Atari - 7800", "Atari - 7800", "atari7800"),
    Platform("no-intro", "Atari - Jaguar", "Atari - Jaguar", "jaguar"),
    Platform("redump", "Atari - Jaguar CD", "Atari - Jaguar CD", "jaguarcd"),
    Platform("no-intro", "Atari - Lynx", "Atari - Lynx", "lynx"),
    Platform("no-intro", "Coleco - ColecoVision", "Coleco - ColecoVision", "colecovision"),
    Platform("no-intro", "GCE - Vectrex", "GCE - Vectrex", "vectrex"),
    Platform("no-intro", "Mattel - Intellivision", "Mattel - Intellivision", "intellivision"),
    Platform("no-intro", "NEC - PC Engine - TurboGrafx 16", "NEC - PC Engine - TurboGrafx 16", "pcengine"),
    Platform("redump", "NEC - PC Engine CD - TurboGrafx-CD", "NEC - PC Engine CD - TurboGrafx-CD", "pcenginecd"),
    Platform("no-intro", "NEC - PC Engine SuperGrafx", "NEC - PC Engine SuperGrafx", "supergrafx"),
    Platform("no-intro", "Nintendo - Family Computer Disk System", "Nintendo - Family Computer Disk System", "fds"),
    Platform("no-intro", "Nintendo - Game Boy", "Nintendo - Game Boy", "gb"),
    Platform("no-intro", "Nintendo - Game Boy Color", "Nintendo - Game Boy Color", "gbc"),
    Platform("no-intro", "Nintendo - Game Boy Advance", "Nintendo - Game Boy Advance", "gba"),
    Platform("no-intro", "Nintendo - Nintendo Entertainment System", "Nintendo - Nintendo Entertainment System", "nes"),
    Platform("no-intro", "Nintendo - Super Nintendo Entertainment System", "Nintendo - Super Nintendo Entertainment System", "snes"),
    Platform("no-intro", "Nintendo - Nintendo 64", "Nintendo - Nintendo 64", "n64"),
    Platform("no-intro", "Nintendo - Nintendo 64DD", "Nintendo - Nintendo 64DD", "n64dd"),
    Platform("no-intro", "Nintendo - Nintendo DS", "Nintendo - Nintendo DS", "nds"),
    Platform("no-intro", "Nintendo - Nintendo 3DS", "Nintendo - Nintendo 3DS", "3ds"),
    Platform("redump", "Nintendo - GameCube", "Nintendo - GameCube", "gamecube"),
    Platform("redump", "Nintendo - Wii", "Nintendo - Wii", "wii"),
    Platform("no-intro", "Nintendo - Wii U (Digital)", "Nintendo - Wii U", "wiiu"),
    Platform("no-intro", "Nintendo - Virtual Boy", "Nintendo - Virtual Boy", "virtualboy"),
    Platform("no-intro", "Sega - SG-1000", "Sega - SG-1000", "sg1000"),
    Platform("no-intro", "Sega - Master System - Mark III", "Sega - Master System - Mark III", "mastersystem"),
    Platform("no-intro", "Sega - Game Gear", "Sega - Game Gear", "gamegear"),
    Platform("no-intro", "Sega - Mega Drive - Genesis", "Sega - Mega Drive - Genesis", "megadrive"),
    Platform("no-intro", "Sega - 32X", "Sega - 32X", "sega32x"),
    Platform("redump", "Sega - Mega-CD - Sega CD", "Sega - Mega-CD - Sega CD", "megacd"),
    Platform("redump", "Sega - Saturn", "Sega - Saturn", "saturn"),
    Platform("redump", "Sega - Dreamcast", "Sega - Dreamcast", "dreamcast"),
    Platform("no-intro", "SNK - Neo Geo Pocket", "SNK - Neo Geo Pocket", "ngp"),
    Platform("no-intro", "SNK - Neo Geo Pocket Color", "SNK - Neo Geo Pocket Color", "ngpc"),
    Platform("redump", "SNK - Neo Geo CD", "SNK - Neo Geo CD", "neogeocd"),
    Platform("redump", "Sony - PlayStation", "Sony - PlayStation", "psx"),
    Platform("redump", "Sony - PlayStation 2", "Sony - PlayStation 2", "ps2"),
    Platform("redump", "Sony - PlayStation Portable", "Sony - PlayStation Portable", "psp"),
    Platform("redump", "Sony - PlayStation 3", "Sony - PlayStation 3", "ps3"),
    Platform("no-intro", "Sony - PlayStation Vita", "Sony - PlayStation Vita", "psvita"),
    Platform("redump", "Microsoft - Xbox", "Microsoft - Xbox", "xbox"),
    Platform("redump", "Microsoft - Xbox 360", "Microsoft - Xbox 360", "xbox360"),
    Platform("redump", "The 3DO Company - 3DO", "The 3DO Company - 3DO", "3do"),
    Platform("mame", "MAME", "MAME", "mame"),
]


GAME_START = re.compile(r"^game\s*\(", re.MULTILINE)
NAME = re.compile(r'^\s*name\s+"((?:\\.|[^"])*)"', re.MULTILINE)
SHA1 = re.compile(r"\bsha1\s+([0-9A-Fa-f]{40})\b")
ROM_NAME = re.compile(r'\brom\s*\(\s*name\s+(?:"([^"]+)"|(\S+))')
YEAR = re.compile(r'^\s*year\s+"?(\d{4})"?', re.MULTILINE)
DEVELOPER = re.compile(r'^\s*developer\s+"((?:\\.|[^"])*)"', re.MULTILINE)
TAG = re.compile(r"\s*[\(\[]([^\)\]]+)[\)\]]\s*$")
METADATA_TAG = re.compile(
    r"^(?:"
    r"world|usa|us|europe|japan|asia|australia|brazil|canada|china|france|germany|"
    r"italy|korea|netherlands|russia|spain|sweden|taiwan|uk|united kingdom|"
    r"en(?:,[a-z]{2})*|[a-z]{2}(?:,[a-z]{2})+|"
    r"rev(?:ision)?\s*.*|v(?:er(?:sion)?)?\s*\d.*|disc\s*\d+|disk\s*\d+|"
    r"side\s*[a-z0-9]+|track\s*\d+|beta.*|proto.*|demo.*|sample.*|"
    r"alt.*|program.*|unl|virtual console|psn|digital"
    r")$",
    re.IGNORECASE,
)
LOW_VALUE = re.compile(
    r"(?:\bbootleg\b|\bhack\b|\bprototype\b|\bproto\b|\bbeta\b|\bdemo\b|"
    r"\bsample\b|\bdebug\b|\btrainer\b|\bpirate\b|\bbad dump\b)",
    re.IGNORECASE,
)


def ensure_database(path: Path) -> str:
    if not (path / ".git").is_dir():
        path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["git", "clone", "--depth", "1", "--filter=blob:none", "--sparse", DATABASE_URL, str(path)],
            check=True,
        )
    subprocess.run(
        [
            "git",
            "-C",
            str(path),
            "sparse-checkout",
            "set",
            "metadat/no-intro",
            "metadat/redump",
            "metadat/mame",
        ],
        check=True,
    )
    return subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()


def unescape(value: str) -> str:
    return html.unescape(value.replace(r"\"", '"').replace(r"\\", "\\")).strip()


def game_blocks(text: str) -> list[str]:
    starts = [match.start() for match in GAME_START.finditer(text)]
    return [text[start:end] for start, end in zip(starts, starts[1:] + [len(text)])]


def base_title(title: str) -> str:
    value = title
    while match := TAG.search(value):
        if not METADATA_TAG.match(match.group(1).strip()):
            break
        value = value[: match.start()].rstrip()
    return value or title


def region_score(title: str) -> tuple[int, int, str]:
    lowered = title.casefold()
    if LOW_VALUE.search(lowered):
        quality = 20
    elif "(world)" in lowered:
        quality = 0
    elif "(usa" in lowered or "(us," in lowered:
        quality = 1
    elif "(europe" in lowered:
        quality = 2
    elif "(japan" in lowered:
        quality = 3
    else:
        quality = 4
    return quality, len(title), title.casefold()


def sanitize_thumbnail_name(title: str) -> str:
    return re.sub(r'[&*/:`"<>?\\|]', "_", title)


def thumbnail_listing(request: tuple[Platform, str]) -> tuple[str, set[str]]:
    platform, artwork_kind = request
    THUMBNAIL_CACHE.mkdir(parents=True, exist_ok=True)
    cache = THUMBNAIL_CACHE / (
        f"{slug(platform.playlist_name)}-{slug(artwork_kind)}.html"
    )
    url = (
        f"{THUMBNAIL_ROOT}/"
        f"{urllib.parse.quote(platform.playlist_name, safe='')}/{artwork_kind}/"
    )
    if cache.is_file():
        body = cache.read_bytes()
    else:
        session = requests.Session()
        session.headers["User-Agent"] = USER_AGENT
        session.mount(
            "https://",
            HTTPAdapter(
                max_retries=Retry(
                    total=4,
                    connect=4,
                    read=4,
                    backoff_factor=1,
                    status_forcelist=(429, 500, 502, 503, 504),
                )
            ),
        )
        response = session.get(url, timeout=90)
        if response.status_code == 404:
            body = b""
        else:
            response.raise_for_status()
            body = response.content
        cache.write_bytes(body)
    return artwork_kind, {
        urllib.parse.unquote(html.unescape(match.decode("utf-8", errors="replace")))
        for match in re.findall(rb'href="([^"]+\.png)"', body, flags=re.IGNORECASE)
    }


def slug(value: str) -> str:
    output = re.sub(r"[^a-z0-9]+", "-", value.casefold()).strip("-")
    return output[:100] or hashlib.sha1(value.encode()).hexdigest()[:16]


def artwork_key(value: str) -> str:
    title = value[:-4] if value.casefold().endswith(".png") else value
    title = base_title(title)
    return "".join(
        character
        for character in unicodedata.normalize("NFKD", title).casefold()
        if character.isalnum()
    )


def choose_artwork(
    candidates: list[dict],
    display_title: str,
    listings: dict[str, set[str]],
    indexes: dict[str, dict[str, list[str]]],
) -> tuple[dict, str, str] | None:
    for artwork_kind in ARTWORK_KINDS:
        available = listings[artwork_kind]
        for candidate in candidates:
            filename = f"{sanitize_thumbnail_name(candidate['title'])}.png"
            if filename in available:
                return candidate, artwork_kind, filename

    wanted = artwork_key(display_title)
    if not wanted:
        return None
    for artwork_kind in ARTWORK_KINDS:
        matches = indexes[artwork_kind].get(wanted, [])
        if matches:
            filename = min(matches, key=region_score)
            return candidates[0], artwork_kind, filename
    return None


def load_launchbox_artwork(
    archive_path: Path,
) -> tuple[dict[tuple[str, str], str], str]:
    games: dict[tuple[str, str], list[int]] = defaultdict(list)
    platforms_by_id: dict[int, str] = {}
    images: dict[int, tuple[int, int, str]] = {}
    region_priority = {
        "North America": 0,
        "World": 1,
        "Europe": 2,
        "Japan": 3,
    }
    with zipfile.ZipFile(archive_path) as archive:
        with archive.open("Metadata.xml") as metadata:
            for _event, element in ET.iterparse(metadata, events=("end",)):
                if element.tag == "Game":
                    database_id = element.findtext("DatabaseID")
                    name = element.findtext("Name")
                    platform = element.findtext("Platform")
                    if database_id and name and platform:
                        game_id = int(database_id)
                        platforms_by_id[game_id] = platform
                        games[(platform.casefold(), artwork_key(name))].append(game_id)
                elif element.tag == "GameAlternateName":
                    database_id = element.findtext("DatabaseID")
                    alternate = element.findtext("AlternateName")
                    if database_id and alternate:
                        game_id = int(database_id)
                        platform = platforms_by_id.get(game_id)
                        if platform:
                            games[
                                (platform.casefold(), artwork_key(alternate))
                            ].append(game_id)
                elif element.tag == "GameImage":
                    database_id = element.findtext("DatabaseID")
                    filename = element.findtext("FileName")
                    image_type = element.findtext("Type")
                    if database_id and filename and image_type in LAUNCHBOX_IMAGE_PRIORITY:
                        game_id = int(database_id)
                        priority = (
                            LAUNCHBOX_IMAGE_PRIORITY[image_type],
                            region_priority.get(element.findtext("Region") or "", 4),
                            filename,
                        )
                        if game_id not in images or priority < images[game_id]:
                            images[game_id] = priority
                if element.tag in {"Game", "GameAlternateName", "GameImage"}:
                    element.clear()
        with archive.open("Files.xml") as files:
            for _event, element in ET.iterparse(files, events=("end",)):
                if element.tag != "File":
                    continue
                platform = element.findtext("Platform")
                filename = element.findtext("FileName")
                game_name = element.findtext("GameName")
                if platform and filename and game_name:
                    game_ids = games.get(
                        (platform.casefold(), artwork_key(game_name)), []
                    )
                    if game_ids:
                        games[
                            (platform.casefold(), artwork_key(filename))
                        ].extend(game_ids)
                element.clear()
    resolved = {}
    for key, game_ids in games.items():
        available = [images[game_id] for game_id in game_ids if game_id in images]
        if available:
            resolved[key] = min(available)[2]
    digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    return resolved, digest


def apply_launchbox_artwork(
    entries: list[dict],
    artwork: dict[tuple[str, str], str],
) -> int:
    added = 0
    for entry in entries:
        if entry["artwork_url"] is not None:
            continue
        platform = LAUNCHBOX_PLATFORMS.get(entry["system"])
        if not platform:
            continue
        filename = artwork.get((platform.casefold(), artwork_key(entry["title"])))
        if not filename:
            continue
        entry["artwork_url"] = (
            f"{LAUNCHBOX_IMAGE_ROOT}/{urllib.parse.quote(filename)}"
        )
        entry["tags"].append("launchbox-artwork")
        added += 1
    return added


def apply_mame_snapshots(
    entries: list[dict],
    archive_path: Path,
    asset_output: Path,
) -> tuple[int, str]:
    listing = subprocess.check_output(
        ["7z", "l", "-ba", str(archive_path)], text=True
    )
    available = {
        line.rsplit(maxsplit=1)[-1].casefold(): line.rsplit(maxsplit=1)[-1]
        for line in listing.splitlines()
        if line.casefold().endswith(".png")
    }
    wanted: dict[str, dict] = {}
    for entry in entries:
        machine = entry.get("mame_machine")
        if (
            entry["system"] == "mame"
            and entry["artwork_url"] is None
            and machine
            and f"{machine}.png".casefold() in available
        ):
            filename = available[f"{machine}.png".casefold()]
            wanted[filename] = entry

    asset_output.mkdir(parents=True, exist_ok=True)
    filenames = sorted(wanted)
    for offset in range(0, len(filenames), 500):
        subprocess.run(
            [
                "7z",
                "e",
                "-y",
                f"-o{asset_output}",
                str(archive_path),
                *filenames[offset : offset + 500],
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )

    added = 0
    for filename, entry in wanted.items():
        path = asset_output / filename
        payload = path.read_bytes()
        if not payload.startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError(f"Progetto-SNAPS asset is not PNG: {filename}")
        entry["artwork_asset"] = {
            "path": f"Artwork/mame/{filename}",
            "size": len(payload),
            "sha256": hashlib.sha256(payload).hexdigest(),
        }
        entry["tags"].append("progetto-snaps-artwork")
        added += 1
    digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    return added, digest


def build_platform(
    database: Path,
    platform: Platform,
    listings: dict[str, set[str]],
) -> list[dict]:
    dat_path = database / "metadat" / platform.metadata_group / f"{platform.database_name}.dat"
    text = dat_path.read_text(encoding="utf-8", errors="replace")
    artwork_indexes: dict[str, dict[str, list[str]]] = {}
    for artwork_kind, filenames in listings.items():
        index: dict[str, list[str]] = defaultdict(list)
        for filename in filenames:
            index[artwork_key(filename)].append(filename)
        artwork_indexes[artwork_kind] = index
    grouped: dict[str, list[dict]] = defaultdict(list)
    for block in game_blocks(text):
        name_match = NAME.search(block)
        if not name_match:
            continue
        title = unescape(name_match.group(1))
        grouped[base_title(title)].append(
            {
                "title": title,
                "sha1": sorted({value.lower() for value in SHA1.findall(block)}),
                "year": int(YEAR.search(block).group(1)) if YEAR.search(block) else None,
                "developer": (
                    unescape(DEVELOPER.search(block).group(1))
                    if DEVELOPER.search(block)
                    else "Unknown"
                ),
                "machine": (
                    next(
                        (
                            value
                            for value in (ROM_NAME.search(block).groups())
                            if value
                        ),
                        "",
                    ).removesuffix(".zip")
                    if ROM_NAME.search(block)
                    else ""
                ),
            }
        )

    detail_url = (
        "https://github.com/libretro/libretro-database/blob/master/metadat/"
        f"{platform.metadata_group}/{urllib.parse.quote(platform.database_name + '.dat')}"
    )
    entries = []
    for display_title, variants in grouped.items():
        candidates = sorted(variants, key=lambda item: region_score(item["title"]))
        pictured = choose_artwork(
            candidates, display_title, listings, artwork_indexes
        )
        representative = pictured[0] if pictured else candidates[0]
        all_hashes = sorted(
            {sha1 for variant in variants for sha1 in variant["sha1"]}
        )
        identity = hashlib.sha1(
            f"{platform.retrobat_system}\0{display_title}".encode()
        ).hexdigest()[:12]
        artwork_url = (
            f"{THUMBNAIL_ROOT}/"
            f"{urllib.parse.quote(platform.playlist_name, safe='')}/{pictured[1]}/"
            f"{urllib.parse.quote(pictured[2])}"
            if pictured
            else None
        )
        entries.append(
            {
                "id": f"libretro-classics/{platform.retrobat_system}-{slug(display_title)}-{identity}",
                "source_id": "libretro-classics",
                "title": display_title,
                "developer": representative["developer"],
                "system": platform.retrobat_system,
                "kind": "commercial game",
                "tags": [
                    "commercial",
                    "local dump required",
                    platform.database_name,
                ],
                "license": "Commercial; user-supplied original media dump",
                "artwork_url": artwork_url,
                "artwork_asset": None,
                "detail_url": detail_url,
                "description": (
                    "Established game metadata and artwork; import a dump from "
                    "original media to play."
                ),
                "release_year": representative["year"],
                "install_state": "browse_only",
                "acquisition": "local_import",
                "known_sha1": all_hashes,
                "mame_machine": (
                    representative["machine"]
                    if platform.retrobat_system == "mame"
                    else None
                ),
            }
        )
    return sorted(entries, key=lambda item: (item["system"], item["title"].casefold()))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--database", type=Path, default=DEFAULT_DATABASE)
    parser.add_argument("--generated-at", required=True)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--launchbox-metadata", type=Path)
    parser.add_argument("--mame-snapshots", type=Path)
    parser.add_argument(
        "--mame-asset-output",
        type=Path,
        default=ROOT / "packaging" / "Artwork" / "mame",
    )
    args = parser.parse_args()

    commit = ensure_database(args.database)
    requests = [
        (platform, artwork_kind)
        for platform in PLATFORMS
        for artwork_kind in ARTWORK_KINDS
    ]
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        downloaded = list(pool.map(thumbnail_listing, requests))
    listings_by_platform = []
    for offset in range(0, len(downloaded), len(ARTWORK_KINDS)):
        listings_by_platform.append(
            dict(downloaded[offset : offset + len(ARTWORK_KINDS)])
        )

    entries = []
    platform_counts = {}
    artwork_counts = {}
    for platform, listings in zip(PLATFORMS, listings_by_platform):
        built = build_platform(args.database, platform, listings)
        entries.extend(built)
        platform_counts[platform.retrobat_system] = len(built)
        artwork_counts[platform.retrobat_system] = sum(
            item["artwork_url"] is not None for item in built
        )

    launchbox_added = 0
    launchbox_digest = None
    if args.launchbox_metadata:
        launchbox_artwork, launchbox_digest = load_launchbox_artwork(
            args.launchbox_metadata
        )
        launchbox_added = apply_launchbox_artwork(entries, launchbox_artwork)

    mame_snapshots_added = 0
    mame_snapshots_digest = None
    if args.mame_snapshots:
        mame_snapshots_added, mame_snapshots_digest = apply_mame_snapshots(
            entries, args.mame_snapshots, args.mame_asset_output
        )
    for entry in entries:
        entry.pop("mame_machine", None)

    ids = [item["id"] for item in entries]
    if len(ids) != len(set(ids)):
        raise RuntimeError("duplicate commercial catalogue ids")
    document = {
        "schema_version": 2,
        "generated_at": args.generated_at,
        "sources": [
            {
                "id": "libretro-classics",
                "name": "Libretro Classics",
                "homepage": "https://github.com/libretro/libretro-database",
                "summary": (
                    "Established No-Intro, Redump, and MAME-derived metadata "
                    "with official Libretro community artwork and LaunchBox "
                    "Games Database fallback artwork, plus Progetto-SNAPS "
                    "MAME screenshots."
                ),
                "distribution_policy": (
                    "Metadata and artwork only. Commercial game payloads are "
                    "never downloaded; the user imports original-media dumps."
                ),
                "snapshot_ref": (
                    f"libretro-database commit {commit}"
                    + (
                        f"; LaunchBox Metadata.zip SHA-256 {launchbox_digest}"
                        if launchbox_digest
                        else ""
                    )
                    + (
                        f"; Progetto-SNAPS snap.7z SHA-256 {mame_snapshots_digest}"
                        if mame_snapshots_digest
                        else ""
                    )
                ),
                "entry_count": len(entries),
            }
        ],
        "entries": entries,
    }
    encoded = (
        json.dumps(document, ensure_ascii=False, separators=(",", ":")) + "\n"
    ).encode()
    args.output.write_bytes(gzip.compress(encoded, compresslevel=9, mtime=0))
    print(
        json.dumps(
            {
                "entries": len(entries),
                "artwork": sum(
                    item["artwork_url"] is not None
                    or item["artwork_asset"] is not None
                    for item in entries
                ),
                "launchbox_artwork_added": launchbox_added,
                "mame_snapshots_added": mame_snapshots_added,
                "known_sha1": sum(bool(item["known_sha1"]) for item in entries),
                "platform_counts": platform_counts,
                "artwork_counts": artwork_counts,
                "output": str(args.output),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
