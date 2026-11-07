# Claim Jumpers: native Windows port

A native Rust port of Claim Jumpers for Windows 10 and 11. The browser game
in the repository root (`claim-jumpers.html`, built from `src/`) is the
reference implementation; the port must match its gameplay, rules,
controls, balance and look unless a change is explicitly approved.

**Status: Milestone 2 (combat and sound).** Fly the tug, hook and tow
salvage, collide with rocks, wrecks and the carrier, and bank cargo at the
bay against the five-minute shift clock. Fire the cutting cannon (it
builds heat), cut wrecks open for their salvage, mind the reactors, shake
off scrap crabs, and fight or smash rival claim jumpers, with every sound
synthesized as in the browser. The career and the other systems arrive in
later milestones (see "Known differences" below).

| Folder | What it is |
|---|---|
| `crates/cj-sim` | The gameplay simulation: pure Rust, no window or audio, deterministic. |
| `crates/cj-game` | The Windows game (`claim-jumpers.exe`): window, rendering, input, HUD. |
| `tools/golden` | Exports reference traces from the browser game for the simulation tests. |
| `spikes/m0-spike` | The Milestone 0 technology test, kept for reference. |

## Stack

| Concern | Crate | Version |
|---|---|---|
| Window, rendering, input | macroquad | 0.4.16 |
| Gamepads | gilrs | 0.11.2 |
| Audio output (procedural synth) | cpal | 0.18.2 |
| Marge's voice (WinRT speech) | tts | 0.26.3 |

Fonts are embedded in the executable; see `assets/fonts/FONTS.md` for their
source and licences (SIL OFL 1.1).

## Development setup (Windows)

1. Install Rust with the MSVC toolchain:
   `winget install Rustlang.Rustup`, then `rustup default stable-x86_64-pc-windows-msvc`.
   Rust 1.85 or newer is required (developed with 1.99.1).
2. Install the Visual Studio C++ build tools (or Visual Studio with the
   "Desktop development with C++" workload), which provide the linker.

## Build and run the game

From the `native` folder:

```bash
cargo run -p cj-game --release
```

The executable is `target\release\claim-jumpers.exe`. It is self-contained
(fonts are embedded), so it can be copied anywhere and run without Rust.

| Action | Keyboard | Gamepad |
|---|---|---|
| Thrust | W or Up | RT or A |
| Brake / reverse | S or Down | LT or D-pad down |
| Turn | A D or Left Right | Left stick or D-pad |
| Fire | Space or J | RB |
| Hook nearest piece | E or K | Y |
| Drop last piece | Q | B |
| Cut all | X | LB |
| Pause | P or Esc | Start |
| Mute | M | Back |

When the shift clock runs out, Enter starts a new shift.

Unattended check: a scripted hook-and-tow with the cannon firing, a rival
and a crab nearby, one simulation step per frame, a screenshot to
`%TEMP%\claim-jumpers-m2-smoke.png`, and a report. The release build has
no console window, so redirect the report to a file:

```bash
target/release/claim-jumpers.exe --smoke > smoke.txt
```

## Tests

```bash
cargo test -p cj-sim
```

- `tests/golden.rs` replays thirteen scenarios and checks every step
  against numbers recorded from the browser game: flight, towing, a
  three-piece train, impacts and a rock breaking, banking, the carrier
  hull, the field walls, and for combat the cannon breaking a rock, a wreck
  cut open (ejections, the break, its crab and pebbles), a rival stealing
  and severing your line and escaping, a cargo smash that scatters a
  wingmate plus a cannon kill crossing a grudge stage, crabs crawling,
  chewing and being spun off, and a reactor blast. They agree to the last
  bit or two of an f64; the tolerance is 1e-7.
- `tests/rules.rs` checks hooking, winch capacity, reeling, drop and cut,
  banking and deliveries, bay repair, shields, losing the tug, the shift
  clock, seeded field generation, and for combat gun heat and overheating,
  reactors priming on impact, rivals escaping with loot, arrivals and the
  rival cap, grudge stages and bounties, the bay and banking killing crabs,
  the under-attack alarm's rate limit and wreck contents.
- The game crate's own tests check the sound engine's filter and
  compressor (`cargo test -p cj-game`).

To re-record the traces after changing the browser game (needs Docker; run
from the repository root, after rebuilding `claim-jumpers.html`):

```bash
docker run --rm -v "$PWD:/work" -v /work/tests/node_modules -w /work/tests mcr.microsoft.com/playwright:v1.49.1-noble sh -c "npm ci --no-audit --no-fund && node /work/native/tools/golden/export.mjs"
```

## Known differences from the browser version (Milestone 2)

Not yet ported (each arrives with its milestone): the end-of-shift report,
economy, shop and parts (twin cutter, side thrusters, grinder slugs and
the rest), cannon levels, crews, faction standing, saves and title screen
(M3); collectors, law, miners, the refinery, missiles, sentries, sealed
holds and the afterburner with its Engine meter (M4); field events, boss
crews, the tutorial, music, Marge's radio and voice (radio lines are
raised but not yet shown or spoken), hit-stop, the highlight reel,
settings, records, daily shifts and co-op (M5). Placeholders, clearly
temporary:

- Pause shows "PAUSED" instead of the pause menu.
- When the clock runs out, "SHIFT OVER" and Enter for a new shift replace
  the end-of-shift report.
- Mining charges, ingots, keycards and rogue cores (which only come from
  milestone 4 systems, so cannot appear yet) are drawn as a plain
  outlined disc.
- The HUD shows only what exists: banked, on the line, shift, rival
  stage, the under-attack warning, clock, shield, hull, heat and winch.
- The rival grudge starts every shift at 0; carrying it between shifts
  (and cooling it) arrives with the career in milestone 3.
- Sound is mono and synthesized by a port of the browser's WebAudio
  graph (same oscillators, filters, envelopes and compressor settings);
  it should sound alike but has not been compared side by side.
- Rock highlights are a soft sprite instead of a gradient clipped to the
  rock's outline.

## Build and run the M0 spike

```bash
cargo run -p m0-spike --release
cargo run -p m0-spike --release -- --smoke
```

## Verified so far

- Windows 11 Pro (build 26300), release build run from an unrelated
  folder: all M0 checks passed (see the Milestone 0 report in the commit
  history).
- Windows 11 Pro (build 26300): the M1 release build's `--smoke` run, from
  an unrelated folder, exits cleanly with the scripted tow in place.
- Windows 11 Pro (build 26300): the M2 release build's `--smoke` run, from
  an unrelated folder, exits cleanly with the cannon firing, a rival
  shooting back, the sound stream open and the screenshot as expected.
  Hands-on keyboard and gamepad play, and listening to the sound, have not
  yet been verified.
- **Windows 10: not yet verified.**
