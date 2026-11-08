CLAIM JUMPERS {VERSION} for Windows
===================================

Tow salvage, fight rival crews, and pay off the loan on your tug before the
bank repossesses it. Five-minute shifts, one career.


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

Your first shift starts with Marge's training. Press T to skip it.


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

(paste that into the File Explorer address bar to open it). To back up or
move your progress, copy that folder. To play from a different folder, set
the CLAIM_JUMPERS_DATA environment variable to it.


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
