"""Adds Sparkle to the app's bill of materials.

cargo-cyclonedx reads the Cargo lockfile, so it describes the app's Rust half and stops
there. Sparkle is the rest of what ships inside Pitboard.app, it is the part that installs
code on someone else's machine, and a bill of materials that leaves it out is worse than
none: it reads as complete. Its version is the one apple/Package.resolved pins, which is
what build-app.sh put in the bundle.

    python3 sbom-add-sparkle.py <bom.json> <Package.resolved>
"""

import json
import sys

bom_path, resolved_path = sys.argv[1], sys.argv[2]

with open(resolved_path) as handle:
    pins = json.load(handle)["pins"]
sparkle = [pin for pin in pins if pin["identity"] == "sparkle"]
if len(sparkle) != 1:
    raise SystemExit(f"{resolved_path} pins {len(sparkle)} packages called sparkle")
version = sparkle[0]["state"]["version"]

with open(bom_path) as handle:
    bom = json.load(handle)
bom.setdefault("components", []).append(
    {
        "type": "library",
        "bom-ref": f"pkg:swift/github.com/sparkle-project/Sparkle@{version}",
        "name": "Sparkle",
        "version": version,
        "purl": f"pkg:swift/github.com/sparkle-project/Sparkle@{version}",
        "licenses": [{"license": {"id": "MIT"}}],
        "externalReferences": [
            {"type": "vcs", "url": "https://github.com/sparkle-project/Sparkle"}
        ],
    }
)
with open(bom_path, "w") as handle:
    json.dump(bom, handle, indent=2)

print(f"added Sparkle {version} to {bom_path}")
