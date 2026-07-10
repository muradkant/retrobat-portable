#!/usr/bin/env python3
"""Build an evidence-backed Featured collection against the classics snapshot."""

from __future__ import annotations

import argparse
import bisect
import gzip
import json
import re
import unicodedata
from pathlib import Path

import requests
from bs4 import BeautifulSoup


ROOT = Path(__file__).resolve().parents[1]
CLASSICS = ROOT / "catalog" / "classics-library-v1.json.gz"
BROWSE = ROOT / "catalog" / "browse-library-v2.json"
ICONIC = ROOT / "catalog" / "iconic-library-v1.json"
OUTPUT = ROOT / "catalog" / "featured-v1.json"
WIKIPEDIA = "https://en.wikipedia.org/wiki/List_of_video_games_listed_among_the_best"
HALL = (
    "https://www.museumofplay.org/exhibits/"
    "world-video-game-hall-of-fame/inducted-games/"
)
HALL_AJAX = "https://www.museumofplay.org/wp/wp-admin/admin-ajax.php"
USER_AGENT = "RetroPort catalogue builder/0.1"


def display_order(title: str) -> str:
    """Move catalogue-style trailing articles before the main title."""
    value = title.strip()
    match = re.match(
        r"^(.+?),\s*(the|an|a)(?=(?:\s*[-:]\s*|$))(.*)$",
        value,
        flags=re.IGNORECASE,
    )
    if match:
        value = f"{match.group(2)} {match.group(1)}{match.group(3)}"
    return value


def normalized(title: str) -> str:
    title = display_order(title)
    value = unicodedata.normalize("NFKD", title).encode("ascii", "ignore").decode()
    value = re.sub(r"^(?:the|ea sports)\s+", "", value, flags=re.IGNORECASE)
    return re.sub(r"[^a-z0-9]+", "", value.casefold())


ALIASES = {
    normalized("Day of the Tentacle"): [normalized("Maniac Mansion: Day of the Tentacle")],
    normalized("Dune II"): [normalized("Dune II: The Building of a Dynasty")],
    normalized("King's Quest"): [normalized("King's Quest: Quest for the Crown")],
    normalized("Ōkami"): [normalized("Ookami")],
    normalized("WarioWare, Inc.: Mega Microgames!"): [
        normalized("WarioWare, Inc. - Mega Microgame$!")
    ],
    normalized("Yoshi's Island"): [normalized("Super Mario World 2 - Yoshi's Island")],
}

ALIAS_PREFIXES = {
    normalized("Dragon Quest VIII"): ["Dragon Quest VIII - Journey of the Cursed King"],
    normalized("Minecraft"): [
        "Minecraft - PlayStation 3 Edition",
        "Minecraft - PlayStation Vita Edition",
        "Minecraft - Xbox 360 Edition",
    ],
    normalized("Pokémon Gold and Silver"): [
        "Pokemon - Gold Version",
        "Pokemon - Silver Version",
        "Pocket Monsters Kin",
        "Pocket Monsters Gin",
    ],
    normalized("Pokémon Red and Blue"): [
        "Pokemon - Red Version",
        "Pokemon - Blue Version",
    ],
    normalized("Pokémon Red and Green"): [
        "Pocket Monsters - Aka",
        "Pocket Monsters - Midori",
    ],
    normalized("Wave Race 64"): ["Wave Race 64 - Kawasaki Jet Ski"],
}

LOW_VALUE = re.compile(
    r"\b(?:bootleg|hack|prototype|proto|beta|demo|sample|debug|pirate)\b",
    flags=re.IGNORECASE,
)


def match_title(
    title: str,
    by_title: dict[str, list[str]],
    prepared_titles: list[tuple[str, str, str]],
) -> list[str]:
    key = normalized(title)
    matches = set(by_title.get(key, []))
    for alias in ALIASES.get(key, []):
        matches.update(by_title.get(alias, []))

    # Regional editions and subtitle-bearing releases are still the named
    # game when the catalogue title starts at a textual separator. This does
    # not turn "Fallout" into "Fallout 2" or "StarCraft" into "StarCraft II".
    expected = display_order(title).casefold()
    prefixes = [
        display_order(alias).casefold() for alias in ALIAS_PREFIXES.get(key, [])
    ]
    for prefix, exact_allowed in [(expected, False), *[(value, True) for value in prefixes]]:
        start = bisect.bisect_left(prepared_titles, (prefix, "", ""))
        for candidate, entry_id, original in prepared_titles[start:]:
            if not candidate.startswith(prefix):
                break
            if LOW_VALUE.search(original):
                continue
            if (exact_allowed and candidate == prefix) or candidate.startswith(prefix + " ("):
                matches.add(entry_id)
    return sorted(matches)


