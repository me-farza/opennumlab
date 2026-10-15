#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  printf '%s\n' 'This local application bundle script requires macOS.' >&2
  exit 1
fi
mathlab_repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$mathlab_repo/scripts/release-package.py" "$@"
