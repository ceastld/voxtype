# Model catalog (`models.json`)

Bundled into the VoxType installer and attached to each app release.

## Fields

| Field | Description |
|-------|-------------|
| `download.url` | Primary URL — use a **domestic CDN** (e.g. Bitiful S3) |
| `download.mirrorUrl` | Optional; tried before `url` |
| `download.fallbackUrls` | Optional extra mirrors |
| `download.sha256` | Optional integrity check |

## Override without reinstall

Copy to `%LOCALAPPDATA%\VoxType\catalog\models.json` and restart VoxType.

## Release checklist

1. Upload model zip to your CDN.
2. Update `sha256` and `url` in this file.
3. Tag `v*.*.*` — CI ships installer + this JSON on GitHub Release.
