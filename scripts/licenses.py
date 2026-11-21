#!/usr/bin/env python3
"""Writes licenses.json for an app's About → Open-source licenses page.

    scripts/licenses.py android|apple OUT

Lists every Rust crate the app links (the not-ffi dependency tree for that platform, from
`cargo tree`), then the platform's own libraries. Each entry: name, version, SPDX license, link.
The apps link each license id to its text on spdx.org. Run by build-android.sh and build-apple.sh;
the output is a build product (gitignored), like the bindings.
"""

import json
import subprocess
import sys

TARGETS = {"android": "aarch64-linux-android", "apple": "aarch64-apple-darwin"}

# Libraries outside Cargo, by platform: what the app's build files pull in.
PLATFORM = {
    "android": [
        ("AndroidX (Compose, Activity, Lifecycle, Glance, WorkManager, CameraX)", "", "Apache-2.0",
         "https://developer.android.com/jetpack/androidx"),
        ("Kotlin standard library", "", "Apache-2.0", "https://github.com/JetBrains/kotlin"),
        ("kotlinx.coroutines", "", "Apache-2.0", "https://github.com/Kotlin/kotlinx.coroutines"),
        ("ZXing", "", "Apache-2.0", "https://github.com/zxing/zxing"),
        ("JNA", "", "Apache-2.0 OR LGPL-2.1-or-later", "https://github.com/java-native-access/jna"),
    ],
    "apple": [],
}


def rust_crates(target: str) -> list[dict]:
    out = subprocess.run(
        ["cargo", "tree", "-p", "not-ffi", "-e", "normal", "--target", target,
         "--prefix", "none", "--format", "{p}\t{l}\t{r}"],
        check=True, capture_output=True, text=True,
    ).stdout
    crates = {}
    for line in out.splitlines():
        package, license_, repo = (line.split("\t") + ["", ""])[:3]
        # "name v1.2.3 (path) (*)" → name, version
        name, _, rest = package.partition(" v")
        version = rest.split(" ")[0]
        if name.startswith("not-") or name == "uniffi-bindgen":
            continue
        url = repo.replace(" (*)", "").strip() or f"https://crates.io/crates/{name}"
        crates[(name, version)] = {
            "name": name,
            "version": version,
            # Old crates write "MIT/Apache-2.0"; SPDX says "MIT OR Apache-2.0".
            "license": license_.replace(" (*)", "").replace("/", " OR ").strip() or "see link",
            "url": url,
        }
    return sorted(crates.values(), key=lambda c: (c["name"].lower(), c["version"]))


def main() -> None:
    platform, out = sys.argv[1], sys.argv[2]
    libs = [
        {"name": n, "version": v, "license": l, "url": u} for n, v, l, u in PLATFORM[platform]
    ] + rust_crates(TARGETS[platform])
    with open(out, "w", encoding="utf-8") as f:
        json.dump({"libraries": libs}, f, ensure_ascii=False, indent=1)
    print(f"{platform}: {len(libs)} libraries in {out}")


if __name__ == "__main__":
    main()
