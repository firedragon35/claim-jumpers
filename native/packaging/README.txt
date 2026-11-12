CLAIM JUMPERS {VERSION} for Windows
===================================

Tow salvage, fight rival crews, and pay off the loan on your tug before the
bank repossesses it. Five-minute shifts, one career.

Copyright (c) 2026, 2027 Neil Bruggeman. All rights reserved.

This is a play-test build. It stops working on 1 June 2027; ask for a newer
build to keep playing. Your career and settings are kept for it. To check
the date, the game asks Google's time server (time.google.com) for the time
when it starts; nothing about you or your game is sent. Offline, it uses
your PC's clock.


REQUIREMENTS
------------
- Windows 11 or Windows 10, 64-bit. (Tested on Windows 11. Windows 10 should
  work but has not been tested yet.)
- A graphics driver with OpenGL 3.1 (or at least 2.1); any PC from the
  last ten years has one.
- Nothing else to install.


HOW TO PLAY
-----------
1. Unzip this folder anywhere you like (Desktop, Documents, a USB stick).
2. Double-click claim-jumpers.exe.

The first time, Windows may show "Windows protected your PC" because the
game isn't signed by a publisher. Click "More info", then "Run anyway".

The game opens asking how to play: Single player, Host server or Join remote
host (see PLAYING ONLINE). In single player, your first shift starts with
Marge's training. Press T to skip it.


CONTROLS
--------
                        Keyboard            Gamepad
Thrust                  W or Up             RT or A
Brake / reverse         S or Down           LT or D-pad down
Turn                    A D or Left Right   Left stick or D-pad
Fire                    Space or J          RB
Hook nearest piece      E or K              Y
Drop last piece         Q                   B
Cut all                 X                   LB
Strafe (side thrusters) Z / C               Right stick
Grind (grinder)         G                   X
Afterburner             Shift               LT + RT together
Pause                   P or Esc            Start
Mute                    M                   Back
PvP on/off (online)     B                   D-pad up
Marge's voice on/off    V
Music on/off            N
Skip training           T

Menus: arrow keys, D-pad or left stick to move; Enter, Space or A to press;
LB / RB to switch tabs. The mouse works too.

The flight keys can be changed under Settings on the title screen. Settings
also has the music, reduced shake and flashing, a colorblind-safe palette,
and local co-op: a second player flies a partner tug from the numpad
(8 thrust, 4 / 6 turn, 5 brake, 0 fire, Enter latch) or a second gamepad.


YOUR SAVES
----------
Your career, records, settings and leaderboard are saved automatically in:

    %APPDATA%\ClaimJumpers

(paste that into the File Explorer address bar to open it). mp_id.txt in
that folder is how servers recognise you when you come back, and only it
can load your career on a server, so keep it to yourself. Your game sends
it to every server you join, though, so join only hosts you trust (see
PLAYING ONLINE). When you host,
the players' careers on your server are kept in its servers\hosted folder.
To back up or move your progress, copy that folder. To play from a
different folder, set the CLAIM_JUMPERS_DATA environment variable to it.


PLAYING ONLINE
--------------
Up to 20 pilots can share one shift. Everyone needs the same version of the
game.

Hosting: choose Host server. Before you do, you can pick how many pilots
the server is for (the field grows with it), the difficulty of every
career on it, and whether PvP is allowed (off unless you switch it on; see
PVP below). Your game runs the server, so the session ends when you
leave. The first time, Windows Firewall may ask whether to allow Claim
Jumpers: allow it on private networks, or nobody else can get in.

Hosting over the internet uses the host's upload speed: about 0.6 Mbit/s
for each pilot, and up to 1.5 Mbit/s each while everyone is in one place,
such as the bay. That is about 3 Mbit/s for 4 pilots, 8 for 8 and 30 for
20. If the shift stutters for everyone at once, host fewer pilots. On a
home network this doesn't matter.

Joining: choose Join remote host, type the host's IP address and press
Find server. The game shows how many pilots are connected, or that the
server is full; press Join. The port is always 47820 (UDP).

  - On the same home network: use the address the host's game shows when
    hosting (it looks like 192.168.x.x).
  - Over the internet: the host forwards UDP port 47820 on their router to
    their PC and gives out their public address. Some internet providers
    don't allow this (carrier-grade NAT). Then the easiest way is Tailscale
    (tailscale.com): install it on every PC, put them on the same tailnet,
    and join with the host's Tailscale address (it looks like 100.x.y.z).

Online keys: Tab shows every pilot with this shift's credits, kills and
deaths. Enter opens the chat: type, then Enter sends (Esc cancels). The
pause card doesn't stop the shared shift.

Between shifts there is a break: see your report, shop, and vote for the
next belt (only belts every pilot's tug can fly count). The next shift
starts when everyone is ready, or after 90 seconds. A player joining in a
shift's last minute waits in the lobby for the next one.

Each server keeps its own career for you, separate from your single-player
career, so you start fresh on every server. Unless you go PvP, other
players' shots and rams stop at your tug but never hurt it.

PvP (player versus player) is only on servers whose host allows it, and
everyone starts each shift Protected. To fight, fly into the bay and press
B (D-pad up on a gamepad); after 10 seconds of arming you are PvP, shown to
everyone by a red ring and a PVP tag on your tug. A PvP pilot can hurt any
tug, and anyone they hurt may fire back at them for 30 seconds after each
attack. Ramming hurts only when both tugs are PvP.

Attacking, or being hit while PvP, locks you in combat for 30 seconds: you
can't switch PvP off (in the bay again) until the lock ends, and if you
leave the game your tug stays in the field until it does. On belts with
law, the first hit in a fight earns the attacker a wanted star and wrecking
the tug two more; firing back earns none. For 20 seconds only its owner and
PvP pilots can pick up a wrecked tug's cargo, and PvP wrecks pay no bounty.

The connection is not encrypted: don't share anything private in the chat.

Join only hosts you trust. To recognise you, your game sends every server
you join the identity in mp_id.txt, the same one for every server. A host
who runs a changed copy of the game could keep it and then play as you,
with your careers, on any other server you play on.


MARGE'S VOICE
-------------
Marge speaks through Windows' own speech voices. If she is silent, check
that a speech voice is installed (Settings > Time & language > Speech), or
press V. Her lines always show on the radio panel either way.


UNINSTALLING
------------
Delete this folder. To remove your saves too, delete %APPDATA%\ClaimJumpers.


TROUBLESHOOTING
---------------
- No sound: check the Windows volume mixer, and that the game isn't muted
  (M). The game uses your default sound device when it starts.
- Gamepad not responding: connect it before or after starting; the most
  recently used pad is player 1. With gamepad co-op on, the last connected
  pad belongs to player 2.
- Window too small: drag the window edge or maximise it; everything scales.


CREDITS
-------
Fonts: Big Shoulders Stencil Display, Barlow Semi Condensed and JetBrains
Mono, under the SIL Open Font License 1.1 (see the licenses folder).
Open-source components: see THIRD-PARTY-NOTICES.txt.
