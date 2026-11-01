# Prototype icons — provenance

Throwaway prototype for "How should REDA's icons look?" (openreda/reda#32). MIT, like the repo.

| Set | Files | Made by | How |
|---|---|---|---|
| `line/` | 12 SVGs, 24 px grid | Claude (Opus 5.5), hand-written SVG | Drawn from scratch after the concept rounds below. |
| `depth/` | 12 SVGs, 24 px grid | Claude (Opus 5.5), hand-written SVG | Drawn from scratch; finish informed by concepts A, C and E. |

No generated image is shipped or traced. The concepts in `../concepts/` were style references only:

- Model: `recraft/recraft-v4.1-flash` through OpenRouter, raster (WebP) output.
- Prompt: `A single software toolbar icon for an electronics design app, simple enough to read at 16 pixels. Subject: <subject>. <style>` with subjects from `subjects.tsv` and styles A–E from `styles.tsv`.
- Contact sheets: `sheet-A.png` … `sheet-E.png` (12 subjects each, in `subjects.tsv` order).

`line/` won. Its SVGs carry the dark-theme role colours; `src/icons.rs` swaps in the light-theme
tones on load (`LIGHT_SWAP`).
