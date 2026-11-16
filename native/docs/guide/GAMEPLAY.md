# Claim Jumpers: Gameplay Guide

You fly a salvage tug that's still on a bank loan. Each **shift** gives you
five minutes on an asteroid belt. Break rocks, cut open wrecks, hook whatever
is worth money onto your tow cable and drag it into the carrier's bay before
rival crews, debt collectors or the law take it from you. After every shift
the bank takes an installment, so earn enough to cover it, spend the rest on
the tug, and go out for the next shift.

![Title screen](img/01-title.png)

## Contents

1. [Starting out](#starting-out)
2. [Controls](#controls)
3. [The shift](#the-shift)
4. [Reading the HUD](#reading-the-hud)
5. [Salvage and what it pays](#salvage-and-what-it-pays)
6. [Trouble in the belt](#trouble-in-the-belt)
7. [The shift report and the loan](#the-shift-report-and-the-loan)
8. [Outfitting between shifts](#outfitting-between-shifts)
9. [Belts and conditions](#belts-and-conditions)
10. [Playing with others](#playing-with-others)
11. [Tips](#tips)

---

## Starting out

1. Unzip the release and run `claim-jumpers.exe`. The game is unsigned, so
   Windows may show "Windows protected your PC". Click **More info**, then
   **Run anyway**.
2. Choose **Single player**.
3. On the title screen, set your pilot name and choose a difficulty for a
   new career:

| Difficulty | What changes |
|---|---|
| **Relaxed** | Installments ×0.6, damage ×0.6, fewer rivals that arrive later, and debt collectors from shift 3. Every encounter still shows up. |
| **Standard** | The intended curve. Pressure builds shift by shift, and debt collectors start at shift 2. |
| **Hard** | Installments ×1.3, damage ×1.3, more rivals arriving sooner, debt collectors from shift 1, and higher fees. |

Your first shift runs Marge's training: five short steps covering flying,
hooking, banking, the cannon and throwing cargo. Press **T** to skip it. You
can replay it from the title screen at any time.

The title screen also has the **Daily challenge**, the same seeded field for
everyone each UTC day with a fixed loadout. It doesn't touch your career.

## Controls

| Action | Keyboard | Gamepad |
|---|---|---|
| Thrust | W or ↑ | RT or A |
| Brake / reverse | S or ↓ | LT or D-pad down |
| Turn | A D or ← → | Left stick or D-pad |
| Fire the cutting cannon | Space or J | RB |
| Hook nearest salvage (or anchor to a big rock) | E or K | Y |
| Drop the last piece | Q | B |
| Cut the whole line | X | LB |
| Strafe (needs side thrusters) | Z / C | Right stick |
| Grind the last piece (needs the grinder) | G | X |
| Afterburner | Shift | LT + RT together |
| Pause | P or Esc | Start |
| Mute, Marge's voice, music | M, V, N | Back (mute) |
| Skip training | T | |
| Fullscreen, screenshot | F11, F12 | |

You can rebind the flight keys under **Settings**. Menus work with the
keyboard, the mouse or a gamepad.

![Settings](img/08-settings.png)

Settings also has the volume controls, reduced shake and flashing, a
colorblind-safe palette, the HUD size, the art style (painted sprites or the
original vector outlines), and local co-op for a second player.

## The shift

A shift lasts **five minutes**. The loop:

1. **Fly out from the carrier bay.** The bay is the dashed ring, and the
   off-screen arrow points to it. Parking near it repairs your hull and
   recharges your shield faster.
2. **Find value.** Shoot rocks to break them into smaller pieces, some of
   which hold ore. Cut into wrecks to spill their cargo. The first tug to
   cut a wreck **stakes a claim** on it: for 30 seconds nobody else can
   take what comes out, and the bay pays 10% more for it.
3. **Hook it.** Fly close and press **E**. Each piece goes onto the end of
   your cable, so you can tow a train of several. Every piece adds weight,
   and heavy loads swing wide and slow your turns. The winch meter shows how
   many tonnes you're towing.
4. **Bank it.** Drag your cargo into the bay ring to sell it. Cargo you
   **throw** into the bay at speed (a slingshot) pays 15% more.
5. **Repeat before the clock runs out.** Anything still on your line at the
   end of the shift isn't banked.

**Cargo is a weapon.** Swing a heavy load and press **X** to cut it loose,
and it keeps flying. A cargo smash into a rival is one of the strongest hits
in the game. **Q** drops just the last piece, which is handy for shedding a
reactor about to blow.

The **afterburner** gives you a burst of speed. Chaining bursts overheats the
engine, so watch the ENGINE meter.

## Reading the HUD

![A busy shift, with the HUD](img/02-shift-hud.png)

*This shot comes from the game's automated self-test, which crowds in many
systems at once: training, a wanted level, the refinery, a hauler and a
staked claim.*

| Where | What it shows |
|---|---|
| Top left | **Banked** this shift, the value **on the line** (what you're towing), your wallet, the active contract and its progress, missiles and sentries. |
| Top centre | The shift number, the belt and its condition, wanted stars and who is chasing you, alerts such as UNDER ATTACK, and the **shift clock**. |
| Top right | The **Shield**, **Hull**, **Heat** (the cannon and thrusters), **Engine** (afterburner heat) and **Winch** (tonnes towed and your limit) meters. |
| Right | The radar minimap. |
| Bottom left | Marge's radio panel. Her lines are also spoken. |
| Bottom right | A key hint for the default keys. |

## Salvage and what it pays

These are base values on Standard. Belts and conditions raise or lower them
(see [Belts and conditions](#belts-and-conditions)). Within each type the last
grade is the rare one.

| Type | Grades and base value (cr) | Notes |
|---|---|---|
| Scrap | Bent 50 · Scrap 80 · Alloy 160 | Light and easy to tow. |
| Ore | Iron 110 · Copper 170 · Cobalt 260 · Platinum 450 | Comes out of rocks you break. |
| Plate | Dented 180 · Hull 280 · Armor 420 | |
| Engine | Burnt-out 550 · Engine block 850 · Fusion drive 1,500 | Heavy (7 t). |
| Reactor core | Cracked 900 · Reactor 1,300 · Pristine 2,200 | **Volatile.** A hard knock primes it to explode. Cracked cores are fragile. |
| Ingot | Silver 500 · Gold 750 · Iridium 1,200 | From the refinery vault. |
| Flight recorder | 950 | Fragile: a hard knock costs 15% of its value. |
| Sealed cargo pod | 200 to 2,000, unknown until banked | Fragile. One in four is **contraband**: it pays double, but a patrol that spots it gives you a wanted star. |
| Keycard | — | Opens a sealed hold. |
| Mining charge | — | Volatile. Seat it in a refinery vault socket. |
| Rogue core | — | Left behind by rogue sentries. Store it, sell it, or have it reprogrammed into a sentry. |

## Trouble in the belt

- **Rival crews** (pink contacts) steal your cargo, sever your line and get
  away with it. Scrapping one pays a bounty, but each kill feeds their
  grudge and hurts your standing with the crews. Some have tempers: greedy
  crews go for the richest cargo, and skittish ones bolt when hit. Crews
  also adapt to how you fight, and they buy upgrades with the cargo they
  steal.
- **Debt collectors** come when the bank thinks your payment is at risk.
  They net your tug and tow it away: wiggle to tear the net. If your load is
  rich enough they confiscate it instead, and otherwise they impound the tug
  for a fee. Destroying one is a crime.
- **The law** reacts to wanted stars. You get them for destroying a debt
  collector (one star), destroying a lawman (two stars) or being spotted
  with contraband. Expect patrols, shakedowns, pursuit, stun bolts, arrests
  and fines, rising to Galactic SWAT at five stars. Stars fade while nobody
  is chasing you, and reaching the bay clears them.
- **Mining crews** cut the field into debris storms.
- **The Kessler refinery** is a rotating station. Tow mining charges into
  its three vault sockets to blow the vault open, which throws out ingots and
  ore. Security drones guard it. Soon after the vault blows, its **jaws**
  wake and pull in and eat anything nearby, your tug included. The jaws can
  be wrecked.
- **Crabs** latch onto cargo and chew it. Spin them off, or bank them to kill
  them.

Partway through each shift a **field event** strikes. It's one of: an
**ion storm** that drops your shields, a **derelict freighter** drifting in,
a named **boss crew** with a bounty, a **hauler** to escort home, or a
**meteor shower** of small, fast rocks.

If your tug is wrecked, the bank takes a **deductible** (500 cr on
Standard) out of what you've banked, and you respawn after a short wait.

## The shift report and the loan

![Shift report](img/03-shift-report.png)

When the clock runs out, the report shows what you banked, kills and
bounties, the contract result, changes in standing, crew wages, any
insurance claim, the **loan installment**, your wallet and how much of the
loan is left.

- Your first tug, the Mk I "Second Mortgage", starts with a **40,000 cr**
  loan.
- The installment starts at 800 cr and grows by 300 cr each shift (scaled
  by difficulty).
- You can go overdrawn, but only up to **a third of what you still owe**.
  Past that, the bank **repossesses** the tug and the career ends.
- Pay the loan off and you trade up to the next tug class on a bigger loan:
  Mk II "Refinance", Mk III "Balloon Payment", Mk IV "Golden Parachute" and
  Mk V "Hostile Takeover". Pay off the Mk V and you've won the career.

Your best single shift earns a rank, from *Space litter* to
*Legend of the belt* at 16,000 cr.

## Outfitting between shifts

Between shifts you're in the machine shop. The panel at the top right shows
your wallet, the loan balance and the next installment. You can also pay
extra off the loan here, which also improves the bank's view of you.

### Tug

![Shop: tug systems](img/04-shop-tug.png)

Your tug has five hardpoints: **Weapon A and B**, a **Cannon mod**, and
**Utility A and B**. Upgrade the levelled systems (shield, engines, hull
armor, cutting cannon, missile rack, sentry firmware) for 1,500, 3,000, 5,000
and then 8,000 cr. Buy sentry drones here once you have a drone bay. The
achievements unlock paint jobs.

### Parts

![Shop: parts](img/05-shop-parts.png)

| Part | Slot | Price | What it does |
|---|---|---|---|
| Cutting cannon | Weapon | stock | Breaks rocks, wrecks and rivals. Builds heat. |
| Twin cutter | Cannon mod | 2,400 | Three-shot volleys. |
| Reinforced winch | Utility | 2,800 | Tow 26 t instead of 16, reel faster, hook from farther away. |
| Side thrusters | Utility | 2,000 | Strafe with Z / C. |
| Magnetic scoop | Utility | 1,800 | Pulls in and hooks nearby scrap and ore. |
| Salvage grinder | Utility | 1,500 | Grinds the last piece on your line into slugs you fire. |
| Armor plating | Utility | 2,200 | +50 hull and +1.5 t of weight. |
| Auto-defense missiles | Weapon | 2,600 | Homing missiles that fire on their own and shoot down nets. Their blasts EMP hostile engines. |
| Drone bay | Utility | 2,000 | Carries up to 3 sentry drones. If the tug is wrecked, its sentries go rogue. |
| Self-sealing hull | Utility | contract-chain reward | Repairs 1.5 hull per second. |
| Long-reach grapple | Utility | contract-chain reward | Hooks from 60% farther away. |

Parts you own stay in the shop when you unmount them.

### Jobs and crew

![Shop: jobs and crew](img/06-shop-jobs-crew.png)

- **Contracts.** Three offers come up each shift (bank a sum, scrap rivals,
  crack the vault, escort a hauler, and so on). Meet one by the end of the
  shift for its bonus. Contract chains award the unique parts.
- **Crew.** Hire one co-pilot at a time for a wage each shift. Each has a
  perk: a rigger who hooks from farther away, a claims adjuster who cuts
  fees, a fixer who calms grudges and patches the hull, or a broker who
  sells cargo for 10% more.

### Map and standing

![Shop: map and standing](img/07-shop-map-standing.png)

Pick next shift's **belt** on the sector map, and keep an eye on your
standing with three factions:

- **Kessler Mutual (the bank).** Better standing changes the installment
  rate. Buy **cargo insurance** here: it covers 60% of the towed cargo you
  lose next shift, up to 5,000 cr.
- **Belt law.** A clean shift raises it.
- **Rival crews.** Scrapping them lowers it. **Buy a truce** to cool their
  grudge.

## Belts and conditions

Every shift happens on a belt, and every belt rolls a **condition** for that
shift, so the sector **shifts** under you from one shift to the next. The
better belts need a bigger tug to get a permit.

| Belt | Needs | Pay | Character |
|---|---|---|---|
| Quiet Drift | Mk I | ×0.75 | Picked over and peaceful, with few rivals. |
| Hollis Belt | Mk I | ×1.0 | Home turf next to the carrier. The usual mix. |
| Kessler Deep | Mk II | ×1.2 | Rich ore, and debt collectors everywhere. |
| Cinder Reach | Mk III | ×1.35 | An old battlefield: fat wrecks, and crews already angry. |
| The Frontier | Mk IV | ×1.6 | No law. Hostile crews and a bounty boss every shift. |

| Condition | Effect this shift |
|---|---|
| Ore boom | Ore and ingots sell for 50% more. |
| Patrol sweep | Patrols twice as often, and twice as many shakedowns. |
| Salvage glut | Three extra wrecks, but prices down 15%. |
| Rival surge | Rivals 40% more often, and boss bounties doubled. |
| Ion front | An ion storm is certain. |
| Night shift | Low visibility and short-range radar, but everything pays 30% more. |

## Playing with others

- **Local co-op.** Turn it on in Settings. Player 2 flies a partner tug from
  the numpad or a second gamepad, and can **latch** onto the tail of your
  cable (Enter, or Y on the gamepad) to haul with you.
- **Online.** Choose **Host server** or **Join remote host**. Up to 20
  pilots can share a shift, each with their own career on that server.
  Between shifts everyone votes on the next belt. **PvP** is opt-in: the
  host has to allow it, and each pilot arms it in the bay with **B**. The
  player README (`README.txt` in the release) covers LAN play, port
  forwarding and Tailscale.

## Tips

- **Bank often.** Cargo on the line is cargo a rival can steal. A few quick
  runs beat one big haul that never gets home.
- **Watch the winch meter.** Overloading makes the tug sluggish, and heavy
  trains swing into rocks.
- **Treat reactors gently,** or throw them at a rival on purpose.
- **Keep a cushion for the installment.** It grows every shift, and Marge
  is right about not spending all of it.
- **Accept contracts that fit your plan,** like a banking target on an ore
  boom belt.
- **Stay clean on law belts.** Wanted stars snowball. Fly to the bay to
  shake them.
