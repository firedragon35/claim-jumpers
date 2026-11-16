# Changelog

All notable changes to the native Windows port of Claim Jumpers. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the port uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.17.1] - 2026-11-16

### Fixed
- The player README's MARGE'S VOICE section described the old Windows speech voices. It now says her lines are recordings built into the game, and points to the V key and the Voice volume if she's silent.

## [1.17.0] - 2026-11-13

Follow-up decisions from the 1.15.0 audit. Online players all need this
version, because the login changed.

### Fixed
- Online, a career's rival crews now grow only from cargo stolen from that pilot, and only the pilot who downs a boss weakens their crews. Before, the whole field's loot and boss kills counted for every career. Marge's shop remarks about bosses follow the same rule.
- Cargo insurance only pays out for your own towed cargo that was destroyed or taken. Cargo lost to an arrest fine, or loose cargo a rival grabbed near you, no longer counts.

### Changed
- Online login no longer sends your secret. The server sends a fresh challenge for each connection and the game answers with a proof, so a captured login can't be replayed. The first time a server sees you, it gets a fingerprint of your identity. Careers and keys saved by older servers still work.
- Stopping a server mid-shift settles the shift first, so nobody loses what they banked. The host's Stop server button shows how many pilots it will disconnect.
- Daily challenges now count toward lifetime records (deaths, rivals scrapped, bosses, kills by hull). Earnings and achievements are still career-only.

### Added
- The bank's insurance offer shows what it covers (60% of towed cargo lost, up to 5,000 cr) and how much you lost last shift.
- Sprite art credits in the player README.

## [1.16.0] - 2026-11-13

Bug fixes and quick improvements from the 1.15.0 audit.

### Fixed
- Online, a crab latched onto one tug slowed everyone's thrust.
- Online, leaving from the pause card now takes two presses and says what it does ("Leave server", or "Stop server" for the host, which ends the shift for everyone). It also no longer claims the shift is paused.
- Escape and gamepad B go back from the Join, Settings, Records, Leaderboard and Sound test screens.
- Quitting a daily challenge from the pause card shows its report.
- The window no longer freezes while a host name is looked up on the Join screen.
- A damaged profile.json, board.json or settings.json is kept as a .bad.json copy with a warning, instead of being silently replaced with defaults.
- The "hull low" call, the hull alarm and the HUD's hull colour all trigger at 30% of the tug's own hull, so armour no longer throws them out of step.

### Added
- New settings: fullscreen (also F11), mono sound, and sound while the game is in the background. Old settings files keep the old behaviour.
- Single player pauses when the window loses focus or the active gamepad disconnects.
- F12 saves a screenshot to the screenshots folder in the save folder.
- Quitting a shift from the pause card shows the shift report before the title.
- End shift needs two presses, like Main menu. An armed button resets after 3 seconds.
- Join screen: `host:port` for servers on another port, Ctrl+V to paste, and a list of the last five servers.
- Online: a badge under the meters with the pilot count and round-trip time. F3 adds round trip, packet loss, traffic, unconfirmed inputs, and simulate and draw time per frame.
- Other tugs, rocks, cargo and the opposition move smoothly between online snapshots.
- Backing plates behind the HUD's banked block, meters and key hint, so the field doesn't show through.
- Messages queue up instead of overwriting each other, so an achievement no longer hides the host's address.
- The title screen's difficulty text shows each mode's installment and damage scale and when debt collectors start. The keyboard and gamepad guides list every control.
- The pilot name field shows how the name will be saved and how many characters are left.
- The leaderboard says it only holds shifts from this PC.
- During training, Marge's panel no longer repeats the training card.
- The last three careers are kept as career.1.json to career.3.json.
- game.log records sound, recording, save and shader problems that the release build used to hide. A crash writes crash.log, and the next launch points to it.
- A splash screen shows during the start-up date check, which can take 1.5 seconds offline.
- Servers write a health line to server.log once a minute (pilots, overruns, slowest step).
- For testing: CJ_NET_JITTER_MS and CJ_NET_LOSS_PCT add jitter and packet loss to a client.

### Changed
- The mixer plays at most 64 voices, dropping the oldest effect (never the music), and never allocates on the audio thread. The particle cap drops the oldest particles without shifting the list.
- Glow render targets are rebuilt once a window resize finishes, not on every frame of the drag.
- The title screen only re-reads career.json when it has changed.
- CI builds the release exe as a download and checks dependencies against RustSec advisories. The toolchain is pinned to 1.99.1 (rust-toolchain.toml).
- New tests: randomised client message decoding, mixer limits and mono, save backups, the log, recent servers, network impairment, and an ignored simulation timing baseline (docs/performance.md).

