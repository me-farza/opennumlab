"""Helpers for inventories of Cargo normal/build dependencies and notices.

SPDX-License-Identifier: GPL-3.0-or-later
"""
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess


def host_target():
    if os.environ.get("CARGO_BUILD_TARGET"):
        return os.environ["CARGO_BUILD_TARGET"]
    output = subprocess.check_output(["rustc", "-vV"], text=True)
    return next(line.split(": ", 1)[1] for line in output.splitlines() if line.startswith("host: "))


def cargo_inventory(target=None, packages=None, all_platforms=False):
    target = target or host_target()
    arguments = ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"]
    if not all_platforms:
        arguments += ["--filter-platform", target]
    metadata = json.loads(subprocess.check_output(arguments, text=True))
    if all_platforms:
        return metadata, metadata["packages"], None
    tree_arguments = [
        "cargo", "tree", "--offline", "--locked", "--target", target,
        "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
    ]
    for package in packages or ["mathlab-desktop", "mathlab-cli"]:
        tree_arguments += ["--package", package]
    tree = subprocess.check_output(tree_arguments, text=True)
    selected = set()
    for line in tree.splitlines():
        match = re.match(r"([^ ]+) v([^ ]+)", line)
        if match:
            selected.add((match.group(1), match.group(2)))
    inventory = [package for package in metadata["packages"] if (package["name"], package["version"]) in selected]
    return metadata, inventory, target


def is_notice_file(path):
    name = PurePosixPath(str(path)).name.upper()
    suffix = PurePosixPath(name).suffix
    if suffix in (".RS", ".PY", ".JS", ".TS", ".STDERR", ".YAML", ".YML"):
        return False
    if name.startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT")):
        return True
    if name in ("OFL.TXT", "UFL.TXT"):
        return True
    return suffix in (".TXT", ".MD", ".RST") and any(marker in name for marker in ("LICENSE", "LICENCE", "COPYRIGHT"))


def packaged_texts(package):
    root = Path(package["manifest_path"]).parent
    texts = {path for path in root.rglob("*") if path.is_file() and is_notice_file(path)}
    if package.get("license_file"):
        declared = root / package["license_file"]
        if declared.is_file():
            texts.add(declared)
    return texts


def vcs_info(package):
    path = Path(package["manifest_path"]).parent / ".cargo_vcs_info.json"
    return json.loads(path.read_text()) if path.is_file() else {}


def github_repository(repository):
    match = re.match(r"https?://github\.com/([^/]+)/([^/#?]+)", repository or "")
    if not match:
        return None
    return match.group(1) + "/" + match.group(2).removesuffix(".git")
