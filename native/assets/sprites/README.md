# Sprites

`rocks/`: 27 rock sprites from the owner-supplied asset pack
(`all_game_assets.zip`, folder `assets/asteroids`): transparent PNGs
extracted from the owner's asteroid concept sheet with an image
generation tool. Plain rock uses the cratered and grey pieces; ore-bearing
rock uses the molten and gold-plated ones. Ice and crystal rocks are left
out until the game has such rocks. They are embedded in the exe by
`crates/cj-game/src/look.rs` (`ROCK_ART`), which says which file draws
which size and kind.

Origin and rights of the art: to be confirmed by the owner before a
public release; add a credit to `packaging/README.txt` if one is needed.
