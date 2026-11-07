# Claim Jumpers

A momentum salvage shooter for the browser. You fly a deep-in-debt salvage tug through an asteroid belt, tow cargo on a cable, swing it as a weapon, and get it back to the carrier before rival crews, debt collectors or the law take it off you.

The game ships as one file, `claim-jumpers.html`: Canvas 2D graphics, WebAudio sound and music, and Web Speech radio chatter from Marge, the dispatcher. It needs no runtime dependencies. It's built from the sources in `src/` (see [Development](#development)).

## Play

Open `claim-jumpers.html` in a modern desktop or mobile browser. On touch screens an on-screen stick and buttons appear.

### Run it in Docker

With Docker Desktop running, from the project folder:

```bash
docker compose up -d --build
```

Then play at http://localhost:8080. This starts two small containers: the game (non-root nginx) and a shared leaderboard service, whose scores are kept in a Docker volume. Both restart with Docker Desktop. Run the same command after changing the game to rebuild. To stop them:

```bash
docker compose down
```

By default the game only listens on this PC. To change that, create a `.env` file next to `compose.yaml`:
- `CJ_BIND=0.0.0.0` lets other devices on your network play and share the leaderboard.
- `CJ_PORT=9000` uses a different port.

## Controls

| Action | Keyboard (default) | Gamepad | Touch |
|---|---|---|---|
| Thrust / turn / brake | W, A/D, S | RT, left stick, LT | Stick (up, sideways, down), Thrust, Brake |
| Fire | Space | RB | Fire |
| Hook / drop last / cut all | E, Q, X | Y, B, LB | Hook, Drop, Cut |
| Strafe (side thrusters) | Z / C | Right stick | |
| Grind (salvage grinder) | G | X | Grind |
| Pause / mute / voice / music | P, M, V, N | Start, Back | II |
| Skip training | T | | Skip button |

Keyboard controls can be rebound under **Settings**, which also has music, reduced shake and flashing, and a colorblind-safe palette.

## What's in it

- **Cargo physics:** a rope-constraint tow cable. Heavy cargo swings, and it hurts what it hits.
- **Career:**
  - Each shift pays an installment on the tug's loan. The schedule restarts with each new loan.
  - The wallet carries over and can go into overdraft, down to a third of the remaining loan.
  - Pay a tug off and you're traded up to the next of five tug classes.
- **Shop:**
  - a sector map of five belts that trade risk for reward
  - contracts that pay a bonus
  - a co-pilot with a perk
  - hardpoints, levelled systems and sentry drones
- **Opposition:**
  - rival crews in four hull types, with an escalating grudge; they scatter when a wingmate is smashed and give each other covering fire
  - debt collectors who net you when your payment is at risk
  - mining drones that only fight back when you hit them
  - a five-star wanted system that ends with the Galactic SWAT team
  - your own sentries, gone rogue after your tug is wrecked
- **Field events:** ion storms, derelict freighters and named rival bosses with bounties.
- **Standing:** reputation with the bank, the law and the rival crews, which changes prices, collectors, patrols and how hard rivals come at you. Truces, and three-step contract chains that reward unique parts.
- **Shift variety:** belt conditions (ore boom, patrol sweep, night shift and more), escort missions, sealed holds with keycards, and rogue cores to sell or reprogram.
- **Presentation:** Marge's take on your last shift in the shop, hit-stop and slow motion on big hits, a highlight replay on the report, and paint jobs unlocked by achievements.
- **Modes:** a seeded **daily challenge** that's the same for everyone each day (with its own leaderboard), and **local co-op**, where a second player's partner tug latches onto the cable (turn it on in Settings).
- **Guided first shift,** adaptive music, lifetime records and 13 achievements, save export and import, and a leaderboard.

## Development

```
src/page.html        markup, with @@CSS@@ / @@JS@@ placeholders
src/styles.css       stylesheet
src/js/*.js          the game, as ES modules with explicit imports/exports
src/js/main.js       entry point: imports every module, then calls boot()
build.mjs            bundles src/js with esbuild and assembles claim-jumpers.html
server/              leaderboard service (Node, no dependencies)
tests/               Playwright browser tests
docker/nginx.conf    web server config
```

Edit files in `src/`, then rebuild `claim-jumpers.html`. With Node installed:

```bash
npm ci
```

```bash
node build.mjs
```

Modules must not do work at load time that depends on another module (for example, calling an imported function in a top-level initializer). Modules import each other in cycles, so another module may not have been evaluated yet. Put that kind of setup in a function that runs on first use, or in `boot()`.

Or without Node, using Docker:

```bash
docker compose --profile build run --rm build
```

Run the tests against the running stack (the first run downloads the Playwright image):

```bash
docker compose --profile test run --rm tests
```

GitHub Actions runs the same tests on every push. It also checks that the committed `claim-jumpers.html` matches what `src/` builds to.

Add `?debug` to the URL to expose `window.__cj`, a handle for driving the simulation from the console. The tests use it.

`.claude/launch.json` is a local preview-server config for Claude Code and is not part of the game.