## [1.15.0] - 2026-11-13

### Added
- Salvage claims. The first tug to cut into a wreck stakes a claim, tagged YOUR CLAIM. For 30 seconds, tugs without PvP can't hook what comes out, and the bay pays 10% more for it.
- Cargo insurance, bought for the next shift on the standing tab (bank row). It pays back 60% of cargo lost that shift, up to 5,000 cr, and the report shows the claim. Old saves load uninsured.
- Smuggling. One sealed pod in four is contraband and tagged in the field. It pays double, but a patrol that sees it on your line gives you a wanted star.
- Rival tempers. Greedy crews chase the richest cargo however far away it is, and skittish ones bolt when hit. Both are tagged over their ships.
- New field event: a meteor shower of small, fast rocks from one edge of the field, aimed at a tug.
- Rock anchors. Pressing hook with nothing in range anchors your line to a nearby big rock, so you can swing around it or brake against it. Press it again to let go.
- Slingshot bonus: cargo thrown into the bay at speed pays 15% more.
- Fragile cargo: sealed pods and flight recorders lose 15% of their value in a hard knock.
- The fixer patches a badly holed hull once a minute.
- The cable flickers red when it's yanked hard.
- Rocks crack as they're mined, and ore rocks show their ore nodes (worn down past half).
- The bay's tractor beam shimmers out toward incoming cargo.
- Heat haze behind burning engines (with effects on, off with reduced motion).
- Laser fire, bullet hits and missiles light up nearby rocks.
- The HUD's banked total counts up.
- Sound: the engine pans as you turn, rivals whoosh past, the hull groans when badly damaged, a beep warns before the guns overheat, and you can hear nearby rivals and lawmen on their radios.
- The music adds a bell, then a shimmer, as your load gets more valuable.
- Marge comments on slingshots, damaged cargo, claims, contraband and meteor showers (text only for now, no recordings yet).

### Notes
- Some audit ideas were already in the game and were left alone: shifting prices (belt conditions), escort contracts (the convoy), cable upgrades (winch and grapple) and a shared co-op cable (the latch). A fuel budget was skipped because engine heat already covers it.

## [1.14.0] - 2026-11-13

### Added
- Positional sound. Field effects pan left and right relative to your tug, and distant ones are quieter and muffled. This works online too.
- Volume settings for master, effects, music and voice (100, 75, 50, 25 or 0%) in a new Sound column. Old settings files load at 100%.
- EMP pulses get their own electric zap, plus a falling engine whine when they knock out a ship. Both are in the sound test.
- Music ducks while Marge or the cockpit is talking.
- More music. Field music cycles through four chord progressions of eight bars each, and each belt has its own key. The menus get a calmer theme. Short stings mark banking a load, a jackpot, a new wanted star and a boss crew arriving.
- Space echo: effects ring slightly in open space and more near the carrier and refinery hulls.
- Field sounds: rocks grinding on each other and on cargo, the carrier's bay drone as you come in, and a creaking taut cable.
- Painted rocks and ships are lit from a fixed upper-left light that stays put as they turn. Explosions light only the side of a painted rock facing them.
- Big explosions and EMP pulses send out a shockwave that bends the picture (with effects on, off with reduced motion).
- Quiet Drift gets ice and blue crystal rocks, and the Frontier exotic crystal ones. Ore rocks keep their own look.
- Painted ships show damage below 40% hull: darker paint, smoke and sparks. This applies to your tug and to rivals.
- The keycard is painted to match the rest of the field.
- A deeper, fainter star layer, and dust drifting just behind the field.
- The engine glow lights the back of the painted tug.

### Changed
- Painted art is mipmapped, so it's sharper when small and shimmers less as it turns.

## [1.13.0] - 2026-11-13

