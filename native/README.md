# Claim Jumpers: native Windows port

A native Rust port of Claim Jumpers for Windows 10 and 11. The browser game
in the repository root (`claim-jumpers.html`, built from `src/`) is the
reference implementation; the port must match its gameplay, rules,
controls, balance and look unless a change is explicitly approved.

**Status: Milestone 4 (the opposition).** A career of five-minute shifts:
fly the tug, tow and fight for salvage, and bank it at the carrier, against
rival crews, debt collectors who net and tow the tug (or confiscate a fat
load, or impound it), the law (wanted stars, patrols, shakedowns, pursuit,
stun bolts, arrests and fines, up to Galactic SWAT), mining drones that cut
the field into debris storms, and the Kessler refinery (charges, the vault,
security drones and the jaws). The afterburner, auto-defense missiles and
torpedoes, and sentry drones (which go rogue if the tug is wrecked) are all
in. After each shift the report settles the loan installment, and the
machine shop sells parts and system levels; paying a loan off trades the
tug up a class. Title, pause, report and shop work with keyboard, mouse or
gamepad. The remaining systems arrive in milestone 5 (see "Known
differences" below).

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
collector, a mining crew and two wanted stars, then the pause card, the
shift report, a purchase in the shop, the shop's Parts tab and the title,
with a screenshot of each in `%TEMP%\claim-jumpers-m4-<screen>.png` and a
report. It uses (and then
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
  load, and seven end-of-shift settlements (continuing, overdrawn,
  repossessed, two trade-ups, the fleet paid off, standing changes). It
  also checks save round-trips, the shop's rules and sentry purchases.
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

## Known differences from the browser version (Milestone 4)

Not yet ported (all in milestone 5): sealed holds and their keycards,
escorted haulers, contracts, crews and wages, the sector map and belts
(including the lawless Frontier), belt conditions, the standing screen,
contract chains (and the unique parts they award), field events, boss
crews, the tutorial, music, Marge's radio and voice (radio lines are
raised but not yet shown or spoken), her shop commentary, paint jobs,
hit-stop, the highlight reel, settings and key rebinding, records and
achievements, the leaderboard and pilot name, the sound test, daily
shifts and co-op. Placeholders, clearly temporary:

- The unique chain parts are listed as locked, as in the browser; the
  self-sealing hull's effect arrives with them.
- The shop has its Tug and Parts tabs; Jobs & crew and Map & standing
  arrive in milestone 5. The title screen leaves out the leaderboard,
  sound test, daily challenge, training, records and settings buttons
  until their screens exist.
- The report's contract and crew-wage rows read "None" until contracts
  and crews exist. Standing changes are tracked and saved (they move the
  bank's installment rate and the rivals' grudge, as in the browser), but
  only the events that already exist can move them.
- The HUD shows what exists: banked, on the line (a sealed pod's value
  shows as "+ ?"), wallet, slugs, missiles, sentries, shift, wanted
  stars, rival stage, the under-attack warning, clock, shield, hull,
  heat, engine and winch (the contract line arrives with contracts).
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
  and all six screenshots look as expected. Hands-on keyboard, mouse and
  gamepad play, gamepad menu navigation, and listening to the sound have
  not yet been verified.
- **Windows 10: not yet verified.**
