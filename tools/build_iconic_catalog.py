#!/usr/bin/env python3
"""Materialize evidence-backed iconic titles missing from the broad catalogues.

The evidence list remains authoritative. LaunchBox supplies established metadata
and artwork, while the platform mapping chooses an actual RetroBat launch route.
No game payload is downloaded by this builder.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import unicodedata
import xml.etree.ElementTree as ET
import zipfile
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import quote


ROOT = Path(__file__).resolve().parents[1]
FEATURED = ROOT / "catalog" / "featured-v1.json"
OUTPUT = ROOT / "catalog" / "iconic-library-v1.json"
IMAGE_ROOT = "https://images.launchbox-app.com"


def normalized(title: str) -> str:
    value = unicodedata.normalize("NFKD", title).encode("ascii", "ignore").decode()
    value = re.sub(r"^(?:the|ea sports)\s+", "", value, flags=re.IGNORECASE)
    value = re.sub(r"\b(?:iii|3)\b", "3", value, flags=re.IGNORECASE)
    value = re.sub(r"\b(?:ii|2)\b", "2", value, flags=re.IGNORECASE)
    value = re.sub(r"\b(?:iv|4)\b", "4", value, flags=re.IGNORECASE)
    return re.sub(r"[^a-z0-9]+", "", value.casefold())


def slug(value: str) -> str:
    result = re.sub(r"[^a-z0-9]+", "-", value.casefold()).strip("-")
    return result[:100] or hashlib.sha1(value.encode()).hexdigest()[:16]


PLATFORM_ROUTES = {
    "MS-DOS": ("dos", 0),
    "Windows": ("windows", 1),
    "Nintendo Wii U": ("wiiu", 2),
    "Nintendo Switch": ("switch", 3),
    "Sony Playstation 4": ("ps4", 4),
    "Arcade": ("mame", 5),
    "Apple II": ("apple2", 6),
    "Commodore 64": ("c64", 7),
    "Sinclair ZX Spectrum": ("zxspectrum", 8),
    "Amstrad CPC": ("amstradcpc", 9),
    "Mac OS": ("macintosh", 10),
}

LAUNCHBOX_TITLE_ALIASES = {
    normalized("Sid Meier's Civilization IV"): normalized("Civilization IV"),
    normalized("Sid Meier's Civilization V"): normalized("Civilization V"),
    normalized("Hearthstone: Heroes of Warcraft"): normalized("Hearthstone"),
    normalized("Disco Elysium: The Final Cut"): normalized("Disco Elysium"),
    normalized("Disco Elysium - The Final Cut"): normalized("Disco Elysium"),
    normalized("Disco Elysium: Final Cut"): normalized("Disco Elysium"),
}

TITLE_PLATFORM_PREFERENCE = {
    normalized("Disco Elysium"): "Windows",
    normalized("Hades"): "Windows",
    normalized("Head over Heels"): "Sinclair ZX Spectrum",
    normalized("Microsoft Flight Simulator"): "MS-DOS",
    normalized("The Witness"): "Windows",
}


def evidence_key(title: str) -> str:
    key = normalized(title)
    return LAUNCHBOX_TITLE_ALIASES.get(key, key)

IMAGE_PRIORITY = {
    "Box - Front": 0,
    "Box - Front - Reconstructed": 1,
    "Screenshot - Gameplay": 2,
    "Screenshot - Game Title": 3,
    "Advertisement Flyer - Front": 4,
}


@dataclass
class Game:
    database_id: int
    name: str
    platform: str
    developer: str
    overview: str
    year: int | None
    community_rating: float
    rating_count: int


def evidence_titles(featured_path: Path) -> dict[str, dict]:
    featured = json.loads(featured_path.read_text())
    missing: dict[str, dict] = {}
    source_names = {source["id"]: source["name"] for source in featured["sources"]}
    for record in featured["titles"]:
        if record["matched_ids"]:
            continue
        key = normalized(record["title"])
        item = missing.setdefault(
            key,
            {"title": record["title"], "evidence_sources": set()},
        )
        item["evidence_sources"].add(source_names[record["source_id"]])
    return missing


def parse_year(element: ET.Element) -> int | None:
    value = element.findtext("ReleaseYear") or element.findtext("ReleaseDate")
    if value and re.match(r"^\d{4}", value):
        return int(value[:4])
    return None


def load_launchbox(
    archive_path: Path,
    wanted: set[str],
) -> tuple[dict[str, list[Game]], dict[int, str], str]:
    games_by_key: dict[str, list[Game]] = defaultdict(list)
    games_by_id: dict[int, Game] = {}
    aliases: list[tuple[int, str]] = []
    image_candidates: dict[int, tuple[int, int, str]] = {}
    region_priority = {"World": 0, "North America": 1, "Europe": 2, "Japan": 3}

    with zipfile.ZipFile(archive_path) as archive:
        with archive.open("Metadata.xml") as metadata:
            for _event, element in ET.iterparse(metadata, events=("end",)):
                if element.tag == "Game":
                    database_id = element.findtext("DatabaseID")
                    name = element.findtext("Name")
                    platform = element.findtext("Platform")
                    if database_id and name and platform and platform in PLATFORM_ROUTES:
                        game = Game(
                            database_id=int(database_id),
                            name=name,
                            platform=platform,
                            developer=element.findtext("Developer") or "Unknown",
                            overview=element.findtext("Overview") or "",
                            year=parse_year(element),
                            community_rating=float(
                                element.findtext("CommunityRating") or 0
                            ),
                            rating_count=int(
                                element.findtext("CommunityRatingCount") or 0
                            ),
                        )
                        games_by_id[game.database_id] = game
                        key = evidence_key(name)
                        if key in wanted:
                            games_by_key[key].append(game)
                elif element.tag == "GameAlternateName":
                    database_id = element.findtext("DatabaseID")
                    alternate = element.findtext("AlternateName")
                    if database_id and alternate:
                        aliases.append((int(database_id), evidence_key(alternate)))
                elif element.tag == "GameImage":
                    database_id = element.findtext("DatabaseID")
                    filename = element.findtext("FileName")
                    image_type = element.findtext("Type")
                    if database_id and filename and image_type in IMAGE_PRIORITY:
                        game_id = int(database_id)
                        rank = (
                            IMAGE_PRIORITY[image_type],
                            region_priority.get(element.findtext("Region") or "", 4),
                            filename,
                        )
                        if game_id not in image_candidates or rank < image_candidates[game_id]:
                            image_candidates[game_id] = rank
                if element.tag in {"Game", "GameAlternateName", "GameImage"}:
                    element.clear()

    for database_id, key in aliases:
        game = games_by_id.get(database_id)
        if game and key in wanted and game not in games_by_key[key]:
            games_by_key[key].append(game)
    images = {game_id: rank[2] for game_id, rank in image_candidates.items()}
    digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    return games_by_key, images, digest


def choose_game(
    key: str, candidates: list[Game], images: dict[int, str]
) -> Game:
    preferred = TITLE_PLATFORM_PREFERENCE.get(key)
    if preferred and any(game.platform == preferred for game in candidates):
        candidates = [game for game in candidates if game.platform == preferred]

    def confidence_bucket(game: Game) -> int:
        if game.rating_count >= 100:
            return 0
        if game.rating_count >= 20:
            return 1
        if game.rating_count >= 5:
            return 2
        return 3

    return min(
        candidates,
        key=lambda game: (
            confidence_bucket(game),
            PLATFORM_ROUTES[game.platform][1],
            game.database_id not in images,
            -game.rating_count,
            -game.community_rating,
            game.database_id,
        ),
    )


def description_for(system: str, evidence: list[str]) -> str:
    evidence_text = " and ".join(evidence)
    if system == "windows":
        route = "Install your owned Windows, Steam, GOG, or publisher copy, then import its game folder or launcher."
    elif system == "dos":
        route = "Import your owned DOS installation, archive, or DOSBox launch descriptor."
    else:
        route = "Import a compatible dump made from your owned copy for this platform."
    return f"Included because it is recognized by {evidence_text}. {route}"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--featured", type=Path, default=FEATURED)
    parser.add_argument("--launchbox-metadata", type=Path, required=True)
    parser.add_argument("--generated-at", required=True)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()

    missing = evidence_titles(args.featured)
    games_by_key, images, digest = load_launchbox(
        args.launchbox_metadata, set(missing)
    )
    unresolved = sorted(
        item["title"] for key, item in missing.items() if not games_by_key.get(key)
    )
    if unresolved:
        raise RuntimeError(
            "LaunchBox has no supported-platform exact/alternate match for: "
            + "; ".join(unresolved)
        )

    entries = []
    for key, evidence in sorted(missing.items(), key=lambda item: item[1]["title"].casefold()):
        game = choose_game(key, games_by_key[key], images)
        system = PLATFORM_ROUTES[game.platform][0]
        evidence_sources = sorted(evidence["evidence_sources"])
        image = images.get(game.database_id)
        identity = hashlib.sha1(evidence["title"].encode()).hexdigest()[:12]
        entries.append(
            {
                "id": f"iconic-evidence/{slug(evidence['title'])}-{identity}",
                "source_id": "iconic-evidence",
                "title": evidence["title"],
                "developer": game.developer,
                "system": system,
                "kind": "evidence-backed iconic game",
                "tags": [
                    "iconic",
                    "community-praised",
                    f"LaunchBox platform: {game.platform}",
                    *evidence_sources,
                ],
                "license": "Commercial; user-supplied owned copy",
                "artwork_url": (
                    f"{IMAGE_ROOT}/{quote(image)}" if image else None
                ),
                "artwork_asset": None,
                "detail_url": (
                    "https://gamesdb.launchbox-app.com/games/details/"
                    f"{game.database_id}"
                ),
                "description": description_for(system, evidence_sources),
                "release_year": game.year,
                "install_state": "browse_only",
                "acquisition": "local_import",
                "known_sha1": [],
            }
        )

    document = {
        "schema_version": 2,
        "generated_at": args.generated_at,
        "sources": [
            {
                "id": "iconic-evidence",
                "name": "Iconic Evidence Completion",
                "homepage": "https://gamesdb.launchbox-app.com/",
                "summary": (
                    "Titles required by the Critical Consensus or World Video Game "
                    "Hall of Fame evidence sets that were absent from the broad retro catalogues."
                ),
                "distribution_policy": (
                    "Metadata and artwork only; the user imports or installs an owned copy."
                ),
                "snapshot_ref": f"LaunchBox Metadata.zip SHA-256 {digest}",
                "entry_count": len(entries),
            }
        ],
        "entries": entries,
    }
    args.output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n")
    print(
        json.dumps(
            {
                "entries": len(entries),
                "with_artwork": sum(entry["artwork_url"] is not None for entry in entries),
                "systems": {
                    system: sum(entry["system"] == system for entry in entries)
                    for system in sorted({entry["system"] for entry in entries})
                },
                "output": str(args.output),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
