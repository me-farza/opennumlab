#!/usr/bin/env bash
# Compatibility entry point for runtime setup guidance.
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
mathlab_script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec "$mathlab_script_directory/setup.sh" "$@"