### Changed
- The rest of the field now uses painted art: security drones, mining drones and sentries (red when rogue), and salvage by type and grade (scrap, each ore, plating, engines, reactor cores, ingots, the rogue core, the flight recorder, sealed pods and mining charges). Rare pieces sit on a glow of their colour. Wrecks are drawn as debris, the derelict freighter as a long hull, the carrier as the collection hub and the refinery as the mining vault, with its jaws, sockets and countdown unchanged. Warning rings, beacons, cracks and emergency lights still show. Vector style and collision are unchanged.
- Painted salvage is 15% bigger, and painted rivals, law, debt collectors and haulers 30% bigger, so they stand out. Collision is unchanged.
- In painted style, a pilot's paint job (or online slot colour) tints its shield instead of the ship, so the art keeps its colours. Vector tugs still wear the paint on the hull.
- Missiles are drawn 10% bigger and torpedoes 15% bigger, in both styles.

### Added
- Missile and torpedo explosions set off an EMP (90 px for a missile, 160 px for a torpedo). Hostiles caught in it lose their engines for 10 seconds: they drift and can still turn and shoot, but can't chase, flee or tow. Blue arcs crackle over disabled ships. This covers rivals, security drones, angry mining crews, and the law and debt collectors once you've provoked them. Your tug, co-op partners and the hauler are never affected.
- Explosions briefly light nearby rocks in the blast's colour. Vector rocks get a bright rim and glow on the side facing the blast, and painted rocks get a wash of light.
- Field ambience: the energy wall at the edge of the belt hums louder as you approach it, and radiation crackles like a Geiger counter, from a few clicks at a cloud's edge to a rattle at its centre. Both only play while flying and respect mute.

## [1.12.0] - 2026-11-12

### Changed
- Ships use painted sprites. Each pilot slot flies its own model (miner, fighter, interceptor, heavy, scout, then repeat), tinted with its paint job or online colour. Rivals have a model per hull (skiff, cutter, gunship, brute), and the sheriff, Galactic SWAT, debt collector and convoy hauler have their own art. Thrust flames, shields, hit flashes, hunter heat, light bars and tags still show. Bought parts show on the tug in the shop but not in the field. Collision is unchanged.

### Added
- An "Art style" setting switches rocks and ships between painted sprites and the original vector outlines (painted by default, saved as `sprites` in settings.json). Vector style shows bought parts on the tug again. The co-op note moves under the controls to make room.

## [1.11.0] - 2026-11-12

### Changed
- Rocks use painted sprites from the asteroid art pack: grey cratered rock and molten or gold-veined ore rock in three sizes, with several variants each. A rock keeps its art as it tumbles, and hits still flash it white. Chunks from a breaking rock use the small rock art. Collision is unchanged.

## [1.10.0] - 2026-11-12

### Added
- Recorded neural voices for Marge and the cockpit warnings, covering training and sound-test lines too. Live numbers and names are built from recorded phrases and number words. The game no longer needs a speech engine or an installed Windows voice. Text, mute, interruptions and the limit on routine lines all still work.

## [1.9.0] - 2026-11-12

### Added
- F3 toggles a frame-time overlay on any screen: frames per second, average frame time, the slowest frame in the last two seconds (red below 60 fps), the particle count and whether effects are on. It starts hidden and isn't saved.

### Changed
- The player README lists F3, mentions the Glow and effects and HUD size settings, and suggests F3 and turning effects off if the game runs slowly.

## [1.8.0] - 2026-11-12

### Added
- Thruster trails. A burning engine leaves a short glowing ribbon that thins and fades over a third of a second. Every pilot's tug has one, online too.
- Breaking rocks throw tumbling chunks along with the dust, lit like the rocks and edged in gold if the rock held ore.

### Changed
- The cable whitens gradually as it tightens and builds a soft halo, instead of switching colour at a single tension.

## [1.7.0] - 2026-11-12

### Added
- A "HUD size" setting: 100%, 125% or 150% (saved as `hudScale` in settings.json, 100% by default). It scales the in-flight HUD: readouts, bars, radar, Marge's messages, the training card, chat and the pilot list. Text is drawn at its final size, so it stays sharp. The HUD never lays out smaller than 1280x720, so on small windows the size is capped to stop panels overlapping. The arrows at the screen edge keep their size, since they point at things in the field.

### Changed
- Settings rows sit a little closer together to fit the new one.

## [1.6.0] - 2026-11-12

### Changed
- Rocks and wrecks are lit from one direction, shading from the upper left to a dark far side, with a bright rim on the lit edges. They read as solid objects instead of outlines. Wrecks also show an inner line of hull plating. This doesn't need effects and is always on.
- With Glow and effects on, radiation clouds are churning green gas with a pulsing rim, and the belt edge is a rippling energy wall that brightens as you approach. Fainter dashed lines still mark the exact hazard and field edges.

