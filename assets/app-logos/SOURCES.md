# Launcher app logos

These are the sources for the app tiles in `apps/launcher.gscene`. Each SVG is
converted with `arachnid import-svg` using the settings in `logos.tsv`:
canvas 320,180 (16:9), corner radius 0.12. `scripts/check-arachnid.sh`
re-runs the conversion and checks the scene still holds exactly that output.

| File | Source | Licence of the drawing |
| --- | --- | --- |
| youtube.svg | simple-icons 16.38.1 `icons/youtube.svg` | CC0-1.0 |
| netflix.svg | simple-icons 16.38.1 `icons/netflix.svg` | CC0-1.0 |
| primevideo.svg | simple-icons 9.0.0 `icons/primevideo.svg`. It was later removed at the brand owner's request and is absent from current releases. | CC0-1.0 |
| browser.svg | @mdi/svg 7.4.47 `svg/web.svg` (generic globe) | Apache-2.0 |
| livetv.svg | @mdi/svg 7.4.47 `svg/television-classic.svg` (generic TV) | Apache-2.0 |

All files were fetched from jsDelivr's npm mirror on 2026-11-26.

YouTube, Netflix and Prime Video are trademarks of their owners. The marks
identify the app that a tile opens, on the owner's own TV; they imply no
endorsement. Brand colours come from simple-icons metadata (YouTube #FF0000,
Netflix #E50914, Prime Video #1F2E3E).

The Goya Pairs tile is drawn in the scene from Pairs' own card art, not
imported.
