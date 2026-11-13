//! Palette, embedded fonts, glow sprites and the additive blend: the
//! pieces the browser builds its look from (src/js/02-utilities.js and the
//! top of src/js/17-rendering.js).

use cj_sim::field::Belt;
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Colour tokens from the browser's COL table, as 0xRRGGBB.
pub mod col {
    pub const VOID: u32 = 0x05070d;
    pub const TEXT: u32 = 0xdde5f3;
    pub const MUTED: u32 = 0x8a97b3;
    pub const HAZARD: u32 = 0xffb020;
    pub const CABLE: u32 = 0x4fe3ff;
    pub const DANGER: u32 = 0xff5a3c;
    pub const ORE: u32 = 0xffd24a;
    pub const STEEL: u32 = 0x9fb4cf;
    pub const ROCK: u32 = 0x8794ab;
    pub const ROCK_FILL: u32 = 0x111723;
    pub const WRECK: u32 = 0x6fa3a0;
    pub const WRECK_FILL: u32 = 0x0d1a1c;
    pub const SHIELD: u32 = 0x8fd8ff;
    pub const JUMPER: u32 = 0xff4fa3;
    pub const REACTOR: u32 = 0xb8ff3b;
    pub const ENGINE: u32 = 0x6fa8ff;
    pub const CRAB: u32 = 0xff7a3c;
    pub const COLLECTOR: u32 = 0xa98bff;
    /// The cost line on upgrades; also the heat meter.
    pub const TRADEOFF: u32 = 0xf2a493;
    pub const LAW: u32 = 0x5b8cff;
    pub const MINER: u32 = 0x58e0a0;
    pub const PANEL_EDGE: u32 = 0x2a3550;
}

/// Colorblind-safe swaps (Okabe-Ito based, CB_PALETTE): they separate the
/// red/green and pink/orange pairs the default neon palette leans on.
const CB_PALETTE: [(u32, u32); 10] = [
    (col::HAZARD, 0xe69f00),
    (col::CABLE, 0x56b4e9),
    (col::JUMPER, 0xcc79a7),
    (col::DANGER, 0xd55e00),
    (col::REACTOR, 0xf0e442),
    (col::ORE, 0xe8c66a),
    (col::MINER, 0x009e73),
    (col::LAW, 0x0072b2),
    (col::CRAB, 0xff9f6b),
    (col::COLLECTOR, 0xb8a6ff),
];

static COLORBLIND: AtomicBool = AtomicBool::new(false);
static REDUCE_MOTION: AtomicBool = AtomicBool::new(false);
static SPRITES: AtomicBool = AtomicBool::new(true);
/// The HUD's magnification while it is being drawn, as f32 bits; 1.0
/// (0x3f800000) the rest of the time, so menus and the field are unaffected.
static HUD_SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// Set while the HUD draws through a magnifying camera (main's frame
/// loop), so its text rasterizes at the size it is shown and stays sharp,
/// and so its buttons can map the mouse into HUD coordinates.
pub fn set_hud_scale(k: f32) {
    HUD_SCALE.store(k.to_bits(), Ordering::Relaxed);
}

pub fn hud_scale() -> f32 {
    f32::from_bits(HUD_SCALE.load(Ordering::Relaxed))
}

/// Choose painted sprites or the original vector outlines (Settings).
pub fn set_sprites(on: bool) {
    SPRITES.store(on, Ordering::Relaxed);
}

pub fn sprites() -> bool {
    SPRITES.load(Ordering::Relaxed)
}

/// Switch the palette (Settings.apply); every colour is looked up per draw.
pub fn set_colorblind(on: bool) {
    COLORBLIND.store(on, Ordering::Relaxed);
}

pub fn set_reduce_motion(on: bool) {
    REDUCE_MOTION.store(on, Ordering::Relaxed);
}

pub fn reduce_motion() -> bool {
    REDUCE_MOTION.load(Ordering::Relaxed)
}

/// motionScale: screen shake and flashes at 20% with reduced motion.
pub fn motion_scale() -> f64 {
    if reduce_motion() {
        0.2
    } else {
        1.0
    }
}

/// The palette colour for 0xRRGGBB, swapped when the colorblind palette is on.
pub fn pal(rgb: u32) -> u32 {
    if COLORBLIND.load(Ordering::Relaxed) {
        if let Some(&(_, cb)) = CB_PALETTE.iter().find(|p| p.0 == rgb) {
            return cb;
        }
    }
    rgb
}

