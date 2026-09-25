"""Adds the command line to the app's bill of materials.

Pitboard.app carries the command line at Contents/Helpers/pitboard, so what the command
line is built from ships in the app too. The app's bill of materials is read from the
bindings crate, which does not depend on the command line, so it lists none of that on its
own. The command line's bill of materials for the same target is folded in: the command
line itself becomes a component, and every component and dependency it lists that the app's
does not is added. Both come from the same lockfile, so a component in both is the same one.

    python3 sbom-add-command-line.py <app bom.json> <command line bom.json>
"""

import json
import sys

app_path, cli_path = sys.argv[1], sys.argv[2]

with open(app_path) as handle:
    app = json.load(handle)
with open(cli_path) as handle:
    cli = json.load(handle)

components = app.setdefault("components", [])
known = {component["bom-ref"] for component in components}
added = 0
for component in [cli["metadata"]["component"], *cli.get("components", [])]:
    if component["bom-ref"] not in known:
        components.append(component)
        known.add(component["bom-ref"])
        added += 1

dependencies = app.setdefault("dependencies", [])
by_ref = {entry["ref"]: entry for entry in dependencies}
for entry in cli.get("dependencies", []):
    if entry["ref"] not in by_ref:
        dependencies.append(entry)
        by_ref[entry["ref"]] = entry
        continue
    depends_on = by_ref[entry["ref"]].setdefault("dependsOn", [])
    for ref in entry.get("dependsOn", []):
        if ref not in depends_on:
            depends_on.append(ref)

with open(app_path, "w") as handle:
    json.dump(app, handle, indent=2)

print(f"added {added} components from {cli_path} to {app_path}")
