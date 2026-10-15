#!/usr/bin/env python3
"""Inventory locked Cargo dependencies and copy available license/notice texts.

Uses local Cargo metadata with --offline. This records upstream metadata; it is
not a replacement for a release audit of native/runtime dependencies.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
from license_support import cargo_inventory, github_repository, packaged_texts, vcs_info


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--target", help="Cargo target triple; default is the configured target or rustc host")
    parser.add_argument("--package", action="append", help="Built package; default mathlab-desktop and mathlab-cli")
    parser.add_argument("--all-platforms", action="store_true", help="Inventory all metadata packages instead of the actual build dependency tree")
    parser.add_argument("--require-complete", action="store_true", help="Fail if any selected package lacks original license text")
    arguments = parser.parse_args()
    metadata, inventory, target_triple = cargo_inventory(arguments.target, arguments.package, arguments.all_platforms)
    destination = arguments.destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    workspace_members = set(metadata["workspace_members"])
    upstream_directory = Path(metadata["workspace_root"]) / "third-party" / "licenses"
    upstream_index = upstream_directory / "index.json"
    upstream = json.loads(upstream_index.read_text()).get("packages", []) if upstream_index.is_file() else []
    records = []
    missing = []
    for package in sorted(inventory, key=lambda item: (item["name"], item["version"])):
        if package["id"] in workspace_members:
            continue
        package_root = Path(package["manifest_path"]).parent
        package_destination = destination / (package["name"] + "-" + package["version"])
        texts = packaged_texts(package)
        copied = []
        for license_path in sorted(texts):
            relative = license_path.relative_to(package_root)
            target = package_destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(license_path, target)
            copied.append(str(target.relative_to(destination)))
        provenance = []
        copyright_status = "original packaged notices retained"
        commit = vcs_info(package).get("git", {}).get("sha1")
        for supplement in upstream:
            if (supplement["name"], supplement["version"], supplement["commit"]) != (package["name"], package["version"], commit):
                continue
            if supplement["repository"].lower() != (github_repository(package.get("repository")) or "").lower():
                continue
            for file in supplement["files"]:
                original = upstream_directory / file["path"]
                if hashlib.sha256(original.read_bytes()).hexdigest() != file["sha256"]:
                    raise RuntimeError("Upstream notice hash changed: " + str(original))
                relative = Path(file["path"])
                target = package_destination / "upstream" / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(original, target)
                copied.append(str(target.relative_to(destination)))
                provenance.append({"url": file["url"], "sha256": file["sha256"], "commit": commit, "source_commit": file.get("source_commit", commit), "kind": file.get("kind", "original upstream notice"), "modification": file.get("modification")})
            copyright_status = supplement.get("copyright_status", "original upstream notices retained")
        record = {
            "name": package["name"],
            "version": package["version"],
            "license": package.get("license"),
            "source": package.get("source"),
            "repository": package.get("repository"),
            "license_texts": copied,
            "upstream_notices": provenance,
            "copyright_status": copyright_status,
        }
        records.append(record)
        if not copied:
            missing.append(package["name"] + "-" + package["version"])
    report = {
        "scope": "All Cargo metadata packages" if arguments.all_platforms else "Cargo normal/build dependency tree for the selected target and package feature configuration",
        "target": target_triple,
        "built_packages": arguments.package or ["mathlab-desktop", "mathlab-cli"],
        "excluded": "Separately installed Octave and operating-system libraries; dev-only and inactive target/optional dependencies in the default inventory",
        "packages": records,
        "missing_license_texts": missing,
        "upstream_copyright_notices_not_provided": [record["name"] + "-" + record["version"] for record in records if "absent" in record["copyright_status"]],
    }
    (destination / "index.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Recorded {} Cargo packages in {}".format(len(records), destination))
    if missing:
        print("License texts require follow-up for: " + ", ".join(missing))
        if arguments.require_complete:
            raise SystemExit(1)
    if report["upstream_copyright_notices_not_provided"]:
        print("Upstream supplied no specific copyright notice for {}; explicit declaration and permission terms retained.".format(", ".join(report["upstream_copyright_notices_not_provided"])))


if __name__ == "__main__":
    main()
