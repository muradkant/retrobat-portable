#!/usr/bin/env python3
"""Build provenance-backed per-game controls metadata.

MAME is the authoritative per-machine provider. Libretro's maintained MAME DAT
connects RetroPort catalogue identities to machine names, MAME -listxml exposes
the machine's declared inputs, and RetroBat's gamesdb.xml adds special-device
requirements used by its own launcher. Generic console/PC presentation remains
runtime-derived from the installed backend and input configuration.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import re
import subprocess
import unicodedata
import xml.etree.ElementTree as ET
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CLASSICS = ROOT / "catalog" / "classics-library-v1.json.gz"
BROWSE = ROOT / "catalog" / "browse-library-v2.json"
ICONIC = ROOT / "catalog" / "iconic-library-v1.json"
MAME_DAT = Path.home() / ".cache/retroport/libretro-database/metadat/mame/MAME.dat"
GAMESDB = ROOT / "RetroBat/emulationstation/resources/gamesdb.xml"
RETROBAT_EMULATIONSTATION_REVISION = "d77fbf1fb198a10bb44221e40e463e2e2c30f1a7"
OUTPUT = ROOT / "catalog/controls-v1.json.gz"
GAME_START = re.compile(r"^game\s*\(", re.MULTILINE)
SHA1 = re.compile(r"\bsha1\s+([0-9A-Fa-f]{40})\b")
ROM_NAME = re.compile(r'\brom\s*\(\s*name\s+(?:"([^"]+)"|(\S+))')


def game_blocks(text: str) -> list[str]:
    starts = [match.start() for match in GAME_START.finditer(text)]
    return [text[start:end] for start, end in zip(starts, starts[1:] + [len(text)])]


def machine_by_sha1(path: Path) -> dict[str, set[str]]:
    result: dict[str, set[str]] = defaultdict(set)
    for block in game_blocks(path.read_text(encoding="utf-8", errors="replace")):
        rom = ROM_NAME.search(block)
        if not rom:
            continue
        machine = next(value for value in rom.groups() if value).removesuffix(".zip")
        for sha1 in SHA1.findall(block):
            result[sha1.casefold()].add(machine)
    return result


def retrobat_devices(path: Path) -> dict[str, list[dict]]:
    result: dict[str, list[dict]] = defaultdict(list)
    root = ET.parse(path).getroot()
    for system in root.findall("system"):
        system_id = system.attrib["id"]
        for game in system.findall("game"):
            devices = []
            for child in game:
                if child.tag in {"gun", "wheel", "spinner", "trackball", "controller"}:
                    devices.append({"type": child.tag, "attributes": dict(sorted(child.attrib.items()))})
            if devices:
                result[f"{system_id}/{game.attrib['id'].casefold()}"] = devices
    return result


def normalized_title(value: str) -> str:
    value = unicodedata.normalize("NFKD", value).encode("ascii", "ignore").decode()
    while re.search(r"\s*[\(\[].*?[\)\]]\s*$", value):
        value = re.sub(r"\s*[\(\[].*?[\)\]]\s*$", "", value)
    return re.sub(r"[^a-z0-9]+", "", value.casefold())


def mame_inputs(executable: str, wanted: set[str]) -> tuple[dict[str, dict], str]:
    version = subprocess.check_output([executable, "-version"], text=True).strip()
    process = subprocess.Popen(
        [executable, "-listxml"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    assert process.stdout is not None
    result: dict[str, dict] = {}
    for _event, machine in ET.iterparse(process.stdout, events=("end",)):
        if machine.tag != "machine":
            continue
        name = machine.attrib.get("name", "")
        if name in wanted:
            input_node = machine.find("input")
            controls = []
            if input_node is not None:
                controls = [dict(sorted(control.attrib.items())) for control in input_node.findall("control")]
                result[name] = {
                    "description": machine.findtext("description") or name,
                    "players": int(input_node.attrib.get("players", "0")),
                    "coins": int(input_node.attrib.get("coins", "0")),
                    "service": input_node.attrib.get("service") == "yes",
                    "tilt": input_node.attrib.get("tilt") == "yes",
                    "controls": controls,
                }
            else:
                result[name] = {
                    "description": machine.findtext("description") or name,
                    "players": 0,
                    "coins": 0,
                    "service": False,
                    "tilt": False,
                    "controls": controls,
                }
        machine.clear()
    if process.wait() != 0:
        raise RuntimeError("mame -listxml failed")
    return result, version


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--generated-at", required=True)
    parser.add_argument("--classics", type=Path, default=CLASSICS)
    parser.add_argument("--mame-dat", type=Path, default=MAME_DAT)
    parser.add_argument("--gamesdb", type=Path, default=GAMESDB)
    parser.add_argument("--mame", default="mame")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()

    classics = json.loads(gzip.decompress(args.classics.read_bytes()))
    browse = json.loads(BROWSE.read_text())
    iconic = json.loads(ICONIC.read_text())
    sha1_machines = machine_by_sha1(args.mame_dat)
    entry_machines: dict[str, list[str]] = {}
    for entry in classics["entries"]:
        if entry["system"] != "mame":
            continue
        machines = sorted(
            {
                machine
                for sha1 in entry.get("known_sha1", [])
                for machine in sha1_machines.get(sha1.casefold(), set())
            }
        )
        if machines:
            entry_machines[entry["id"]] = machines

    inputs, mame_version = mame_inputs(
        args.mame, {machine for machines in entry_machines.values() for machine in machines}
    )
    devices = retrobat_devices(args.gamesdb)
    system_aliases = {"mame": "arcade"}
    retrobat_profiles = {}
    for entry in [*browse["entries"], *classics["entries"], *iconic["entries"]]:
        system = system_aliases.get(entry["system"], entry["system"])
        item = devices.get(f"{system}/{normalized_title(entry['title'])}")
        if item:
            retrobat_profiles[entry["id"]] = item
    profiles = {}
    for entry_id, machines in sorted(entry_machines.items()):
        variants = []
        for machine in machines:
            item = dict(inputs.get(machine, {}))
            item["machine"] = machine
            special = devices.get(f"arcade/{machine.casefold()}", [])
            if special:
                item["special_devices"] = special
            variants.append(item)
        profiles[entry_id] = {"variants": variants}

    database_commit = subprocess.check_output(
        ["git", "-C", str(args.mame_dat.parents[2]), "rev-parse", "HEAD"], text=True
    ).strip()
    document = {
        "schema_version": 1,
        "generated_at": args.generated_at,
        "sources": [
            {
                "name": "MAME -listxml",
                "version": mame_version,
                "url": "https://docs.mamedev.org/commandline/commandline-all.html",
            },
            {
                "name": "Libretro Database MAME DAT",
                "version": database_commit,
                "url": "https://github.com/libretro/libretro-database/tree/master/metadat/mame",
            },
            {
                "name": "RetroBat gamesdb.xml",
                "version": (
                    f"{RETROBAT_EMULATIONSTATION_REVISION}; sha256="
                    f"{hashlib.sha256(args.gamesdb.read_bytes()).hexdigest()}"
                ),
                "url": (
                    "https://github.com/RetroBat-Official/emulationstation/blob/"
                    f"{RETROBAT_EMULATIONSTATION_REVISION}/resources/gamesdb.xml"
                ),
            },
        ],
        "catalog_entries": len(classics["entries"]),
        "mame_catalog_entries": sum(entry["system"] == "mame" for entry in classics["entries"]),
        "mame_profiles": profiles,
        "retrobat_profiles": dict(sorted(retrobat_profiles.items())),
    }
    payload = json.dumps(document, ensure_ascii=False, separators=(",", ":")).encode()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("wb") as output:
        with gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0) as compressed:
            compressed.write(payload)
    print(
        json.dumps(
            {
                "mame_catalog_entries": document["mame_catalog_entries"],
                "mapped_profiles": len(profiles),
                "machine_variants": sum(len(profile["variants"]) for profile in profiles.values()),
                "mame_inputs_found": len(inputs),
                "exact_retrobat_profiles": len(retrobat_profiles),
                "output_bytes": args.output.stat().st_size,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
