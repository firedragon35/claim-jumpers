# Changelog

All notable changes to the native Windows port of Claim Jumpers. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the port uses [Semantic Versioning](https://semver.org/) as defined in
[CLAUDE.md](CLAUDE.md#versioning).

## [1.0.4] - 2026-11-09

Brings the port in line with the browser game's 1.0.6 and 1.0.7 review fixes.
Golden traces are re-recorded from browser 1.0.7.

### Fixed

- Rivals, drones and collectors crashing into rocks no longer earn bounties,
  grudge or tally. Nor do reactors you didn't set off: one knocked while not
  towed or thrown, or shot by enemy fire. Wrecked Kessler jaws always count on
  the kill tally, as in the browser.
- A blast or torpedo no longer also hits the fragments it just made, so
  boulders split instead of vanishing.
- A crab on the hull lets go of the cargo it crawled off, so banking that
  cargo no longer kills it.
- The refinery drifts around the carrier instead of possibly towards it.
- Cargo taken in an arrest counts in the report's Stolen / lost figure.
- Contract offers survive a restart instead of being rerolled.
- A chain step and an ordinary offer with the same target no longer both show
  as accepted.
- Paying off the loan in the shop shows a blank report instead of the last
  shift's figures and replay.
- The net and hook prompts show rebound keys.
- The cuff meter follows the sheriff closest to making the arrest.
- Night-shift arrows no longer reveal contacts the radar can't see.

## [1.0.3] - 2026-11-09

### Fixed

- A career file that can't be read or parsed is copied to `career.bad.json`
  before a new career can replace it. Saves are flushed to disk before the
  final rename, so a power cut can't leave an empty file.
- Save failures now show a warning on screen; the release build has no
  console, so they used to be invisible.
- Skipping training is remembered, as finishing it is. After "Replay
  training", a skip no longer brings training back every shift.
- Local co-op: the camera frames both tugs and zooms out as they spread
  apart, as in the browser.
- Opening Settings from the pause card no longer moves the shift's cargo and
  clock behind it.
- An enemy shot at exactly the edge of hearing no longer produces invalid
  sound samples.
- A key that is another action's fixed key can't be rebound (it would have
  triggered both actions), as in the browser.
- Sound comes back after the output device is lost, such as when a headset
  is unplugged, and when a device appears after a silent start.
- SWAT, stun and rogue bolts have their browser sizes and colours.

### Added

- Gamepad rumble on hull hits, reactor blasts and being netted, as in the
  browser.

### Changed

- `--smoke` exits non-zero unless every screen was captured and the career
  reached disk at shift 2; `tools/package.ps1` stops a smoke run that hangs.
- The minimum Rust version is corrected to 1.89, which the locked `uuid`
  dependency already required.
- `tools/package.ps1` builds with `--locked`, so a release ships exactly the
  dependency versions in `Cargo.lock`.

### Removed

- The short spoken variants of radio lines, which were never read out; Marge
  reads the full line, as the browser does.
- `spikes/m0-spike`, the Milestone 0 technology test. It remains in git history.

## [1.0.2] - 2026-11-08

### Changed

- Clippy findings fixed so the lint can be enforced: a redundant match guard
  in the physics separation step, a test helper that now uses
  `slice::contains`, and a collapsed `if` in the exhaust particle emitter.
  Two intentional cases are documented with `#[allow]` instead: `Rng::next`
  keeps the name that mirrors the browser's `Math.random()`, and the spark
  constructor keeps its eight particle fields as arguments. No change to
  gameplay, the random sequence, saves, controls or the package layout.
- `spikes/m0-spike` is no longer a default workspace member (plain
  `cargo build` and `cargo test` skip it; `cargo run -p m0-spike` still works).

### Repository

- Added `native/scripts/check.sh` (sanitization, version sync, rustfmt, clippy,
  tests), a pre-commit hook, and CI workflows for the simulation (Ubuntu) and
  the game (Windows). Not part of the shipped exe.

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

## Development milestones (before 1.0.0)

Per-milestone checks recorded while the port was built, moved here from the README in 1.0.2.
All runs were on Windows 11 Pro (build 26300).

- Windows 11 Pro (build 26300), release build run from an unrelated
  folder: all M0 checks passed (see the Milestone 0 report in the commit
  history).
- Windows 11 Pro (build 26300): the M1 release build's `--smoke` run, from
  an unrelated folder, exits cleanly with the scripted tow in place.
- Windows 11 Pro (build 26300): the M2 release build's `--smoke` run, from
  an unrelated folder, exits cleanly with the cannon firing, a rival
  shooting back, the sound stream open and the screenshot as expected.
- Windows 11 Pro (build 26300): the M3 release build's `--smoke` tour, from
  an unrelated folder, exits cleanly: the shift settles (1,840 earned, 800
  installment, 1,040 in the wallet), the shop buys and mounts a part, the
  career is saved and reloaded by the title, and all six screenshots look
  as expected.
- Windows 11 Pro (build 26300): the M4 release build's `--smoke` tour, from
  an unrelated folder, exits cleanly with the law pursuing, a collector,
  the refinery's drones, a mining crew, missiles and sentries all live,
  and all six screenshots look as expected.
- Windows 11 Pro (build 26300): the M5 part 1 release build's `--smoke`
  tour, from an unrelated folder, exits cleanly with an ion storm, a boss
  crew, an escorted hauler, a sealed hold and its keycard and a contract
  live; the report settles the missed contract; the shop accepts a
  contract, hires a co-pilot and buys a truce; and all eight screenshots
  look as expected.
- Windows 11 Pro (build 26300): the M5 part 2 release build's `--smoke`
  tour, from an unrelated folder, exits cleanly with training running
  (it reached step 3 of 5), Marge's radio panel showing her lines, the
  speech synthesizer found (3 English voices; speech is off for the check
  run), and the report replaying a highlight.
- Windows 11 Pro (build 26300): the M5 part 3 release build's `--smoke`
  tour, from an unrelated folder, exits cleanly through the new title
  menu, settings, records, the leaderboard (with the tour's own shift on
  it) and the sound test, and all twelve screenshots look as expected.
