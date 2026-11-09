#!/usr/bin/env bash
# Refreshes crates/not-core/data/exercises.json from wger.de's public exercise database.
# The data is CC-BY-SA (3.0/4.0) or CC0 per exercise; authors are kept for attribution, and the
# file stays under those licences (see crates/not-core/data/EXERCISES-LICENSE.md).
set -euo pipefail
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
offset=0
while :; do
  curl -sSf "https://wger.de/api/v2/exerciseinfo/?limit=200&offset=$offset" -o "$tmp/page-$offset.json"
  next=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["next"] or "")' "$tmp/page-$offset.json")
  [ -z "$next" ] && break
  offset=$((offset + 200))
done
python3 - "$tmp" > crates/not-core/data/exercises.json <<'PY'
import glob, json, sys
EN, FR = 2, 12
out = []
for path in sorted(glob.glob(sys.argv[1] + "/page-*.json")):
    for e in json.load(open(path))["results"]:
        names = {t["language"]: t for t in e["translations"]}
        if EN not in names:
            continue
        aliases = sorted({a["alias"].strip() for lang in (EN, FR) if lang in names for a in names[lang]["aliases"]})
        muscle = lambda m: m["name_en"] or m["name"]
        out.append({
            "id": e["uuid"],
            "en": names[EN]["name"].strip(),
            "fr": names[FR]["name"].strip() if FR in names else None,
            "aliases": aliases,
            "category": e["category"]["name"],
            "muscles": [muscle(m) for m in e["muscles"]],
            "secondary": [muscle(m) for m in e["muscles_secondary"]],
            "equipment": [q["name"] for q in e["equipment"] if not q["name"].startswith("none")],
            "license": e["license"]["short_name"],
            "author": (e["license_author"] or "").strip(),
        })
out.sort(key=lambda x: x["en"].lower())
json.dump(out, sys.stdout, ensure_ascii=False, separators=(",", ":"))
PY
echo "exercises: $(python3 -c 'import json; print(len(json.load(open("crates/not-core/data/exercises.json"))))'), $(du -h crates/not-core/data/exercises.json | cut -f1)"
