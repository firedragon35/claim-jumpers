# Changelog

All notable changes to the native Windows port of Claim Jumpers. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the port uses [Semantic Versioning](https://semver.org/) as defined in
[CLAUDE.md](CLAUDE.md#versioning).

## [1.0.1] - 2026-11-07

### Fixed

- Every screen drew upside down: menus, HUD and the field were mirrored top
  to bottom, so text was unreadable, mouse clicks landed on the mirror image
  of a button, and turning looked reversed. macroquad 0.4's
  `Camera2D::from_display_rect` points Y up; all three cameras now use
  `display_camera()`, which points Y down as the browser canvas does.
- Pausing did nothing: P, Esc and the gamepad's Start opened the pause card
  and closed it again in the same frame.
- The `--smoke` screenshots were saved upside down once the window drew
  upright (they had flipped the image to undo the mirrored cameras).

No change to saves, controls or the package layout.

## [1.0.0] - 2026-11-07

First release: every system of the browser game ported, packaged as
`ClaimJumpers-1.0.0-windows-x64.zip` by `tools/package.ps1`. See the
README for the full feature list.

The 1.0.0 zip handed out on this date has the bugs fixed in 1.0.1. The
fixes reached the repository before the commit that recorded 1.0.0
(883d7bf), so a build of that commit already has them but still reports
1.0.0; 1.0.1 is the first version number that guarantees them.
