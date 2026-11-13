//! Palette, embedded fonts, glow sprites and the additive blend: the
//! pieces the browser builds its look from (src/js/02-utilities.js and the
//! top of src/js/17-rendering.js).

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
pub fn rock_art(size: u8, ore: bool, id: u32) -> usize {
    let size = size.clamp(1, 3);
    let fits =
        || (0..ROCK_ART.len()).filter(move |&i| ROCK_ART[i].0 == size && ROCK_ART[i].1 == ore);
    let n = fits().count();
    fits()
        .nth(id as usize % n)
        .expect("every size has art of both kinds")
}

pub struct Look {
    pub fonts: Fonts,
    /// Rock sprites, in `ROCK_ART` order (pick one with `rock_art`).
    pub rocks: Vec<Texture2D>,
    /// Source × alpha added to the frame: canvas 'lighter'.
    additive: Material,
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
        Look {
            fonts: Fonts::load(),
            rocks: ROCK_ART
                .iter()
                .map(|(_, _, png)| Texture2D::from_file_with_format(png, Some(ImageFormat::Png)))
                .collect(),
            additive,
            sprites: RefCell::new(HashMap::new()),
        }
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