/// 0xRRGGBB as a colour, through the palette.
pub fn hex(rgb: u32) -> Color {
    Color::from_hex(pal(rgb))
}

/// 0xRRGGBB plus alpha.
pub fn rgba(rgb: u32, a: f32) -> Color {
    let c = hex(rgb);
    Color::new(c.r, c.g, c.b, a)
}

/// The resolved colour of a simulation tint.
pub fn tint_rgb(t: cj_sim::events::Tint) -> u32 {
    use cj_sim::events::Tint::*;
    match t {
        Hazard => col::HAZARD,
        Cable => col::CABLE,
        Steel => col::STEEL,
        Rock => col::ROCK,
        Ore => col::ORE,
        Danger => col::DANGER,
        Text => col::TEXT,
        Wreck => col::WRECK,
        Jumper => col::JUMPER,
        Crab => col::CRAB,
        Reactor => col::REACTOR,
        Collector => col::COLLECTOR,
        Law => col::LAW,
        Miner => col::MINER,
        Muted => col::MUTED,
        Rgb(c) => c,
    }
}

/// The browser's three Google Fonts families, embedded at compile time so
/// nothing depends on the working directory or installed fonts.
pub struct Fonts {
    /// Big Shoulders Stencil Display Black (900): numbers, stencils.
    pub display: Font,
    /// Barlow Semi Condensed SemiBold: body text.
    pub body: Font,
    /// JetBrains Mono Bold (700): HUD and labels.
    pub mono: Font,
    /// JetBrains Mono Medium (500): small print.
    pub mono_light: Font,
}

impl Fonts {
    pub fn load() -> Fonts {
        let load = |bytes: &[u8]| load_ttf_font_from_bytes(bytes).expect("embedded font is valid");
        Fonts {
            display: load(include_bytes!(
                "../../../assets/fonts/BigShouldersStencilDisplay-Black.ttf"
            )),
            body: load(include_bytes!(
                "../../../assets/fonts/BarlowSemiCondensed-SemiBold.ttf"
            )),
            mono: load(include_bytes!(
                "../../../assets/fonts/JetBrainsMono-Bold.ttf"
            )),
            mono_light: load(include_bytes!(
                "../../../assets/fonts/JetBrainsMono-Medium.ttf"
            )),
        }
    }
}

// macroquad's default shader, reused with an additive blend state.
const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;
const FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

// Painted sprites lit in the world, not in the art: the sprite's own
// turn puts each texel back in world space, so a steady key light from the
// upper left stays put while the rock spins, and a blast lights only the
// side that faces it.
const LIT_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform float Turn;
uniform vec4 Key;
uniform vec4 Blast;
uniform vec4 BlastColor;
void main() {
    vec4 c = color * texture2D(Texture, uv);
    vec2 o = (uv - 0.5) * 2.0;
    float s = sin(Turn);
    float k = cos(Turn);
    vec2 w = vec2(o.x * k - o.y * s, o.x * s + o.y * k);
    vec3 rgb = c.rgb * (1.0 + Key.z * dot(w, Key.xy));
    float face = clamp(dot(w, Blast.xy) * 0.7 + 0.35, 0.0, 1.0);
    rgb += (c.rgb * 1.6 + 0.25) * BlastColor.rgb * Blast.z * face;
    gl_FragColor = vec4(rgb, c.a);
}"#;

/// The key light: toward the upper left (screen y points down), and how
/// much brighter the lit edge is than the middle (the far edge as much darker).
const KEY_LIGHT: [f32; 3] = [
    -std::f32::consts::FRAC_1_SQRT_2,
    -std::f32::consts::FRAC_1_SQRT_2,
    0.22,
];

// Rock art from the owner's asset pack (see assets/sprites/README.md).
macro_rules! rock_png {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/sprites/rocks/", $name, ".png"))
    };
}