## [1.5.0] - 2026-11-12

### Added
- Glow and effects. The field is drawn off-screen and finished with a bloom pass, so bright things (thrusters, shots, the cable, cargo, sparks) glow while rocks and the background stay dark. A light colour grade and faint film grain finish the frame, and reduced motion holds the grain still. The HUD and menus are drawn on top and stay sharp.
- A procedural nebula behind the stars, in the old nebula's colours and parallax. It replaces the old one while effects are on.
- A "Glow and effects" setting turns all of this off (on by default, saved as `effects` in settings.json). If the graphics driver can't compile the effects, they stay off and the field draws as before.

## [1.4.6] - 2026-11-11

Fixes from a third repository audit.

### Fixed
- An online game could crash when a new shift started. When a player left during a shift, the others moved down a pilot slot. If a player's Welcome message for the next shift got lost (it's resent after 300 ms), their first snapshots used the old pilot number, which could be out of range. Snapshots meant for another pilot are now ignored until the Welcome arrives.
- Hosting used more upload than necessary. Tugs, salvage, rocks and shots sent every value at full size 20 times a second. Timers and levels that sit at zero most of the time now take one byte while they do, and headings and spins are sent at half size. Snapshots are a quarter smaller with pilots spread out and two fifths smaller with everyone at the bay, about 0.55 Mbit/s per pilot instead of 0.75. Hull, shield and credit values are still exact.
- Shop purchases were saved to disk on the server's step loop, so a modified client spamming requests at the rate limit could make the break stutter for everyone. The server now saves changed careers once a second, at the start of each shift, when a lobby player leaves, and when it stops.
- A player whose game crashed or lost its connection and rejoined before the server noticed was told their pilot was already flying and had to wait for the old connection to time out. With the right identity, the new connection now takes over the pilot (or lobby place) straight away, and the old one is told why it was dropped.

### Changed
- `tools/package.ps1` refuses to run with uncommitted changes and prints the commit it packages, so every release zip matches a commit.
- The player guide gives the upload needed for hosting (about 0.6 Mbit/s per pilot, more with everyone in one place), and the multiplayer design note lists the measured numbers next to its estimate.

## [1.4.5] - 2026-11-11

Fixes from a second repository audit.

### Fixed
- The shop's contract offers and the belt conditions were only saved once the player bought or picked something, so going back to the title and pressing Continue rerolled them as often as you liked. Online, a lobby player could do the same by leaving and rejoining during the break. Rolls are now saved as soon as they're made, offline and online.
- A script could join a server with a new player id and leave again, over and over, and the server wrote a key and a career to disk every time. A server now accepts at most its player limit of new players per minute. Past that, a new id is told the server is busy and to try again in a minute, and nothing is written. Known players still get in.
- A hosted server's server.log only restarted when hosting began, so a long session could grow it forever. It now also restarts past 1 MB mid-session, keeping the previous one as server.old.log.
- A pilot whose name only used scripts the fonts can't draw showed as "Pilot N" when joining mid-shift but "New pilot" in the lobby, so two of them looked identical. Each now gets a unique "Pilot N".

### Changed
- The Join screen and the player guide explain that the game sends your pilot identity (`mp_id.txt`, the same for every server) to each server you join, so a host could use it to play as you elsewhere. Only join hosts you trust.

## [1.4.4] - 2026-11-11

A follow-up fix from the independent audit.

### Fixed
- If a server couldn't save the key that ties a career to its owner's secret, it let the player in anyway and saved the career without a key, so anyone using that id could claim it later. The server now turns the player away and writes nothing until the key can be saved. A key that can't be read still turns them away.
- A player turned away for that reason is told the server couldn't save their pilot and to ask the host to check its disk, instead of being told the server knows their pilot from another install.

## [1.4.3] - 2026-11-11

More online fixes from the independent audit.

### Fixed
- Servers loaded saved careers by player id alone, so after a restart, or once a player had left and been forgotten, anyone with that id and any secret could take the career. Each career now belongs to the secret that first claimed it, and other secrets are refused. The server only stores a SHA-256 fingerprint of the secret (`<player>.key` next to the career). Careers saved by 1.4.2 and earlier still work: the first player back claims them.
- Online, a player who reconnected during the break got their tug back but couldn't shop or vote until they'd flown another shift. They now get their career back right away.

