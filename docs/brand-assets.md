# Brand Assets

Aetower uses one canonical app-icon source image and derives macOS package
assets during release packaging.

## Canonical Files

- `assets/brand/aetower-app-icon-source.png`
  - Committed source image for the app icon.
  - Use a square PNG, preferably 1024 x 1024 or larger.
  - Transparency is supported.
- `assets/brand/aetower-app-icon-preview.png`
  - Optional 1024 px preview generated from the source.

## Import A New Icon

Save the source image locally, then run:

```sh
sh scripts/import-app-icon.sh /path/to/source-icon.png
```

The importer normalizes the filename, writes the 1024 px preview, and verifies
that `scripts/generate-app-icon.sh` can produce the macOS `.icns` file. It also
removes edge-connected light checkerboard matte pixels from RGB exports so the
committed source uses real alpha transparency.

## Package Behavior

`scripts/package-macos.sh` calls `scripts/generate-app-icon.sh`.

Generation order:

1. If `assets/brand/aetower-app-icon-source.png` exists, derive all `.iconset`
   sizes from it with `sips`, then build `Aetower.icns` with `iconutil`.
2. If no source image exists, fall back to the deterministic generated icon so
   development packaging still works.

Release packages embed the generated icon as:

```text
Aetower.app/Contents/Resources/Aetower.icns
```

and declare it through:

```text
CFBundleIconFile = Aetower
```
