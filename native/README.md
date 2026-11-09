# Claim Jumpers: native Windows port

A native Rust port of Claim Jumpers for Windows 10 and 11. This is the lead
version of the game. The browser game (`claim-jumpers-game`) it was ported
from is a frozen baseline at its 1.0.7, and new gameplay is built here first.

**Status: Milestone 6, release 1.2.0 (all systems ported, packaged for Windows; history in [CHANGELOG.md](CHANGELOG.md)).** A career of
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
arrows are in.

Marge is on the radio: every line from the browser, shown in the
bottom-left panel with its throttle and once-per-shift rules, and spoken
through the Windows speech synthesizer (her low, dry delivery, the
punchline lower and slower, and the cockpit callouts in a second voice
after a caution chime). Adaptive music plays under everything, with drums
and a lead coming in as danger rises. A career's first shift runs the
guided training (five steps; T skips it), cargo smashes and boss kills
hit-stop and slow the action, fast cargo leaves trails, and the shift's
best moment is replayed on the report. Marge also comments on your last
shift in the shop.

The title screen has the rest of the browser's menus: a pilot name, the
leaderboard (best shifts on this PC per difficulty, and the daily
challenge), the sound test, the daily challenge (one seeded field per UTC
day, with a fixed loadout, that leaves the career alone), replaying the
training, records (lifetime stats, the kill tally and thirteen
achievements, which unlock eight paint jobs for the tug), and settings
(music, reduced shake and flashing, a colorblind-safe palette, local
co-op and key rebinding). With co-op on, a second player flies a partner
tug from the numpad or a second gamepad and can latch onto the tail of
the cable to haul together. Every menu works with keyboard, mouse or
gamepad (typing the pilot name needs a keyboard).

| Folder | What it is |
|---|---|
| `crates/cj-sim` | The gameplay simulation: pure Rust, no window or audio, deterministic. |
| `crates/cj-game` | The Windows game (`claim-jumpers.exe`): window, rendering, input, sound, HUD, menus and saves. |
| `tools/golden` | Exported the original traces from the browser game (kept for reference). |

## Stage build

For trying a change before it ships, from the `native` folder:

```bash
powershell -ExecutionPolicy Bypass -File tools/stage.ps1
```