/// Rock sprites: (size, ore-bearing, PNG). Sizes match `Rock::size`
/// (1 small, 2 medium, 3 large); ore-bearing art is the molten and
/// gold-plated rock.
const ROCK_ART: [(u8, bool, &[u8]); 27] = [
    (3, true, rock_png!("large_a_molten")),
    (3, true, rock_png!("large_b_metallic")),
    (3, true, rock_png!("breakup_large_molten")),
    (3, false, rock_png!("large_c_cratered")),
    (2, true, rock_png!("medium_01_molten")),
    (2, true, rock_png!("medium_02_metallic")),
    (2, true, rock_png!("medium_06_ore_slab")),
    (2, true, rock_png!("breakup_medium_01_molten")),
    (2, true, rock_png!("breakup_medium_03_metallic")),
    (2, false, rock_png!("medium_03_cratered")),
    (2, false, rock_png!("medium_07_cratered")),
    (2, false, rock_png!("breakup_medium_02_cratered")),
    (1, true, rock_png!("small_02_molten")),
    (1, true, rock_png!("small_03_metallic")),
    (1, true, rock_png!("small_09_metallic")),
    (1, true, rock_png!("breakup_small_02_molten")),
    (1, true, rock_png!("breakup_small_03_metallic")),
    (1, false, rock_png!("small_01_rock")),
    (1, false, rock_png!("small_04_cratered")),
    (1, false, rock_png!("small_07_rock")),
    (1, false, rock_png!("small_08_cratered")),
    (1, false, rock_png!("small_10_rock")),
    (1, false, rock_png!("breakup_small_01_rock")),
    (1, false, rock_png!("breakup_small_04_cratered")),
    (1, false, rock_png!("field_sample_rock_03")),
    (1, false, rock_png!("field_sample_rock_05")),
    (1, false, rock_png!("field_sample_rock_08")),
];

/// Which `Look::rocks` sprite draws a rock of this size and kind. The id
/// picks among the variants, so a rock keeps its look as it tumbles.
/// Rocks only some belts have: Quiet Drift's ice and blue crystal, the
/// Frontier's exotic crystal. (Belt, size, PNG), sizes as in `ROCK_ART`.
const BELT_ROCK_ART: [(Belt, u8, &[u8]); 7] = [
    (Belt::Quiet, 3, rock_png!("large_d_ice_crystal")),
    (Belt::Quiet, 2, rock_png!("medium_08_ice")),
    (Belt::Quiet, 2, rock_png!("medium_04_blue_crystal")),
    (Belt::Quiet, 1, rock_png!("small_05_blue_crystal")),
    (Belt::Frontier, 3, rock_png!("large_e_exotic_crystal")),
    (Belt::Frontier, 2, rock_png!("medium_05_exotic_crystal")),
    (Belt::Frontier, 1, rock_png!("small_06_exotic_crystal")),
];

/// The belt's own art for a rock, if it gets some: every other plain rock
/// in a belt that has its own; ore-bearing rock keeps its ore art so its
/// value still reads at a glance. An index into `Look::belt_rocks`.
pub fn belt_rock_art(belt: Belt, size: u8, ore: bool, id: u32) -> Option<usize> {
    let size = size.clamp(1, 3);
    if ore || id % 2 == 1 {
        return None;
    }
    let fits: Vec<usize> = (0..BELT_ROCK_ART.len())
        .filter(|&i| BELT_ROCK_ART[i].0 == belt && BELT_ROCK_ART[i].1 == size)
        .collect();
    (!fits.is_empty()).then(|| fits[(id / 2) as usize % fits.len()])
}

pub fn rock_art(size: u8, ore: bool, id: u32) -> usize {
    let size = size.clamp(1, 3);
    let fits =
        || (0..ROCK_ART.len()).filter(move |&i| ROCK_ART[i].0 == size && ROCK_ART[i].1 == ore);
    let n = fits().count();
    fits()
        .nth(id as usize % n)
        .expect("every size has art of both kinds")
}

macro_rules! ship_png {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/sprites/ships/", $name, ".png"))
    };
}

/// Player models, one per pilot slot (see `player_model`).
const PLAYER_ART: [&[u8]; 5] = [
    ship_png!("player_miner"),
    ship_png!("player_fighter"),
    ship_png!("player_interceptor"),
    ship_png!("player_heavy"),
    ship_png!("player_scout"),
];

/// Which player model pilot slot `slot` flies: they take turns, so the
/// first five pilots on a server all look different.
pub fn player_model(slot: usize) -> usize {
    slot % PLAYER_ART.len()
}