def representative_matches(
    title: str,
    matches: list[str],
    entries_by_id: dict[str, dict],
) -> list[str]:
    """Keep the best representative per system, not every regional revision."""
    expected = normalized(title)

    def rank(entry: dict) -> tuple:
        candidate = entry["title"]
        lowered = candidate.casefold()
        if normalized(candidate) == expected:
            identity = 0
        elif any(
            normalized(candidate) == normalized(alias)
            for alias in ALIASES.get(expected, [])
        ):
            identity = 1
        else:
            identity = 2
        if "(world" in lowered or "(usa" in lowered:
            region = 0
        elif "(europe" in lowered:
            region = 1
        elif "(japan" in lowered:
            region = 2
        elif "(" not in candidate:
            region = 0
        else:
            region = 3
        return (
            identity,
            region,
            entry.get("artwork_url") is None and entry.get("artwork_asset") is None,
            len(candidate),
            candidate.casefold(),
        )

    by_system: dict[str, list[dict]] = {}
    for entry_id in matches:
        entry = entries_by_id[entry_id]
        if not LOW_VALUE.search(entry["title"]):
            by_system.setdefault(entry["system"], []).append(entry)
    return sorted(
        min(system_entries, key=rank)["id"] for system_entries in by_system.values()
    )


def wikipedia_titles(session: requests.Session) -> list[str]:
    soup = BeautifulSoup(session.get(WIKIPEDIA, timeout=60).content, "html.parser")
    tables = soup.select("table.wikitable")
    table = next(
        table
        for table in tables
        if "Game" in [heading.get_text(" ", strip=True) for heading in table.select("th")[:8]]
    )
    titles = []
    for row in table.select("tbody tr"):
        game_cell = row.find("td", recursive=False)
        if game_cell is None:
            continue
        title = game_cell.get_text(" ", strip=True)
        if title and title != "Game":
            titles.append(title)
    return sorted(set(titles))


def hall_titles(session: requests.Session) -> list[str]:
    response = session.post(
        HALL_AJAX,
        data={"action": "filter_projects", "type": "games", "number": 200},
        timeout=60,
    )
    response.raise_for_status()
    soup = BeautifulSoup(response.content, "html.parser")
    return sorted(
        {
            title.get_text(" ", strip=True)
            for title in soup.select(".column-card .card-title")
            if title.get_text(" ", strip=True)
        }
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--generated-at", required=True)
    parser.add_argument("--classics", type=Path, default=CLASSICS)
    parser.add_argument("--browse", type=Path, default=BROWSE)
    parser.add_argument("--iconic", type=Path, default=ICONIC)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()

    classics = json.loads(gzip.decompress(args.classics.read_bytes()))
    browse = json.loads(args.browse.read_text())
    iconic_entries = (
        json.loads(args.iconic.read_text())["entries"] if args.iconic.is_file() else []
    )
    entries = browse["entries"] + classics["entries"] + iconic_entries
    by_title: dict[str, list[str]] = {}
    for entry in entries:
        by_title.setdefault(normalized(entry["title"]), []).append(entry["id"])
    entries_by_id = {entry["id"]: entry for entry in entries}
    prepared_titles = [
        (display_order(entry["title"]).casefold(), entry["id"], entry["title"])
        for entry in entries
    ]
    prepared_titles.sort()

    session = requests.Session()
    session.headers["User-Agent"] = USER_AGENT
    inputs = [
        (
            "critical-consensus",
            "Critical Consensus",
            WIKIPEDIA,
            (
                "Games appearing on at least six separate best-of lists from "
                "different established publications."
            ),
            wikipedia_titles(session),
        ),
        (
            "world-video-game-hall-of-fame",
            "World Video Game Hall of Fame",
            HALL,
            (
                "Games recognized by The Strong for sustained popularity and "
                "influence on games, popular culture, or society."
            ),
            hall_titles(session),
        ),
    ]
    titles = []
    matched_ids = set()
    for source_id, _, _, _, source_titles in inputs:
        for title in source_titles:
            matches = representative_matches(
                title,
                match_title(title, by_title, prepared_titles),
                entries_by_id,
            )
            matched_ids.update(matches)
            titles.append(
                {
                    "title": title,
                    "source_id": source_id,
                    "matched_ids": matches,
                }
            )

    document = {
        "schema_version": 1,
        "generated_at": args.generated_at,
        "sources": [
            {
                "id": source_id,
                "name": name,
                "url": url,
                "methodology": methodology,
                "title_count": len(source_titles),
            }
            for source_id, name, url, methodology, source_titles in inputs
        ],
        "titles": titles,
        "matched_entry_count": len(matched_ids),
        "unmatched_title_count": sum(not item["matched_ids"] for item in titles),
    }
    args.output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n")
    print(
        json.dumps(
            {
                "input_titles": len(titles),
                "matched_titles": sum(bool(item["matched_ids"]) for item in titles),
                "matched_entries": len(matched_ids),
                "unmatched_examples": [
                    item["title"] for item in titles if not item["matched_ids"]
                ][:30],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