It builds the working tree, runs the `--smoke` check and refreshes `stage\`.
Run `stage\play-stage.cmd` to play it: the window title says `[STAGE]`, and
saves go in `stage\saves`, so it never touches your real career and test
careers survive rebuilds. `stage\BUILD.txt` records what was built.

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
   Rust 1.89 or newer is required (developed with 1.99.1).
2. Install the Visual Studio C++ build tools (or Visual Studio with the
   "Desktop development with C++" workload), which provide the linker.

## Build and run the game

From the `native` folder:

```bash
cargo run -p cj-game --release
```

The executable is `target\release\claim-jumpers.exe`. It is self-contained:
fonts and the icon are embedded, and the C runtime is linked statically
(`.cargo/config.toml`), so it needs only DLLs that ship with Windows and can
be copied anywhere and run without Rust or the Visual C++ Redistributable.

## Release package

From the `native` folder:

```bash
powershell -ExecutionPolicy Bypass -File tools/package.ps1
```

It builds the release exe, runs the `--smoke` check from an unrelated
folder (stopping if it fails or hangs), and writes
`dist\ClaimJumpers-<version>-windows-x64.zip` with:

- `claim-jumpers.exe`;
- `README.txt`, the player's guide (from `packaging/README.txt`):
  requirements, how to run it (including the SmartScreen prompt for an
  unsigned exe), controls, where saves live, uninstalling and
  troubleshooting;
- `licenses/`, the fonts' SIL Open Font Licence texts and their sources;
- `THIRD-PARTY-NOTICES.txt`, every crate compiled into the exe with its
  licence and licence files (written by `tools/third_party.py` from
  `cargo metadata`).

The version is `crates/cj-game/Cargo.toml`'s; `build.rs` puts it, with the
icon, into the exe's Windows resources (Details tab), using the Windows
SDK's `rc.exe` (installed with the C++ build tools; without it the build
warns and leaves the icon out). The icon is drawn by `tools/make_icon.py`
(the Mk I tug, standard library only), which writes
`assets/icon/claim-jumpers.ico` and the raw window-icon images. There is no
installer: the zip unpacks and runs in place, and the saves live in
`%APPDATA%\ClaimJumpers`. The exe is not code-signed.

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
| Marge's voice on / off | V | |
| Music on / off | N | |
| Skip training | T | |

The flight keys can be rebound in Settings. With local co-op on, P2 flies
from the numpad (8 thrust, 4 / 6 turn, 5 or 2 brake, 0 fire, Enter latch)
or from the last connected gamepad (left stick turn, RT or A thrust, LT
brake, RB fire, Y latch), and player 1 leaves that gamepad alone.

In menus: arrow keys, D-pad or left stick move between buttons; Enter,
Space or A press; LB / RB switch tabs; Start presses the screen's main
button. The mouse works too.

The career, best shifts and chosen difficulty are saved in
`%APPDATA%\ClaimJumpers` (`career.json`, `best.json`, `mode.txt`), with
the settings (`settings.json`), the profile of records and achievements
(`profile.json`), the local leaderboard (`board.json`), the pilot name
(`pilot.txt`) and whether training is done (`training.txt`), all in the
browser's field names. Saves are written atomically, and a malformed save
is repaired on load the way the browser repairs one. A career file that
can't be read at all is copied to `career.bad.json` before a new career
replaces it, and a save that fails shows a warning on screen. Set
`CLAIM_JUMPERS_DATA` to use another folder.

Unattended check: a scripted shift with a tow, the cannon, a rival, a
crab, missiles and sentries, the refinery waking its drones, a debt
collector, a mining crew, two wanted stars, an ion storm, a boss crew, an
escorted hauler, a sealed hold and its keycard and a contract, then the
pause card, the shift report, a purchase in the shop, the shop's Parts
tab, accepting a contract and hiring a co-pilot on Jobs & crew, buying a
truce on Map & standing, the title, settings, records, the leaderboard
and the sound test (playing one sound), with a screenshot of each in
`%TEMP%\claim-jumpers-m5-<screen>.png` and a report. It exits non-zero
unless all twelve screens were captured and the career on disk reached
shift 2. It uses (and then
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
  these systems (tests/specs in the `claim-jumpers-game` repository): collectors only net
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
  self-sealing hull mounts and repairs the hull, and that training holds
  the rivals and the field event back and advances on each action.
- `tests/rules.rs` checks hooking, winch capacity, reeling, drop and cut,
  banking and deliveries, bay repair, shields, losing the tug, the shift
  clock, seeded field generation, and for combat gun heat and overheating,
  reactors priming on impact, rivals escaping with loot, arrivals and the
  rival cap, grudge stages and bounties, the bay and banking killing crabs,
  the under-attack alarm's rate limit and wreck contents.
- The game crate's own tests check the sound engine's filter and
  compressor, the save files on disk, menu focus movement, how Marge's
  lines are tidied and split into sentences for speech, settings loading
  (validation, round trip) and key rebinding (swaps and reserved keys),
  the profile (validation, unlocking once, paint jobs needing their
  achievement), pilot-name cleaning, the local board (top 20 per mode,
  the last three dailies), the daily field being the same twice for one
  day and the browser's FNV-1a seed, and night shifts shortening radar.
- `tests/field.rs` also ports the browser's co-op test (the partner
  latches onto the cable's tail and pulls) and checks that a cargo smash
  raises hit-stop, a highlight and the achievement.

Marge's lines are generated from the browser's tables by
`node tools/gen_lines.cjs .. <claim-jumpers-game checkout>` (run from `native`),
which writes `crates/cj-game/src/lines.rs`.

The golden traces began as recordings of the browser game. This code now
leads, so after a deliberate change re-record them from the simulation and
check that only the expected ones moved:

```bash
UPDATE_GOLDEN=1 cargo test -p cj-sim --test golden
```

To re-record the traces and career answers from the browser game instead
(needs Docker and a checkout of `claim-jumpers-game` with `claim-jumpers.html`
built; run from this repository's root, with `CJ_BROWSER_REPO` set to that
checkout's path):

```bash
docker run --rm -v "$PWD:/work" -v "$CJ_BROWSER_REPO:/browser" -e CJ_BROWSER_REPO=/browser -v /browser/tests/node_modules -w /browser/tests mcr.microsoft.com/playwright:v1.65.0-noble sh -c "npm ci --no-audit --no-fund && node /work/native/tools/golden/export.mjs && node /work/native/tools/golden/career.mjs"
```

## Differences from the browser's 1.0.7 baseline

Added here since the browser was frozen: radiation clouds from reactors, a
shockwave when the tug is wrecked, wanted stars fading while nobody chases
you, your shots passing through towed cargo, rivals sidestepping shots,
missiles that can lose their lock, level 5 cannon lasers, refinery charges
that come back, crews adapting to how you fight, rival standing that fades
between shifts, and rival crews that buy upgrades with stolen cargo.

Everything in the browser game is ported. Deliberate differences:

- The leaderboard is this PC's only (the browser can also post to a
  shared online board). The browser's save export and import are left
  out: the save folder above is plain files that can be copied.
- The in-play key hint and the title's keyboard guide show the default
  keys; rebound keys show in Settings.
- Lifetime kills and stats are added to the profile when a shift ends
  (the browser adds each kill as it happens), so a shift abandoned by
  closing the window doesn't count.
- The daily field is the same for every player of this port on a given
  day, but not the same field the browser generates that day (the two
  draw their random numbers in different orders).
- Marge's voice setting is kept in `settings.json` (the browser keeps it
  in its own localStorage key).

- Contract offers are drawn with a Fisher-Yates shuffle. The browser
  shuffles with `sort(() => Math.random() - 0.5)`, whose bias depends on
  V8's sort; the offers come from the same pool with the same chances
  otherwise.
- Boss hulls are drawn 1.4 times the size, as in the browser, but keep
  their normal line width (the browser's canvas scale thickens the
  outline too).
- The night shift's darkness is a vertex-coloured ring rather than a
  canvas radial gradient; the radii and colours are the browser's.
- Marge's voice is the Windows speech synthesizer, not the browser's;
  she prefers the same voices by name (Hazel, Susan and so on, then Zira),
  with the browser's pitch and rate. Which voices exist depends on the
  Windows language packs installed.
- The music's kick drum falls in pitch over its whole 0.2 s note; the
  browser's reaches 40 Hz at 0.15 s.
- Whether training has been done is kept in `training.txt` in the save
  folder (the browser keeps it in localStorage).
- None of the game's fonts has the ★ and ☆ glyphs (the browser falls back
  to a system font), so wanted stars are drawn as star shapes.
- Enemy bolts hit the hauler's drawn hull. (The browser uses a circle
  smaller than the ship, so bolts cross its tail and corners.)
- Opening Settings from the pause card freezes the shift behind it. (The
  browser runs its menu drift there, which moves the paused shift's cargo
  and clock.)
- Added for the desktop: a Quit button on the title screen, and arrow
  keys moving menu focus (the browser relies on Tab and the mouse there).
- Sound is mono and synthesized by a port of the browser's WebAudio
  graph (same oscillators, filters, envelopes and compressor settings);
  it should sound alike but has not been compared side by side.
- Rock highlights are a soft sprite instead of a gradient clipped to the
  rock's outline.

## Verified so far

Earlier per-milestone checks (M0 to M5) are recorded in the changelog under "Development
milestones".

- Windows 11 Pro (build 26300): the 1.0.0 package, built by
  `tools/package.ps1`, unzipped into a fresh folder and run from an
  unrelated working directory, passes the `--smoke` tour (exit 0, twelve
  screens, sound and speech found). The exe shows its icon and version
  1.0.0 in Explorer, and `dumpbin /dependents` lists only Windows system
  DLLs (no VCRUNTIME140.dll).
- Not yet verified: hands-on keyboard, mouse and gamepad play, gamepad
  menu navigation, rebinding a key by hand, two-player co-op on real
  controllers, flying a daily challenge, the colorblind palette and
  reduced motion on screen, and listening to the sound, music and
  Marge's voice.
- **Windows 10: not yet verified.**

## License

No license file is present and no license is granted for the code in this repository. It is a
private repository. The bundled fonts keep their own SIL Open Font License terms (see
`licenses/` in the release package).
