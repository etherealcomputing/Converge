# Converge brand

## Logo

The Converge logo lives in `assets/`:

1. `assets/converge-logo.webp` is the canonical source as provided
2. `assets/converge-logo.jpg` is a compatibility export
3. `assets/converge-logo-256.jpg` is the README sized variant

Use `assets/converge-logo-256.jpg` in `README.md` so the page stays fast.

## Regenerating assets (macOS)

These commands require `sips` which ships with macOS.

```bash
mkdir -p assets
sips -s format jpeg -s formatOptions 80 assets/converge-logo.webp --out assets/converge-logo.jpg
sips -Z 256 -s format jpeg -s formatOptions 80 assets/converge-logo.jpg --out assets/converge-logo-256.jpg
```

