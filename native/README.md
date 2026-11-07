# Claim Jumpers: native Windows port

A native Rust port of Claim Jumpers for Windows 10 and 11. The browser game
in the repository root (`claim-jumpers.html`, built from `src/`) is the
reference implementation; the port must match its gameplay, rules,
controls, balance and look unless a change is explicitly approved.

**Status: Milestone 5, part 1 (the career's field systems).** A career of
five-minute shifts: fly the tug, tow and fight for salvage, and bank it at
the carrier, against rival crews, debt collectors who net and tow the tug
(or confiscate a fat load, or impound it), the law (wanted stars, patrols,
shakedowns, pursuit, stun bolts, arrests and fines, up to Galactic SWAT),
mining drones that cut the field into debris storms, and the Kessler
refinery (charges, the vault, security drones and the jaws). The
afterburner, auto-defense missiles and torpedoes, and sentry drones (which
go rogue if the tug is wrecked) are all in.

Each shift is worked on a belt picked from the sector map (five belts, from
the Quiet Drift to the lawless Frontier, each needing a tug class), under
that shift's belt condition (ore boom, patrol sweep, salvage glut, rival
surge, ion front or night). Somewhere in the shift a field event strikes:
an ion storm that drops the shields, a derelict freighter drifting in, a
named boss crew with a bounty, or a hauler to escort home. Sealed holds
open only with their keycard, and rogue sentries leave cores to store,
sell or reprogram. The machine shop now has all four tabs: the tug, parts,
jobs and crew (three contract offers a shift, contract chains that award
unique parts, and four co-pilots with a wage and a perk), and the map and
standing (bank, law and rivals, and a truce with the crews). The report
settles the contract and the wage with the installment. The HUD shows the
contract, the belt and its condition, and the radar minimap and off-screen
arrows are in. Title, pause, report and shop work with keyboard, mouse or
gamepad. The rest of milestone 5 follows (see "Known differences" below).

| Folder | What it is |
|---|---|
| `crates/cj-sim` | The gameplay simulation: pure Rust, no window or audio, deterministic. |
| `crates/cj-game` | The Windows game (`claim-jumpers.exe`): window, rendering, input, sound, HUD, menus and saves. |
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
| Strafe (side thrusters) | Z / C | Right stick |
| Grind last piece (grinder) | G | X |
| Afterburner | Shift | LT + RT together |
| Pause | P or Esc | Start |
| Mute | M | Back |

In menus: arrow keys, D-pad or left stick move between buttons; Enter,
Space or A press; LB / RB switch tabs; Start presses the screen's main
button. The mouse works too.

The career, best shifts and chosen difficulty are saved in
`%APPDATA%\ClaimJumpers` (`career.json`, `best.json`, `mode.txt`), in the
browser's field names. Saves are written atomically, and a malformed save
is repaired on load the way the browser repairs one. Set
`CLAIM_JUMPERS_DATA` to use another folder.

Unattended check: a scripted shift with a tow, the cannon, a rival, a
crab, missiles and sentries, the refinery waking its drones, a debt
collector, a mining crew, two wanted stars, an ion storm, a boss crew, an
escorted hauler, a sealed hold and its keycard and a contract, then the
pause card, the shift report, a purchase in the shop, the shop's Parts
tab, accepting a contract and hiring a co-pilot on Jobs & crew, buying a
truce on Map & standing, and the title, with a screenshot of each in
`%TEMP%\claim-jumpers-m5-<screen>.png` and a report. It uses (and then
deletes) its own temporary save folder, never the player's. The release
build has no console window, so redirect the report to a file:

```bash
target/release/claim-jumpers.exe --smoke > smoke.txt
```

## Tests

```bash
cargo test
```

- `tests/golden.rs` replays twenty-four scenarios and checks every step
  against numbers recorded from the browser game: flight, towing, a
  three-piece train, impacts and a rock breaking, banking, the carrier
  hull, the field walls, and for combat the cannon breaking a rock, a wreck
  cut open (ejections, the break, its crab and pebbles), a rival stealing
  and severing your line and escaping, a cargo smash that scatters a
  wingmate plus a cannon kill crossing a grudge stage, crabs crawling,
  chewing and being spun off, a reactor blast, and for fitted loadouts
  strafing on side thrusters, grinding cargo into slugs and firing them,
  the magnetic scoop, and a hot twin-cutter, armor and winch loadout on a
  Mk III hull, and for the opposition the afterburner overheating, a debt
  collector netting and towing the tug while it tears free, a law
  pursuit with stun bolts, a mining crew cutting rocks, the refinery
  waking its drones, the hungry jaws pulling, eating and being wrecked,
  and missiles, torpedoes and sentries that go rogue when the tug is
  wrecked. They agree to the last bit or two of an f64; the tolerance is
  1e-7.