/// Ship sprites. The player and rival art points its nose up the image;
/// the sheriff, collector and hauler art points it to the right.
pub struct ShipArt {
    /// In `PLAYER_ART` order.
    pub players: Vec<Texture2D>,
    pub skiff: Texture2D,
    pub cutter: Texture2D,
    pub gunship: Texture2D,
    pub brute: Texture2D,
    pub sheriff: Texture2D,
    pub collector: Texture2D,
    pub hauler: Texture2D,
}

/// A painted sprite, with mipmaps: the art is ~512 px but drawn at ~50,
/// and plain linear filtering would sample it too sparsely and shimmer.
fn png(bytes: &[u8]) -> Texture2D {
    mipmapped(Texture2D::from_file_with_format(
        bytes,
        Some(ImageFormat::Png),
    ))
}

/// Gives `tex` mipmaps and trilinear filtering (see `png`).
fn mipmapped(tex: Texture2D) -> Texture2D {
    let id = tex.raw_miniquad_id();
    // SAFETY: called on the main thread after the window exists, like every
    // other texture call here; the id is this texture's own.
    let gl = unsafe { get_internal_gl() };
    gl.quad_context.texture_generate_mipmaps(id);
    gl.quad_context.texture_set_min_filter(
        id,
        FilterMode::Linear,
        macroquad::miniquad::MipmapFilterMode::Linear,
    );
    tex
}

impl ShipArt {
    fn load() -> ShipArt {
        ShipArt {
            players: PLAYER_ART.iter().map(|b| png(b)).collect(),
            skiff: png(ship_png!("rival_raider")),
            cutter: png(ship_png!("rival_interceptor")),
            gunship: png(ship_png!("rival_gunship")),
            brute: png(ship_png!("rival_elite")),
            sheriff: png(ship_png!("security_patrol_sheriff")),
            collector: png(ship_png!("debt_collector")),
            hauler: png(ship_png!("cargo_hauler")),
        }
    }
}

macro_rules! art_png {
    ($path:literal) => {
        include_bytes!(concat!("../../../assets/sprites/", $path, ".png"))
    };
}

/// Sprites for everything besides rocks and ships: drones, cargo, wrecks
/// and the two stations. Arrays run in grade order (see cj_sim::items).
/// The keycard, painted in code (the pack has none): a teal card with a
/// bevelled edge, a gold chip, a dark magnetic stripe and a lit status bar,
/// lying along x. Fine grain keeps it from looking flat next to the art.
pub fn keycard_image() -> Image {
    let (w, h) = (128u16, 84u16);
    let mut img = Image::gen_image_color(w, h, Color::new(0.0, 0.0, 0.0, 0.0));
    let (fw, fh) = (f32::from(w), f32::from(h));
    let corner = 12.0;
    let teal = |k: f32| Color::new(0.07 + 0.08 * k, 0.2 + 0.14 * k, 0.26 + 0.14 * k, 1.0);
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (f32::from(x) + 0.5, f32::from(y) + 0.5);
            // Distance outside the rounded rectangle (negative inside).
            let qx = (px - fw / 2.0).abs() - (fw / 2.0 - corner);
            let qy = (py - fh / 2.0).abs() - (fh / 2.0 - corner);
            let out = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - corner + 1.0;
            if out > 1.0 {
                continue;
            }
            let (u, v) = (px / fw, py / fh);
            let mut c = teal(1.0 - v);
            // Bevel: lit along the top and left, shaded along the bottom and right.
            if out > -3.0 {
                let lift = if u < 0.5 && v < 0.5 { 0.25 } else { -0.2 };
                c = Color::new(c.r + lift, c.g + lift, c.b + lift, 1.0);
            }
            if (0.2..0.36).contains(&v) {
                c = Color::new(0.04, 0.05, 0.06, 1.0);
            }
            if (0.12..0.34).contains(&u) && (0.48..0.76).contains(&v) {
                let line = ((u - 0.12) * 40.0).fract() < 0.18 || ((v - 0.48) * 30.0).fract() < 0.2;
                let gold = if line { 0.7 } else { 1.0 };
                c = Color::new(0.85 * gold, 0.66 * gold, 0.28 * gold, 1.0);
            }
            if (0.45..0.86).contains(&u) && (0.8..0.87).contains(&v) {
                c = Color::new(0.35, 0.95, 1.0, 1.0);
            }
            let grain = (((px * 12.9898 + py * 78.233).sin() * 43758.547).fract() - 0.5) * 0.04;
            let a = (1.0 - out).clamp(0.0, 1.0);
            img.set_pixel(
                u32::from(x),
                u32::from(y),
                Color::new(c.r + grain, c.g + grain, c.b + grain, a),
            );
        }
    }
    img
}

