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
that folder is how servers recognise you when you come back. When you host,
the players' careers on your server are kept in its servers\hosted folder.
To back up or move your progress, copy that folder. To play from a
different folder, set the CLAIM_JUMPERS_DATA environment variable to it.


PLAYING ONLINE
--------------
Up to 20 pilots can share one shift. Everyone needs the same version of the
game.

Hosting: choose Host server. Before you do, you can pick how many pilots
the server is for (the field grows with it) and the difficulty of every
career on it. Your game runs the server, so the session ends when you
leave. The first time, Windows Firewall may ask whether to allow Claim
Jumpers: allow it on private networks, or nobody else can get in.

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
career, so you start fresh on every server. Other players' shots stop at
your tug but never hurt it.

The connection is not encrypted: don't share anything private in the chat.


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