### Changed
- The server uses the `sha2` crate (RustCrypto) for those fingerprints.

## [1.4.2] - 2026-11-11

Online fixes from an independent audit, so one player's shift no longer
affects another's.

### Fixed
- Online, the end of a shift gave every pilot with a drone bay everyone's surviving sentries, so drones multiplied past the bay's limit. Each career now only gets its own back.
- Online, a pilot who joined a shift already in progress (including the first player on a fresh server) flew without their sentries, and the end of the shift then saved their bay as empty. Sentries now launch on arrival.
- Online, a debt collector impounding one tug ended the shift for everyone. Now only that tug leaves the field for the rest of the shift (its load floats free and its stars are cleared, as with a wreck). Everyone else keeps flying, and the shift only ends early if every tug is impounded. The impounded player's HUD shows "IMPOUNDED · BACK NEXT SHIFT". Single player is unchanged.
- Online, a player who arrived during the break couldn't open the shop until they'd flown a shift. They now get their career, with that break's offers, when they arrive.
- Online, arresting one pilot called off the law chasing other wanted pilots. Now only the arrested pilot's pursuers stand down.

## [1.4.1] - 2026-11-11

Fixes from the repository audit.

### Fixed
- Online, a player who dropped and rejoined in the same shift couldn't steer, hook, drop, cut or toggle PvP for about as long as they'd already flown. The server kept the old connection's message numbers and threw away the new ones as repeats.
- Online, paying off the loan during the break traded the tug up silently (the shop kept showing its previous message). It now says "Loan paid off. Traded up: …" with the new tug and loan.
- Pilot names only keep letters the fonts can draw (ASCII and Latin), like chat already did. Other scripts showed as blanks for everyone, and Cyrillic or Greek lookalikes let one player impersonate another. Saved names and local leaderboard entries in other scripts lose those letters on load.
- A server that falls more than 0.25 s behind (sleeping PC, paused VM) now skips the lost time instead of running every missed step at once, which made the shift clock jump and flew tugs on stale controls.

### Changed
- Single player and the server handle shop requests through the same function (`cj_net::ShopReq::apply`), so the two shops can't drift apart. An out-of-range offer number is refused there instead of being used as an index.

## [1.4.0] - 2026-11-11

Opt-in PvP on servers that allow it, and a play-test cut-off.

### Added
- PvP, off unless the host allows it (Launch screen "PvP: On", or `--pvp` on the headless server). Everyone starts each shift Protected. Pressing B (D-pad up) in the bay starts 10 seconds of arming, then the tug is PvP, shown to everyone by a red ring and a PVP tag and to its pilot on the HUD. A PvP pilot can hurt any tug, and anyone they hurt can fire back for 30 seconds after each attack. Ramming only does damage between two PvP tugs. The PvP key can be rebound in Settings. If an older settings file already had another action on B, that action keeps it and PvP moves to a free key.
- Combat lock. Attacking, or being hit while PvP, keeps a pilot in PvP for 30 seconds. PvP can't be turned off in the bay during it, and a player who leaves mid-fight leaves their tug in the field until it ends.
- On belts with law, the first hit in a fight earns the attacker a wanted star, and a PvP wreck earns two more. Firing back earns none. A PvP wreck's cargo is held for 20 seconds for its owner and PvP pilots, PvP wrecks pay no bounty, and the Tab list counts them as kills.
- A copyright notice ("Copyright © 2026, 2027 Neil Bruggeman. All rights reserved.") on the opening screen, in the player README and on the exe's Details tab.
- Play-test builds stop working on 1 June 2027. After that the game only shows a screen saying the build has expired and asking the player to get a newer one. Saves aren't touched. `--smoke` fails after that date. The date comes from network time (`time.google.com`), so winding back the PC clock doesn't help. Offline, the PC clock is used.

## [1.3.0] - 2026-11-11

Online co-op for up to 20 pilots in one shift.