- `tests/opposition.rs` ports the browser's own Playwright tests for
  these systems (tests/specs in the repository root): collectors only net
  when the payment is at risk; a shakedown roll starts a chase in the same
  frame; a wrecked tug orphans its sentries and rogue kills are not
  blamed on you; missiles ignore collectors and the law until you hit
  them; a wrecked tug, or reaching the bay, clears the wanted level;
  level-5 missiles fire torpedo salvos; chained afterburner bursts
  overheat the engine; and the refinery's jaws pull from half as far and
  can be wrecked.
- `tests/career.rs` checks the career against answers recorded from the
  browser (`golden/career.json`): the loadout numbers for nine loadouts,
  108 installments and the bank's rate, five malformed saves repaired on
  load, and thirteen end-of-shift settlements (continuing, overdrawn,
  repossessed, two trade-ups, the fleet paid off, standing changes, and
  contracts met and missed, crew wages, a chain finished, a chain broken by
  a wrecked tug, a chain advancing, and stored cores at the cap). It also
  checks save round-trips (with contracts, chains, crew, belt, conditions
  and cores), the shop's rules and sentry purchases.
- `tests/field.rs` ports the browser's own tests for the career's field
  systems (tests/specs/phases-6-9.spec.js): bank standing moves the
  installment and a truce drops the grudge a stage; a contract chain's last
  step awards its unique part; an ore boom raises ore prices by half; an
  escorted hauler reaching the bay meets the escort contract; a sealed hold
  ignores damage and opens with its keycard; and a rogue sentry's core is
  stored and reprogrammed into a sentry. It also checks that the awarded
  self-sealing hull mounts and repairs the hull.
- `tests/rules.rs` checks hooking, winch capacity, reeling, drop and cut,
  banking and deliveries, bay repair, shields, losing the tug, the shift
  clock, seeded field generation, and for combat gun heat and overheating,
  reactors priming on impact, rivals escaping with loot, arrivals and the
  rival cap, grudge stages and bounties, the bay and banking killing crabs,
  the under-attack alarm's rate limit and wreck contents.
- The game crate's own tests check the sound engine's filter and
  compressor, the save files on disk, and menu focus movement.

To re-record the traces and career answers after changing the browser game
(needs Docker; run from the repository root, after rebuilding
`claim-jumpers.html`):

```bash
docker run --rm -v "$PWD:/work" -v /work/tests/node_modules -w /work/tests mcr.microsoft.com/playwright:v1.49.1-noble sh -c "npm ci --no-audit --no-fund && node /work/native/tools/golden/export.mjs && node /work/native/tools/golden/career.mjs"
```

## Known differences from the browser version (Milestone 5, part 1)

Not yet ported (the rest of milestone 5): the tutorial, music, Marge's
radio and voice (radio lines are raised but not yet shown or spoken), her
shop commentary, paint jobs, hit-stop, the highlight reel, settings and
key rebinding, records and achievements, the leaderboard and pilot name,
the sound test, daily shifts and co-op. Placeholders, clearly temporary:

- The title screen leaves out the leaderboard, sound test, daily
  challenge, training, records and settings buttons until their screens
  exist.

Deliberate differences:

- Contract offers are drawn with a Fisher-Yates shuffle. The browser
  shuffles with `sort(() => Math.random() - 0.5)`, whose bias depends on
  V8's sort; the offers come from the same pool with the same chances
  otherwise.
- Boss hulls are drawn 1.4 times the size, as in the browser, but keep
  their normal line width (the browser's canvas scale thickens the
  outline too).
- The night shift's darkness is a vertex-coloured ring rather than a
  canvas radial gradient; the radii and colours are the browser's.
- None of the game's fonts has the ★ and ☆ glyphs (the browser falls back
  to a system font), so wanted stars are drawn as star shapes.
- Added for the desktop: a Quit button on the title screen, and arrow
  keys moving menu focus (the browser relies on Tab and the mouse there).
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
  look as expected. Hands-on keyboard, mouse and gamepad play, gamepad
  menu navigation, and listening to the sound have not yet been verified.
- **Windows 10: not yet verified.**
