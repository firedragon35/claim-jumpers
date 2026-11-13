# Changelog

All notable changes to the native Windows port of Claim Jumpers. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the port uses [Semantic Versioning](https://semver.org/) as defined in
[CLAUDE.md](CLAUDE.md#versioning).

## [Unreleased]

## [1.9.0] - 2026-11-12

### Added

- F3 shows a frame-time overlay on any screen: frames per second, the
  average frame time, the slowest frame of the last two seconds (red when
  it's below 60 fps), the particle count and whether effects are on. It is
  hidden by default and not saved.

### Changed

- The player README lists F3, mentions the Glow and effects and HUD size
  settings, and suggests F3 and turning effects off when the game runs
  slowly.

## [1.8.0] - 2026-11-12

### Added

- Thruster trails: a burning engine leaves a short glowing ribbon behind the
  tug that thins and fades over a third of a second. Every pilot's tug has
  one, online too.
- A rock that breaks throws tumbling chunks along with its dust, lit like
  the rocks and edged in gold when it held ore.

### Changed

- The cable whitens smoothly as it tightens, and a soft halo builds around
  it, instead of switching colour at one tension.

## [1.7.0] - 2026-11-12

### Added

- Settings: "HUD size" cycles 100%, 125% and 150% (saved as `hudScale` in
  `settings.json`; 100% by default). It makes the in-flight HUD bigger:
  readouts, bars, radar, Marge's dispatches, the training card, chat and
  the pilot list. Text is drawn at the size it is shown, so it stays sharp.
  The HUD always keeps at least a 1280x720 layout, so on smaller windows the
  size is capped to stop its cards overlapping. The arrows at the screen
  edge stay as they are, since they point at things in the field.

### Changed

- The rows on the Settings screen sit a little closer together to make room
  for the new one.

## [1.6.0] - 2026-11-12

### Changed

- Rocks and wrecks are lit from one direction. Their bodies shade from a
  lit upper-left side to a dark far side, and their outlines catch a
  bright rim on the lit edges and fade on the shadowed ones, so they read
  as solid objects rather than outlines. Wrecks also show an inner line of
  hull plating. This needs no effects and is always on.
- With Glow and effects on, a radiation cloud is a churning green gas with
  a pulsing rim, and the edge of the belt is a rippling energy wall that
  brightens as you fly toward it. The dashed lines still mark the exact
  hazard edge and field edge, fainter.

## [1.5.0] - 2026-11-12

### Added

- Glow and effects. The field is drawn off-screen and finished with a bloom
  pass, so bright things (thrusters, shots, the cable, cargo, sparks) glow
  while rocks and the background stay dark. A gentle colour grade and faint
  film grain finish the frame; reduced motion holds the grain still. The HUD
  and menus are drawn after it and stay sharp.
- A procedural nebula behind the stars, in the same colours and with the
  same parallax as the old nebula glows, which it replaces while effects
  are on.
- Settings: "Glow and effects" turns all of it off (on by default; saved as
  `effects` in `settings.json`). If the graphics driver can't compile the
  effects, they stay off and the field draws as before.

## [1.4.6] - 2026-11-11

Fixes from a third repository audit.

### Fixed

- An online game could crash when a new shift started. If a player had left
  during the last shift, the others moved down a pilot slot; a player whose
  Welcome to the new shift was lost on the way (it is resent 300 ms later)
  took the new shift's first snapshots with their old pilot number, which
  could be past the new shift's pilots. A snapshot made for another pilot is
  now dropped until the Welcome arrives.
- Hosting used more upload than it needed to. Tugs, salvage, rocks and shots
  sent every number at full size 20 times a second; their timers and levels,
  which mostly rest at zero, now travel as one byte while they do, and their
  headings and spins at half size. A snapshot is a quarter smaller with the
  pilots spread out and two fifths smaller with everyone at the bay: about
  0.55 Mbit/s per pilot, where it was 0.75. Hull, shield and credit figures
  are sent exactly, as before.
- A shop request that went through was saved on the spot, waiting for the
  disk on the server's step loop, so a changed client sending requests as
  fast as the rate limit allows could make the break stutter for everyone.
  The server now saves changed careers once a second, when the next shift
  starts, when a lobby player leaves, and when it stops.
- A player whose game crashed or lost its network, and who joined again
  before the server had noticed, was told their pilot was already flying
  and had to wait for the old connection to time out. With the right
  identity the new connection now takes over the pilot (or the place in the
  lobby) at once, and the old one is told why it was disconnected.

### Changed

- `tools/package.ps1` stops if the working tree has uncommitted changes, and
  prints the commit it packages, so every release zip matches a commit.
- The player guide says how much upload hosting needs (about 0.6 Mbit/s for
  each pilot, more while everyone is in one place), and the multiplayer
  design note carries the measured figures beside its estimate.

## [1.4.5] - 2026-11-11

Fixes from a second repository audit.

### Fixed

- The shop's contract offers and every belt's condition were only saved
  once the player bought or picked something, so going back to the title
  and pressing Continue rolled a fresh set, as often as the player liked.
  Online, a player waiting in the lobby could do the same by leaving and
  joining again during the break. The roll is now saved as soon as it is
  made, offline and on servers.
- A script that joined a server with a new player id and left the lobby,
  over and over, wrote a key and a career to disk each time without end,
  flushing each one on the server's step loop. A server now lets in at most
  as many players new to it per minute as its player limit; past that a
  new id is told the server is busy and to try again in a minute, and
  nothing is written. Players the server already knows still get in.
- A hosted server's `server.log` was only started again when hosting began,
  so a long session could grow it without limit. It now starts again past
  1 MB during a session too, keeping the full one as `server.old.log`.
- A pilot whose name was only in scripts the game's fonts can't draw
  became "Pilot N" when joining mid-shift but "New pilot" in the lobby, so
  two such players in the lobby looked the same. Each now gets a
  "Pilot N" nobody else on the server has.

### Changed

- The Join screen and the player guide now say that the game sends each
  server you join your pilot's identity (`mp_id.txt`, the same for every
  server), so a host could use it to play as you elsewhere: join only hosts
  you trust.

## [1.4.4] - 2026-11-11

A follow-up fix from the independent audit.

### Fixed

- When a server couldn't save the key that binds a career to its owner's
  secret, it let the player in anyway and saved their career with no key,
  so the next player using that id with any secret could claim it. The
  server now turns the player away and writes no career until the key can
  be saved. A key that can't be read turns them away too, as before.
- A player turned away for that reason is now told the server couldn't
  save their pilot and to ask the host to check its disk, instead of being
  told the server knows their pilot from another install.

## [1.4.3] - 2026-11-11

More online fixes from the independent audit.

### Fixed

- A server loaded a saved career by the player's id alone, so once it
  had restarted, or had forgotten a player who left, anyone using that id
  with any secret could take over the career. Each career on a server now
  belongs to the secret that first claimed it, and any other is refused.
  The server keeps only a SHA-256 fingerprint of the secret
  (`<player>.key` beside the career), never the secret itself. Careers
  saved by 1.4.2 and earlier keep working: the first player back claims
  them.
- Online, a player who reconnected during the break between shifts got
  their tug back but couldn't shop or vote for the next belt until they
  had flown another shift. They now get their career as they return.

### Changed

- The server uses the `sha2` crate (RustCrypto) for those fingerprints.

## [1.4.2] - 2026-11-11

Online fixes from an independent audit: one player's shift no longer
changes another's.

### Fixed

- Online, the end of a shift gave each pilot with a drone bay every
  pilot's surviving sentries, so drones multiplied from shift to shift
  past the bay's limit. Each career now gets back only its own.
- Online, a pilot who joined a shift already under way (including the
  first player into a freshly started server) flew without their sentries,
  and the end of the shift then saved their bay empty. They now launch on
  arrival.
- Online, a debt collector impounding one tug ended the shift for
  everyone. Now only that tug leaves the field, for the rest of the shift
  (its load floats free and its stars are cleared, as for a wreck); the
  others fly on, and the shift ends early only once every tug in it is
  impounded. The impounded player's HUD says "IMPOUNDED · BACK NEXT SHIFT".
  Single player is unchanged.
- Online, a player who arrived during the break between shifts could not
  open the shop until they had flown a shift. They now get their career,
  with the break's offers, as they arrive.
- Online, arresting one pilot called off the law units chasing other
  wanted pilots. Only the arrested pilot's pursuers stand down now.

## [1.4.1] - 2026-11-11

Fixes from the repository audit.

### Fixed

- Online, a player who lost the connection and came back in the same
  shift could not steer, hook, drop, cut or switch PvP for about as long
  as they had already flown: the server kept the old connection's message
  numbers and dropped the new connection's as repeats.
- Online, paying off the loan in the break traded the tug up without a
  word (the shop kept showing its last sentence). It now says
  "Loan paid off. Traded up: …" with the new tug and loan.
- Pilot names keep only letters the game's fonts can draw (ASCII and
  Latin), as chat lines already did. Other scripts showed as blanks on
  every player's screen, and Cyrillic or Greek lookalikes let one player
  pass as another. A saved name or local leaderboard entry in another
  script loses those letters when it is loaded.
- A server that falls more than 0.25 s behind (the PC slept, a VM was
  paused) skips the lost time instead of running every missed step at
  once, which jumped the shift clock and flew the tugs on stale controls.

### Changed

- Single player and the server apply shop requests through one function
  (`cj_net::ShopReq::apply`), so the two shops can't drift apart. An offer
  number past the offers is refused there instead of being used as an
  index.

## [1.4.0] - 2026-11-11

Opt-in PvP on servers whose host allows it, and a play-test cut-off.

### Added

- PvP, off unless the host allows it (Launch screen: "PvP: On"; headless
  server: `--pvp`). Everyone starts each shift Protected. B (D-pad up)
  in the bay starts 10 seconds of arming; then the tug is PvP, shown to
  every pilot by a red ring and a PVP tag, and to its own pilot on the
  HUD. A PvP pilot can hurt any tug; whoever they hurt may fire back for
  30 seconds after each attack. Ramming hurts only between two PvP tugs.
  The PvP key can be changed under Settings; if an earlier version's
  settings already put another action on B, that action keeps it and PvP
  moves to a free key.
- A combat lock: attacking, or being hit while PvP, holds a pilot in PvP
  for 30 seconds. PvP can't be switched off (in the bay) during it, and a
  player who leaves mid-fight leaves the tug in the field until it ends.
- On belts with law, the first hit in a fight earns the attacker a wanted
  star and a PvP wreck two more; firing back earns none. A PvP wreck's
  cargo is held for 20 seconds for its owner and PvP pilots, PvP wrecks
  pay no bounty, and the Tab list counts them as kills.

- A copyright notice: "Copyright © 2026, 2027 Neil Bruggeman. All rights
  reserved." on the opening screen, in the player README and on the
  exe's Details tab.
- Play-test builds stop working on 1 June 2027. From then on the game
  shows only a screen saying the build has expired, and asks the player
  to get a newer one; saves are left untouched. `--smoke` fails past
  that date. The date comes from network time (`time.google.com`), so
  setting the PC's clock back doesn't help; offline, the PC's clock is
  used.

## [1.3.0] - 2026-11-11

Online co-op: up to 20 pilots share one shift.

### Added

- The game opens asking how to play: Single player, Host server or Join
  remote host. Hosting runs the server in your game, for up to 2 to 20
  pilots (the field grows with the crowd) at a career difficulty you pick.
  Joining looks the server up first and shows how many pilots are on it,
  or that it is full. The port is always 47820 (UDP). There's also a
  headless `cj-server` for running a server without the game.
- Your own tug and shots respond at once, however far away the server is.
  The server decides everything, and corrections stay small even over a
  laggy link that loses packets.
- Every pilot's tug and tow line is drawn, each other pilot in their own
  colour. Nearby players' sounds fade with distance, and the radar shows
  what the server reports beyond your view.
- Tab shows every pilot with this shift's credits, kills and deaths.
  Enter opens a public chat, shown in the bottom-left corner over the
  field.
- Shift follows shift. In the break you see your own report, shop with
  your career on that server (every purchase is checked by the server),
  and vote for the next belt among those everyone's tug can fly. The next
  shift starts when everyone is ready, or after 90 seconds.
- Each server keeps its own career for each player, separate from your
  single-player career, in the difficulty the host chose. A host's
  careers are kept for the next time they host.
- Players who join in a shift's last minute wait in the lobby for the
  next one. A player who drops out keeps their tug and cargo safely out of
  the field until they return.
- Hints when nobody can get in: allowing the game through Windows
  Firewall, and the address to give out. The player guide has a new
  PLAYING ONLINE section, covering LAN play, port forwarding and Tailscale.

### Changed

- Other players' shots stop at your tug, but never hurt it. Damage between
  players comes with opt-in PvP in a later version.
- Radiation clouds burn every tug inside them. Rival and law bolts hit any
  tug in their path, and a refinery drone takes turns between the tugs in
  range.
- Long messages wrap on screen instead of running off a narrow window.

### Notes

- Online play is IPv4 only, and the connection is not encrypted.

## [1.2.0] - 2026-11-09

### Added

- Rival crews buy upgrades with cargo they steal: tougher hulls at 3,000 cr,
  then faster engines at 6,000, then quicker guns at 10,000 (level 2 at most
  on Relaxed). Taking down a boss crew empties half their war chest. The
  report says when they upgrade, and Map & standing shows their level.

### Changed

- Rival standing fades 10% toward zero each shift, unless that shift pushed
  it further from zero.

## [1.1.0] - 2026-11-09

This version is the lead: the browser game is now a frozen baseline at 1.0.7.

### Added

- A destroyed reactor leaves a radiation cloud for 25 seconds. Inside it,
  shields are out and the hull burns slowly. Clouds never settle over the bay.
- A wrecked tug throws out a shockwave that shoves and damages what's nearby.
- Wanted stars fade one every 20 seconds while nobody is chasing you.
- Level 5 cannons fire instant lasers.
- Rivals hunting you sidestep incoming shots: skiffs most, brutes never.
- Missiles and torpedoes can lose their lock, less often at higher levels.
- Crews adapt to how you usually kill them, and Marge warns you when they do.
- New refinery charges arrive if none are left and the vault is still shut.
- `tools/stage.ps1` builds a separate stage copy with its own saves.

### Changed

- Your shots pass through cargo you're towing, including reactors.

### Fixed

- Enemy bolts hit the escorted hauler's whole drawn hull, so they no longer
  pass through its tail and corners.

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