### Added
- The game opens with a choice: Single player, Host server or Join remote host. Hosting runs the server inside your game for 2 to 20 pilots (the field grows with the player count) at a career difficulty you choose. Joining looks the server up first and shows how many pilots are on it, or that it's full. The port is always 47820 (UDP). A headless `cj-server` runs a server without the game.
- Your own tug and shots respond instantly, however far away the server is. The server has the final say, and corrections stay small even on a laggy connection that drops packets.
- Every pilot's tug and tow line is drawn, each in their own colour. Nearby players' sounds fade with distance, and the radar shows what the server reports beyond your view.
- Tab shows every pilot with this shift's credits, kills and deaths. Enter opens public chat, shown bottom-left over the field.
- Shifts run back to back. In the break you see your own report, shop with your career on that server (the server checks every purchase), and vote on the next belt from those everyone's tug can fly. The next shift starts when everyone is ready, or after 90 seconds.
- Each server keeps a separate career for each player, apart from their single-player career, at the difficulty the host chose. A host's careers are kept for the next time they host.
- Players who join in a shift's last minute wait in the lobby for the next one. A player who drops keeps their tug and cargo safely out of the field until they return.
- Help for when nobody can connect: allowing the game through Windows Firewall, and which address to give out. The player guide has a new PLAYING ONLINE section covering LAN play, port forwarding and Tailscale.

### Changed
- Other players' shots stop at your tug but never hurt it. Player damage arrives with opt-in PvP in a later version.
- Radiation clouds burn every tug inside them. Rival and law bolts hit any tug in their path, and refinery drones take turns between the tugs in range.
- Long messages wrap instead of running off the edge of a narrow window.

### Notes
- Online play is IPv4 only, and the connection isn't encrypted.

## [1.2.0] - 2026-11-09

### Added
- Rival crews spend stolen cargo on upgrades: tougher hulls at 3,000 cr, faster engines at 6,000, then quicker guns at 10,000 (capped at level 2 on Relaxed). Taking down a boss crew halves their war chest. The report says when they upgrade, and Map & standing shows their level.

### Changed
- Rival standing drifts 10% back toward zero each shift, unless that shift pushed it further out.

## [1.1.0] - 2026-11-09

From this version on, the native game leads. The browser game is frozen at 1.0.7.

### Added
- A destroyed reactor leaves a radiation cloud for 25 seconds. Inside it, shields are down and the hull slowly burns. Clouds never settle over the bay.
- A wrecked tug lets out a shockwave that pushes and damages anything nearby.
- Wanted stars fade one every 20 seconds while nobody is chasing you.
- Level 5 cannons fire instant lasers.
- Rivals hunting you dodge incoming shots. Skiffs dodge the most, brutes never.
- Missiles and torpedoes can lose their lock, less often at higher levels.
- Crews adapt to the way you usually kill them, and Marge warns you when they do.
- New refinery charges show up if none are left and the vault is still shut.
- `tools/stage.ps1` builds a separate stage copy with its own saves.

### Changed
- Your shots pass through the cargo you're towing, reactors included.

### Fixed
- Enemy bolts hit the whole drawn hull of the escorted hauler, so they no longer pass through its tail and corners.

## [1.0.4] - 2026-11-09

Catches up with the browser game's 1.0.6 and 1.0.7 review fixes. Golden
traces re-recorded from browser 1.0.7.

### Fixed
- Rivals, drones and collectors crashing into rocks no longer earn bounties, grudge or tally. Neither do reactors you didn't set off (knocked while not towed or thrown, or shot by enemy fire). Wrecked Kessler jaws always count on the kill tally, as in the browser.
- A blast or torpedo no longer hits the fragments it just created, so boulders split instead of vanishing.
- A crab on the hull lets go of the cargo it crawled off, so banking that cargo no longer kills it.
- The refinery drifts around the carrier rather than sometimes toward it.
- Cargo taken in an arrest counts toward the report's Stolen / lost figure.
- Contract offers survive a restart instead of being rerolled.
- A chain step and a normal offer with the same target no longer both show as accepted.
- Paying off the loan in the shop shows a blank report instead of the last shift's figures and replay.
- The net and hook prompts show rebound keys.
- The cuff meter follows the sheriff closest to making the arrest.
- Night-shift arrows no longer reveal contacts the radar can't see.

## [1.0.3] - 2026-11-09

### Fixed
- A career file that can't be read or parsed is copied to career.bad.json before a new career replaces it. Saves are flushed to disk before the final rename, so a power cut can't leave an empty file.
- Save failures show a warning on screen. The release build has no console, so they used to go unnoticed.
- Skipping training is remembered, just like finishing it. After "Replay training", skipping no longer brings training back every shift.
- Local co-op: the camera frames both tugs and zooms out as they spread apart, as in the browser.
- Opening Settings from the pause card no longer moves the shift's cargo and clock in the background.
- An enemy shot right at the edge of hearing no longer produces invalid sound samples.
- A key that is another action's fixed key can't be rebound, since it would trigger both actions (same as the browser).
- Sound recovers when the output device is lost (a headset unplugged, say) and when a device appears after starting silent.
- SWAT, stun and rogue bolts use the browser's sizes and colours.

