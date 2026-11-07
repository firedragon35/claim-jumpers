# Fonts

The browser game loads these three families from Google Fonts. The native
build embeds them in the executable, so players need nothing installed.

| File | Family | Weight | Used for |
|---|---|---|---|
| `BigShouldersStencilDisplay-Black.ttf` | Big Shoulders Stencil Display | 900 | Titles, big numbers |
| `BigShouldersStencilDisplay-Bold.ttf` | Big Shoulders Stencil Display | 700 | Headings |
| `BarlowSemiCondensed-Medium.ttf` | Barlow Semi Condensed | 500 | Body text |
| `BarlowSemiCondensed-SemiBold.ttf` | Barlow Semi Condensed | 600 | Buttons, emphasis |
| `BarlowSemiCondensed-Bold.ttf` | Barlow Semi Condensed | 700 | Strong text |
| `JetBrainsMono-Medium.ttf` | JetBrains Mono | 500 | Labels, ledgers |
| `JetBrainsMono-Bold.ttf` | JetBrains Mono | 700 | Tags, HUD |

## Where they came from

All files come from the official Google Fonts repository,
<https://github.com/google/fonts> (`ofl/` directory), downloaded 2026-11-07.

- Barlow Semi Condensed ships as static files and is used unmodified.
- Big Shoulders Stencil Display and JetBrains Mono ship only as variable
  fonts (kept in `source/`). The static weights above were made from them
  with fontTools 4.55.3 (`fontTools.varLib.instancer`, pinning the `wght`
  axis), because the native text renderer draws a variable font at its
  default weight only. To regenerate them, run `instancer` on the files in
  `source/` with `wght=900`/`700` (Big Shoulders) and `wght=500`/`700`
  (JetBrains Mono).

## Licence

All three families are under the SIL Open Font License 1.1. Their licence
texts are in `licenses/`, and none of them declares a Reserved Font Name, so
the instanced files may keep the family names. The licence must ship with
the fonts (the release package includes `licenses/`), and the fonts may not
be sold on their own.
