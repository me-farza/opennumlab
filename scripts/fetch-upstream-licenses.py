#!/usr/bin/env python3
"""Fetch missing original GitHub notices at published crates' recorded commits.

This opt-in network operation adds provenance-checked texts under
third-party/licenses. It never substitutes a generic license for an upstream
copyright notice and does not download code/binaries.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import urllib.error
import urllib.parse
import urllib.request

from license_support import cargo_inventory, github_repository, is_notice_file, packaged_texts, vcs_info


def read_url(url):
    request = urllib.request.Request(url, headers={"User-Agent": "Open-MathLab-license-audit"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target")
    parser.add_argument("--package", action="append")
    arguments = parser.parse_args()
    metadata, packages, target = cargo_inventory(arguments.target, arguments.package)
    workspace = set(metadata["workspace_members"])
    destination = Path(metadata["workspace_root"]) / "third-party" / "licenses"
    destination.mkdir(parents=True, exist_ok=True)
    index_path = destination / "index.json"
    old = json.loads(index_path.read_text()) if index_path.exists() else {"packages": []}
    records = {(item["name"], item["version"], item["commit"]): item for item in old["packages"]}
    trees = {}
    downloads = {}
    unresolved = []
    for package in sorted(packages, key=lambda item: (item["name"], item["version"])):
        if package["id"] in workspace:
            continue
        if packaged_texts(package) and " AND " not in (package.get("license") or ""):
            continue
        vcs = vcs_info(package)
        commit = vcs.get("git", {}).get("sha1")
        repository = github_repository(package.get("repository"))
        if not repository or not commit:
            unresolved.append(package["name"] + "-" + package["version"])
            continue
        key = (repository, commit)
        existing = records.get((package["name"], package["version"], commit))
        if existing and all((destination / file["path"]).is_file() and hashlib.sha256((destination / file["path"]).read_bytes()).hexdigest() == file["sha256"] for file in existing["files"]):
            print("Reused {} {}: retained exact-commit notices".format(package["name"], package["version"]), flush=True)
            continue
        try:
            if key not in trees:
                tree_url = "https://api.github.com/repos/{}/git/trees/{}?recursive=1".format(repository, commit)
                tree = json.loads(read_url(tree_url))
                if tree.get("truncated"):
                    raise RuntimeError("The upstream file tree was truncated")
                trees[key] = tree["tree"]
            scope = vcs.get("path_in_vcs", "").rstrip("/")
            candidates = []
            for item in trees[key]:
                path = item["path"]
                parsed = PurePosixPath(path)
                if item["type"] != "blob" or not is_notice_file(path):
                    continue
                if parsed.is_absolute() or ".." in parsed.parts:
                    raise RuntimeError("Invalid upstream notice path")
                if len(parsed.parts) == 1 or not scope or path.startswith(scope + "/"):
                    candidates.append(path)
            files = []
            for path in candidates:
                raw_url = "https://raw.githubusercontent.com/{}/{}/{}".format(repository, commit, urllib.parse.quote(path))
                relative = Path(repository) / commit / path
                local = destination / relative
                if raw_url not in downloads:
                    content = read_url(raw_url)
                    content.decode("utf-8")
                    local.parent.mkdir(parents=True, exist_ok=True)
                    local.write_bytes(content)
                    downloads[raw_url] = hashlib.sha256(content).hexdigest()
                files.append({"path": str(relative), "url": raw_url, "sha256": downloads[raw_url]})
            copyright_status = "original upstream notices retained"
            if not files and package["name"] == "dispatch" and package["version"] == "0.2.0" and package.get("license") == "MIT":
                for source_path in ("Cargo.toml", "README.md"):
                    raw_url = "https://raw.githubusercontent.com/{}/{}/{}".format(repository, commit, source_path)
                    content = read_url(raw_url)
                    if source_path == "Cargo.toml" and not re.search(r'^license\s*=\s*"MIT"\s*$', content.decode("utf-8"), re.MULTILINE):
                        raise RuntimeError("The original source does not declare the expected MIT license")
                    relative = Path(repository) / commit / ("UPSTREAM-" + source_path)
                    local = destination / relative
                    local.parent.mkdir(parents=True, exist_ok=True)
                    local.write_bytes(content)
                    files.append({"path": str(relative), "url": raw_url, "sha256": hashlib.sha256(content).hexdigest(), "kind": "original upstream license declaration and project context"})
                spdx_revision = json.loads(read_url("https://api.github.com/repos/spdx/license-list-data/commits/main"))["sha"]
                spdx_url = "https://raw.githubusercontent.com/spdx/license-list-data/{}/text/MIT.txt".format(spdx_revision)
                standard_text = read_url(spdx_url).decode("utf-8")
                permission_text = "\n".join(line for line in standard_text.splitlines() if not line.startswith("Copyright (c) <year>")) + "\n"
                for name, text in (("SPDX-MIT-reference.txt", standard_text), ("MIT-permission.txt", permission_text)):
                    relative = Path(repository) / commit / name
                    local = destination / relative
                    local.write_text(text)
                    files.append({"path": str(relative), "url": spdx_url, "source_commit": spdx_revision, "sha256": hashlib.sha256(local.read_bytes()).hexdigest(), "kind": "standard MIT terms accompanying explicit upstream MIT declaration", "modification": "Unassigned template copyright line omitted; no year or copyright holder was invented" if name == "MIT-permission.txt" else "Unmodified SPDX template reference; its placeholders are not an upstream copyright notice"})
                copyright_status = "upstream copyright notice absent; original explicit MIT declaration and standard permission terms retained without inventing a year or holder"
            if not files:
                raise RuntimeError("No original upstream notice found")
            records[(package["name"], package["version"], commit)] = {
                "name": package["name"], "version": package["version"], "repository": repository,
                "commit": commit, "path_in_vcs": scope, "files": files,
                "copyright_status": copyright_status,
            }
            print("Retained {} {}: {} provenance-recorded notice/declaration files".format(package["name"], package["version"], len(files)), flush=True)
        except (urllib.error.URLError, ValueError, RuntimeError) as error:
            unresolved.append(package["name"] + "-" + package["version"])
            print("Unresolved {} {}: {}".format(package["name"], package["version"], error), flush=True)
    index = {
        "scope": "Original upstream shared-workspace notices at published crate commits, with explicitly identified standard-license supplements where upstream provided only a license declaration",
        "last_requested_target": target,
        "packages": sorted(records.values(), key=lambda item: (item["name"], item["version"], item["commit"])),
        "unresolved": unresolved,
    }
    index_path.write_text(json.dumps(index, indent=2) + "\n")
    if unresolved:
        raise SystemExit("Unresolved notices: " + ", ".join(unresolved))


if __name__ == "__main__":
    main()