### Added
- Gamepad rumble on hull hits, reactor blasts and being netted, as in the browser.

### Changed
- `--smoke` exits non-zero unless every screen was captured and the career reached disk at shift 2. `tools/package.ps1` stops a smoke run that hangs.
- The minimum Rust version is corrected to 1.89, which the locked `uuid` dependency already needed.
- `tools/package.ps1` builds with `--locked`, so releases ship exactly the dependency versions in Cargo.lock.

### Removed
- Short spoken versions of radio lines, which were never used. Marge reads the full line, as in the browser.
- `spikes/m0-spike`, the Milestone 0 technology test. It's still in the git history.

## [1.0.2] - 2026-11-08

### Changed
- Fixed the clippy warnings so the lint can be enforced: a redundant match guard in physics separation, a test helper switched to `slice::contains`, and a collapsed `if` in the exhaust particles. Two deliberate cases are marked `#[allow]` instead: `Rng::next` keeps its name to mirror the browser's `Math.random()`, and the spark constructor keeps its eight particle arguments. No change to gameplay, the random sequence, saves, controls or the package layout.
- `spikes/m0-spike` is no longer a default workspace member, so plain `cargo build` and `cargo test` skip it (`cargo run -p m0-spike` still works).

### Repository
- Added `native/scripts/check.sh` (sanitization, version sync, rustfmt, clippy, tests), a pre-commit hook, and CI for the simulation (Ubuntu) and the game (Windows). None of this ships in the exe.

## [1.0.1] - 2026-11-07

### Fixed
- Everything drew upside down. Menus, the HUD and the field were flipped top to bottom, so text was unreadable, clicks hit the mirror image of a button, and turning looked reversed. macroquad 0.4's `Camera2D::from_display_rect` points Y up, so all three cameras now use `display_camera()`, which points Y down like the browser canvas.
- Pausing did nothing: P, Esc and Start opened the pause card and closed it again in the same frame.
- `--smoke` screenshots came out upside down once the window drew the right way up (they had been flipped to undo the mirrored cameras).

No change to saves, controls or the package layout.

## [1.0.0] - 2026-11-07

First release, with every system of the browser game ported, packaged as
`ClaimJumpers-1.0.0-windows-x64.zip` by `tools/package.ps1`. The README has
the full feature list.

The 1.0.0 zip handed out that day still has the bugs fixed in 1.0.1. Those
fixes landed before the commit that recorded 1.0.0 (883d7bf), so a build of
that commit already includes them but still says 1.0.0. 1.0.1 is the first
version number guaranteed to have them.

## Development milestones (before 1.0.0)

Checks recorded for each milestone while the port was built, moved here from
the README in 1.0.2. Every run was a release build on Windows 11 Pro
(build 26300), started from an unrelated folder.

- M0: all checks passed (see the Milestone 0 report in the commit history).
- M1: `--smoke` exits cleanly with the scripted tow in place.
- M2: `--smoke` exits cleanly with the cannon firing, a rival shooting back, the sound stream open and the screenshot as expected.
- M3: the `--smoke` tour exits cleanly. The shift settles (1,840 earned, 800 installment, 1,040 in the wallet), the shop buys and mounts a part, the title saves and reloads the career, and all six screenshots look right.
- M4: the `--smoke` tour exits cleanly with the law in pursuit, a collector, the refinery's drones, a mining crew, missiles and sentries all active, and all six screenshots look right.
- M5 part 1: the `--smoke` tour exits cleanly with an ion storm, a boss crew, an escorted hauler, a sealed hold and its keycard, and a contract all active. The report settles the missed contract, the shop accepts a contract, hires a co-pilot and buys a truce, and all eight screenshots look right.
- M5 part 2: the `--smoke` tour exits cleanly with training running (it reached step 3 of 5), Marge's lines on the radio panel, the speech synthesizer found (3 English voices, with speech off for the check), and the report replaying a highlight.
- M5 part 3: the `--smoke` tour exits cleanly through the new title menu, settings, records, the leaderboard (with the tour's own shift on it) and the sound test, and all twelve screenshots look right.