pub struct PropArt {
    /// Painted in code (see `keycard_image`).
    pub keycard: Texture2D,
    pub security_drone: Texture2D,
    pub mining_drone: Texture2D,
    pub sentry: Texture2D,
    pub rogue_sentry: Texture2D,
    /// Bent, plain, alloy.
    pub scrap: [Texture2D; 3],
    /// Iron, copper, cobalt, platinum.
    pub ore: [Texture2D; 4],
    pub plate: Texture2D,
    /// Burnt-out, block, fusion drive.
    pub engine: [Texture2D; 3],
    /// Cracked, standard, pristine.
    pub reactor: [Texture2D; 3],
    /// Silver, gold, iridium.
    pub ingot: [Texture2D; 3],
    pub rogue_core: Texture2D,
    pub blackbox: Texture2D,
    pub pod: Texture2D,
    pub charge: Texture2D,
    /// Debris for ordinary wrecks (pick with `wreck_art`).
    pub wrecks: Vec<Texture2D>,
    pub freighter: Texture2D,
    /// The salvage carrier's main hull.
    pub carrier: Texture2D,
    /// The refinery's hub and vault.
    pub refinery: Texture2D,
}

impl PropArt {
    fn load() -> PropArt {
        PropArt {
            security_drone: png(art_png!("drones/security_drone")),
            mining_drone: png(art_png!("drones/mining_drone")),
            sentry: png(art_png!("drones/sentry_drone")),
            rogue_sentry: png(art_png!("drones/rogue_sentry_drone")),
            scrap: [
                png(art_png!("items/hull_fragment_02")),
                png(art_png!("items/hull_fragment_01")),
                png(art_png!("items/hull_fragment_03")),
            ],
            ore: [
                png(art_png!("items/iron_ore")),
                png(art_png!("items/copper_ore")),
                png(art_png!("items/cobalt_ore")),
                png(art_png!("items/platinum_ore")),
            ],
            plate: png(art_png!("items/hull_plating_large")),
            engine: [
                png(art_png!("items/engine_fragment_damaged")),
                png(art_png!("items/engine_section_intact")),
                png(art_png!("items/turbine_assembly")),
            ],
            reactor: [
                png(art_png!("items/micro_core_low_yield")),
                png(art_png!("items/standard_core")),
                png(art_png!("items/advanced_core_high_yield")),
            ],
            ingot: [
                png(art_png!("items/nickel_ore")),
                png(art_png!("items/iridium_ore")),
                png(art_png!("items/xenon_crystal")),
            ],
            rogue_core: png(art_png!("items/unstable_core")),
            blackbox: png(art_png!("items/cargo_container_standard")),
            pod: png(art_png!("items/rare_cargo")),
            charge: png(art_png!("items/hazardous_material_container")),
            keycard: mipmapped(Texture2D::from_image(&keycard_image())),
            wrecks: vec![
                png(art_png!("wrecks/structural_frame")),
                png(art_png!("wrecks/fuel_tank_leaking")),
                png(art_png!("wrecks/sensor_array_damaged")),
                png(art_png!("wrecks/fuel_manifold")),
                png(art_png!("wrecks/radiator_panel")),
            ],
            freighter: png(art_png!("wrecks/derelict_carrier")),
            carrier: png(art_png!("stations/hub_ship_collection_dropoff")),
            refinery: png(art_png!("stations/space_mining_vault")),
        }
    }
}

/// Which `PropArt::wrecks` piece draws a wreck: its emergency light's
/// phase is random per wreck and never changes, so it keeps its look.
pub fn wreck_art(light: f64, n: usize) -> usize {
    (light.abs() * 1000.0) as usize % n.max(1)
}

/// The art for grade `grade` among `n` grades, clamped so a grade the
/// art doesn't cover falls back to the last one.
pub fn grade_art(grade: usize, n: usize) -> usize {
    grade.min(n.saturating_sub(1))
}

