# Sprites

`rocks/`: 34 rock sprites from the owner-supplied asset pack
(`all_game_assets.zip`, folder `assets/asteroids`): transparent PNGs
extracted from the owner's asteroid concept sheet with an image
generation tool. Plain rock uses the cratered and grey pieces; ore-bearing
rock uses the molten and gold-plated ones. Quiet Drift's plain rock also
uses the ice and blue crystal pieces, and the Frontier's the exotic crystal
ones (`BELT_ROCK_ART`). They are embedded in the exe by
`crates/cj-game/src/look.rs` (`ROCK_ART`), which says which file draws
which size and kind. Two ore nodes from the pack's `assets/ores`
(`mixed_ore_node.png`, and `depleted_node.png` once the rock is half
mined) are drawn on ore-bearing rocks (`PropArt`).

`ships/`: 12 ship sprites from the same pack (`assets/ships`, plus
`assets/cargo/cargo_hauler.png`): the five player models, four rival
hulls, the sheriff, the debt collector and the hauler. `ShipArt` in
`look.rs` says which draws what. Player and rival art points its nose up
the image; the sheriff, collector and hauler point right.

`drones/`, `items/`, `wrecks/` and `stations/`: 33 more sprites from the
same pack for drones and sentries, salvage by type and grade, wrecks and
the derelict freighter, the carrier's hub and the refinery's vault.
`PropArt` in `look.rs` says which draws what. The rest of the pack
(the other ore nodes, other containers, the field sample
rocks and the leftover debris) isn't used yet. The keycard has no
art in the pack; it is painted in code (`keycard_image` in `look.rs`).

Origin and rights of the art: to be confirmed by the owner before a
public release; add a credit to `packaging/README.txt` if one is needed.
