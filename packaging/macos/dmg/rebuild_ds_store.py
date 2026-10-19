#!/usr/bin/env python3
"""Rewrite dmg-layout.DS_Store from generate.py without resvg.

generate.py renders background.tiff with resvg and then writes the Finder layout.
The TIFF only depends on background.svg, and the layout only depends on the volume
name, the icon positions and the background alias - so when only the volume name
changed, the layout can be regenerated without resvg:

    pip install ds_store==1.3.3 mac_alias==2.2.3
    python3 packaging/macos/dmg/rebuild_ds_store.py

This deliberately does NOT touch background.tiff. Run generate.py on a machine with
resvg whenever the artwork changes.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import generate  # noqa: E402  (path set above)


def main():
    out = os.path.join(generate.HERE, "dmg-layout.DS_Store")
    generate.write_ds_store(out)
    print(f"rewrote {out} (volume {generate.VOLUME}, icons {sorted(generate.ICONS)})")


if __name__ == "__main__":
    main()