pub struct Look {
    pub fonts: Fonts,
    /// Rock sprites, in `ROCK_ART` order (pick one with `rock_art`).
    pub rocks: Vec<Texture2D>,
    /// In `BELT_ROCK_ART` order (see `belt_rock_art`).
    pub belt_rocks: Vec<Texture2D>,
    pub ships: ShipArt,
    pub props: PropArt,
    /// Source × alpha added to the frame: canvas 'lighter'.
    additive: Material,
    /// Painted sprites under the world's lights (see `LIT_FRAGMENT`).
    lit: Material,
    /// Glow sprites per (colour, hot), built on first use like the browser's spriteCache.
    sprites: RefCell<HashMap<(u32, bool), Texture2D>>,
}

impl Look {
    pub fn new() -> Look {
        let additive = load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: FRAGMENT,
            },
            MaterialParams {
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::One,
                    )),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .expect("additive material compiles (checked in M0)");
        let lit = load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: LIT_FRAGMENT,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("Turn", UniformType::Float1),
                    UniformDesc::new("Key", UniformType::Float4),
                    UniformDesc::new("Blast", UniformType::Float4),
                    UniformDesc::new("BlastColor", UniformType::Float4),
                ],
                // A material starts with blending off; sprites need the usual alpha blend.
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    )),
                    ..Default::default()
                },
                textures: vec![],
            },
        )
        .expect("lit material compiles");
        Look {
            fonts: Fonts::load(),
            rocks: ROCK_ART.iter().map(|(_, _, b)| png(b)).collect(),
            belt_rocks: BELT_ROCK_ART.iter().map(|(_, _, b)| png(b)).collect(),
            ships: ShipArt::load(),
            props: PropArt::load(),
            additive,
            lit,
            sprites: RefCell::new(HashMap::new()),
        }
    }

    /// Run `f` (one sprite turned by `turn`) under the key light and, if
    /// given, a blast's light: (direction toward it x, y, strength, colour).
    pub fn lit(&self, turn: f32, blast: Option<(f64, f64, f64, u32)>, f: impl FnOnce()) {
        let m = &self.lit;
        m.set_uniform("Turn", turn);
        let [kx, ky, kz] = KEY_LIGHT;
        m.set_uniform("Key", [kx, ky, kz, 0.0]);
        let (bx, by, bk, rgb) = blast.unwrap_or((0.0, 0.0, 0.0, 0));
        m.set_uniform("Blast", [bx as f32, by as f32, bk as f32, 0.0]);
        let c = hex(rgb);
        m.set_uniform("BlastColor", [c.r, c.g, c.b, 1.0]);
        gl_use_material(m);
        f();
        gl_use_default_material();
    }

    /// Run `f` with additive blending.
    pub fn additive(&self, f: impl FnOnce()) {
        gl_use_material(&self.additive);
        f();
        gl_use_default_material();
    }

    /// The browser's glowSprite: a 64 px radial gradient. Hot sprites have
    /// a white core (sparks, flashes); soft ones fade evenly (nebulae).
    fn sprite(&self, rgb: u32, hot: bool) -> Texture2D {
        // Cached by the colour actually drawn, so the palette swap applies.
        let rgb = pal(rgb);
        if let Some(t) = self.sprites.borrow().get(&(rgb, hot)) {
            return t.clone();
        }
        let c = Color::from_hex(rgb);
        let stops: &[(f32, [f32; 4])] = if hot {
            &[
                (0.0, [1.0, 1.0, 1.0, 1.0]),
                (0.2, [c.r, c.g, c.b, 0.95]),
                (0.5, [c.r, c.g, c.b, 0.3]),
                (1.0, [c.r, c.g, c.b, 0.0]),
            ]
        } else {
            &[
                (0.0, [c.r, c.g, c.b, 1.0]),
                (0.5, [c.r, c.g, c.b, 0.35]),
                (1.0, [c.r, c.g, c.b, 0.0]),
            ]
        };
        let size = 64u16;
        let mut img = Image::gen_image_color(size, size, Color::new(0.0, 0.0, 0.0, 0.0));
        for y in 0..u32::from(size) {
            for x in 0..u32::from(size) {
                let d = ((x as f32 + 0.5 - 32.0).powi(2) + (y as f32 + 0.5 - 32.0).powi(2)).sqrt()
                    / 32.0;
                let v = gradient(stops, d.min(1.0));
                img.set_pixel(x, y, Color::new(v[0], v[1], v[2], v[3]));
            }
        }
        let tex = Texture2D::from_image(&img);
        tex.set_filter(FilterMode::Linear);
        self.sprites.borrow_mut().insert((rgb, hot), tex.clone());
        tex
    }

    /// Stamp a glow of `radius` at (x, y) with overall `alpha`. Call inside
    /// [`Look::additive`] for light, as the browser does.
    pub fn glow(&self, x: f32, y: f32, radius: f32, rgb: u32, hot: bool, alpha: f32) {
        let tex = self.sprite(rgb, hot);
        draw_texture_ex(
            &tex,
            x - radius,
            y - radius,
            Color::new(1.0, 1.0, 1.0, alpha),
            DrawTextureParams {
                dest_size: Some(vec2(radius * 2.0, radius * 2.0)),
                ..Default::default()
            },
        );
    }
}

