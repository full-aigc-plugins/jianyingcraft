# App icon

The icon is original work by JianyingCraft contributors, drawn from code (no third-party artwork,
iconography or reference was used). It is a gradient rounded square carrying three vertical bars and
a timeline baseline — the same idea as the editor's timeline.

## Files

| File | Purpose |
|---|---|
| `assets/brand/jianyingcraft.svg` | canonical vector mirror of the geometry |
| `assets/brand/make-icon.py` | generator: computes the geometry and renders every raster format |
| `jianyingcraft-1024.png` | 1024 px render (store listings, docs) |
| `jianyingcraft-512.png` | 512 px render, embedded by `apps/filmcraft/src/main.rs` as the macOS runtime icon |
| `jianyingcraft.ico` | Windows icon, 16–256 px, embedded into the exe by `apps/filmcraft/build.rs` |
| `jianyingcraft.icns` | macOS bundle icon (`CFBundleIconFile`) |
| `hicolor/<size>/apps/ai.jianyingcraft.png`, `hicolor/scalable/…svg` | Linux icon theme, used by the desktop entry |

## Regenerating

```sh
packaging/icons.sh            # or: python assets/brand/make-icon.py
```

The outputs are committed, so builds and packaging never need Python or Pillow.

## Notes

- The Linux hicolor files keep the packaged app id `ai.jianyingcraft` until the
  packaging/bundle-id migration lands; only the artwork changed here.
- Every file has a `.attribution` sidecar and a row in [`ATTRIBUTION.md`](../../ATTRIBUTION.md);
  `cargo xtask assets` enforces both.