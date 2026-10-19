#!/usr/bin/env bash
# Regenerate every app icon from the JianyingCraft mark.
#
# The artwork is original and code-drawn: `assets/brand/make-icon.py` computes the geometry
# (gradient rounded square, three bars, timeline baseline) and renders PNG, ICO, ICNS and the
# Linux hicolor set with Pillow. `assets/brand/jianyingcraft.svg` is the vector mirror of the same
# geometry. No third-party artwork or external renderer is involved, so this runs on any platform
# with Python 3 + Pillow.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PY="${PYTHON:-python3}"
command -v "$PY" >/dev/null || PY=python
"$PY" "$ROOT/assets/brand/make-icon.py"
echo "icons regenerated from assets/brand/jianyingcraft.svg"