/// Linear interpolation between colour stops at position `t` (0..1).
fn gradient(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    for w in stops.windows(2) {
        let ((t0, a), (t1, b)) = (w[0], w[1]);
        if t <= t1 {
            let k = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k);
        }
    }
    stops[stops.len() - 1].1
}

/// Text drawn centred on (x, y) (canvas textAlign center, textBaseline middle).
/// `scale` lets world-space text rasterize at its on-screen size.
pub fn text_centered(text: &str, x: f32, y: f32, size: f32, font: &Font, color: Color, scale: f32) {
    let px = (size * scale).round().clamp(1.0, 400.0) as u16;
    let font_scale = 1.0 / scale;
    let dims = measure_text(text, Some(font), px, font_scale);
    draw_text_ex(
        text,
        x - dims.width / 2.0,
        y + dims.offset_y / 2.0 - dims.height * 0.06,
        TextParams {
            font: Some(font),
            font_size: px,
            font_scale,
            color,
            ..Default::default()
        },
    );
}

/// Text with its left edge at `x` and its top at `y`; returns its width.
/// Under a HUD scale `k`, glyphs rasterize at `size * k` pixels and are
/// drawn at 1/k, so they land at `size` in HUD units but stay sharp after
/// the camera magnifies them. At k = 1 this is exactly the plain call.
pub fn text_left(text: &str, x: f32, y: f32, size: f32, font: &Font, color: Color) -> f32 {
    let k = hud_scale();
    let px = (size * k) as u16;
    let dims = measure_text(text, Some(font), px, 1.0 / k);
    draw_text_ex(
        text,
        x,
        y + dims.offset_y,
        TextParams {
            font: Some(font),
            font_size: px,
            font_scale: 1.0 / k,
            color,
            ..Default::default()
        },
    );
    dims.width
}

/// Width of a run of text at `size` (in HUD units under a HUD scale).
pub fn text_width(text: &str, size: f32, font: &Font) -> f32 {
    let k = hud_scale();
    measure_text(text, Some(font), (size * k) as u16, 1.0 / k).width
}

/// A five-point star (★ filled, ☆ hollow) centred on (x, y). None of the
/// game's fonts has the star glyphs (the browser falls back to a system
/// font), so the wanted stars are drawn.
pub fn star(x: f32, y: f32, r: f32, filled: bool, color: Color) {
    let pts: Vec<Vec2> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            vec2(x + a.cos() * rr, y + a.sin() * rr)
        })
        .collect();
    if filled {
        crate::draw::fill_poly(&pts, color);
    } else {
        crate::draw::polyline(&pts, true, 1.0, color);
    }
}

/// Centred world text in which any ★ is drawn as a star shape.
pub fn text_centered_stars(
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    font: &Font,
    color: Color,
    scale: f32,
) {
    if !text.contains('★') {
        text_centered(text, x, y, size, font, color, scale);
        return;
    }
    let (head, rest) = text.split_once('★').expect("contains a star");
    let n = 1 + rest.matches('★').count();
    let star_w = size * 0.9;
    let hw = text_width(head, size, font);
    let total = hw + star_w * n as f32;
    let left = x - total / 2.0;
    text_centered(head, left + hw / 2.0, y, size, font, color, scale);
    for i in 0..n {
        star(
            left + hw + star_w * (i as f32 + 0.5),
            y,
            size * 0.38,
            true,
            color,
        );
    }
}
