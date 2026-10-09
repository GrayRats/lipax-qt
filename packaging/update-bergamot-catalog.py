#!/usr/bin/env python3
"""Refresh the bundled offline verification catalog from Mozilla Remote Settings."""
import json
from pathlib import Path
from urllib.request import urlopen

ENDPOINT = "https://firefox.settings.services.mozilla.com/v1/buckets/main/collections/translations-models/records"
CDN = "https://firefox-settings-attachments.cdn.mozilla.net/"


def main():
    with urlopen(ENDPOINT, timeout=30) as response:
        records = json.load(response)["data"]
    groups = {}
    for record in records:
        version = record["version"]
        if not all(part.isdigit() for part in version.split(".")):
            continue  # Nightly / alpha models are not installed automatically.
        attachment = record["attachment"]
        key = (record["fromLang"], record["toLang"], version)
        groups.setdefault(key, {})[record["fileType"]] = {
            "name": record["name"], "size": attachment["size"],
            "sha256": attachment["hash"], "url": CDN + attachment["location"],
        }
    models = []
    for (source, target, version), files in sorted(
        groups.items(), key=lambda item: tuple(map(int, item[0][2].split("."))), reverse=True
    ):
        if {"model", "lex"} <= files.keys() and (
            "vocab" in files or {"srcvocab", "trgvocab"} <= files.keys()
        ):
            models.append({"pair": source + "-" + target, "version": version, "files": files})
    if not models:
        raise RuntimeError("Mozilla returned no complete stable model sets")
    target = Path(__file__).resolve().parents[1] / "crates/core/src/translate/bergamot_catalog.json"
    target.write_text(json.dumps(models, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(models)} model sets to {target}")


if __name__ == "__main__":
    main()
