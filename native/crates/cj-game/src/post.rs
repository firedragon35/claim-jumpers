//! Full-screen effects for the field: bloom, a colour grade with film
//! grain, and a procedural nebula behind the stars.
//!
//! While the effects are on, `draw::render` draws the field into an
//! off-screen target instead of the window. `display_camera` reads the
//! active target from this module, so every camera the field sets lands
//! there without each draw call knowing about it. `finish` then blurs the
//! bright parts at three sizes and composites everything onto the window,
//! before the screen-space overlays (night, flashes) and the HUD, which
//! stay sharp.
//!
//! If a shader fails to compile on some driver, the effects switch
//! themselves off and the field draws exactly as it did without them.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation};
use macroquad::prelude::*;
use std::cell::RefCell;

thread_local! {
    /// The target the field draws into this frame; None draws to the window.
    static TARGET: RefCell<Option<RenderTarget>> = const { RefCell::new(None) };
}

/// The render target cameras should draw into right now, if any.
pub(crate) fn target() -> Option<RenderTarget> {
    TARGET.with(|t| t.borrow().clone())
}

fn set_target(rt: Option<RenderTarget>) {
    TARGET.with(|t| *t.borrow_mut() = rt);
}

/// Shared vertex shader. Same as macroquad's default, plus `pos`: the
/// vertex position in the camera's units, which the nebula samples so it
/// is laid out in logical pixels whatever the window's pixel ratio.
const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying vec2 uv;
varying vec2 pos;
varying vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
    pos = position.xy;
}"#;

/// Halve the resolution with a 4-tap box filter and keep only what is
/// brighter than `Threshold` (a soft knee, so glow fades in rather than
/// popping). A threshold below zero passes everything through.
const DOWN: &str = r#"#version 100
precision highp float;
varying vec2 uv;
uniform sampler2D Texture;
uniform vec2 Texel;
uniform float Threshold;
void main() {
    vec3 c = texture2D(Texture, uv + Texel * vec2(-0.5, -0.5)).rgb
           + texture2D(Texture, uv + Texel * vec2( 0.5, -0.5)).rgb
           + texture2D(Texture, uv + Texel * vec2(-0.5,  0.5)).rgb
           + texture2D(Texture, uv + Texel * vec2( 0.5,  0.5)).rgb;
    c *= 0.25;
    float b = max(c.r, max(c.g, c.b));
    float k = smoothstep(Threshold, Threshold + 0.3, b);
    gl_FragColor = vec4(c * k, 1.0);
}"#;

/// One direction of a 9-tap Gaussian, done in 5 samples by letting the
/// linear filter blend neighbouring texels (offsets 1.38 and 3.23).
const BLUR: &str = r#"#version 100
precision highp float;
varying vec2 uv;
uniform sampler2D Texture;
uniform vec2 Dir;
void main() {
    vec3 c = texture2D(Texture, uv).rgb * 0.227027;
    c += texture2D(Texture, uv + Dir * 1.384615).rgb * 0.316216;
    c += texture2D(Texture, uv - Dir * 1.384615).rgb * 0.316216;
    c += texture2D(Texture, uv + Dir * 3.230769).rgb * 0.070270;
    c += texture2D(Texture, uv - Dir * 3.230769).rgb * 0.070270;
    gl_FragColor = vec4(c, 1.0);
}"#;

/// The frame: scene plus three blurred glow levels (tight to wide), then
/// a gentle grade (cool shadows, warm highlights), a soft shoulder so
/// stacked glow doesn't flatten to white, and faint film grain.
const COMPOSITE: &str = r#"#version 100
precision highp float;
varying vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Bloom1;
uniform sampler2D Bloom2;
uniform sampler2D Bloom3;
uniform float Strength;
uniform float Seed;
void main() {
    vec3 c = texture2D(Texture, uv).rgb;
    vec3 glow = texture2D(Bloom1, uv).rgb * 0.9
              + texture2D(Bloom2, uv).rgb * 0.8
              + texture2D(Bloom3, uv).rgb * 0.7;
    c += glow * Strength;
    float l = dot(c, vec3(0.299, 0.587, 0.114));
    c += vec3(-0.004, 0.006, 0.014) * (1.0 - l);
    c *= mix(vec3(1.0), vec3(1.05, 1.0, 0.94), clamp(l, 0.0, 1.0));
    vec3 over = max(c - 0.8, 0.0);
    c = min(c, 0.8) + over / (1.0 + over * 2.5);
    float n = fract(sin(dot(uv * 1000.0 + Seed, vec2(12.9898, 78.233))) * 43758.5453);
    c += (n - 0.5) * 0.018;
    gl_FragColor = vec4(c, 1.0);
}"#;

/// Domain-warped value-noise clouds in three colours with darker dust
/// lanes. Kept dim (peak brightness well under the bloom threshold) so
/// the background never glows or competes with the play field.
const NEBULA: &str = r#"#version 100
precision highp float;
varying vec2 pos;
uniform vec2 Offset;
uniform float Time;
uniform vec3 ColA;
uniform vec3 ColB;
uniform vec3 ColC;
float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}
float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
               mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}
float fbm(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 5; i++) {
        v += a * noise(p);
        p = p * 2.03 + vec2(17.1, 9.2);
        a *= 0.5;
    }
    return v;
}
void main() {
    vec2 p = (pos + Offset) / 560.0;
    vec2 q = vec2(fbm(p + vec2(Time * 0.006, 0.0)), fbm(p + vec2(5.2, 1.3)));
    float n = fbm(p + 1.7 * q);
    float density = smoothstep(0.42, 0.82, n);
    vec3 col = mix(ColA, ColB, clamp(q.x * 1.6 - 0.35, 0.0, 1.0));
    col = mix(col, ColC, smoothstep(0.5, 0.85, q.y) * 0.7);
    float dust = smoothstep(0.5, 0.75, fbm(p * 2.4 + q * 2.0));
    gl_FragColor = vec4(col * 1.25, density * 0.6 * (1.0 - 0.6 * dust));
}"#;

/// How much of the blurred glow is added back to the frame.
const BLOOM_STRENGTH: f32 = 0.85;
/// Brightness (0–1, brightest channel) where glow starts.
const BLOOM_THRESHOLD: f32 = 0.42;
/// How far the nebula drifts per world unit the camera moves: matches
/// the old sprite nebulae, so the background parallax is unchanged.
const NEBULA_PARALLAX: f32 = 0.02;

/// The compiled shaders. Built once; all of them or none.
struct Mats {
    down: Material,
    blur: Material,
    composite: Material,
    nebula: Material,
}

impl Mats {
    fn load() -> Result<Mats, macroquad::Error> {
        // Custom materials don't blend unless asked: the passes overwrite
        // their target, but the nebula must lay over the background.
        let alpha_blend = PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };
        let mat = |fragment: &'static str,
                   uniforms: &[(&str, UniformType)],
                   textures: &[&str],
                   pipeline_params: PipelineParams| {
            load_material(
                ShaderSource::Glsl {
                    vertex: VERTEX,
                    fragment,
                },
                MaterialParams {
                    uniforms: uniforms
                        .iter()
                        .map(|&(n, t)| UniformDesc::new(n, t))
                        .collect(),
                    textures: textures.iter().map(|&t| t.to_owned()).collect(),
                    pipeline_params,
                },
            )
        };
        use UniformType::{Float1, Float2, Float3};
        Ok(Mats {
            down: mat(
                DOWN,
                &[("Texel", Float2), ("Threshold", Float1)],
                &[],
                PipelineParams::default(),
            )?,
            blur: mat(BLUR, &[("Dir", Float2)], &[], PipelineParams::default())?,
            composite: mat(
                COMPOSITE,
                &[("Strength", Float1), ("Seed", Float1)],
                &["Bloom1", "Bloom2", "Bloom3"],
                PipelineParams::default(),
            )?,
            nebula: mat(
                NEBULA,
                &[
                    ("Offset", Float2),
                    ("Time", Float1),
                    ("ColA", Float3),
                    ("ColB", Float3),
                    ("ColC", Float3),
                ],
                &[],
                alpha_blend,
            )?,
        })
    }
}

/// One bloom level: the downsampled image and a scratch target the
/// horizontal blur writes into before the vertical one writes back.
struct Level {
    img: RenderTarget,
    tmp: RenderTarget,
    size: Vec2,
}

/// Off-screen targets sized to the window in physical pixels.
struct Targets {
    /// Window size in physical pixels these were made for.
    px: (u32, u32),
    /// The field, multisampled like the window so lines stay smooth.
    scene: RenderTarget,
    levels: [Level; 3],
}

impl Targets {
    fn new(px: (u32, u32)) -> Targets {
        let linear = |rt: RenderTarget| {
            rt.texture.set_filter(FilterMode::Linear);
            rt
        };
        let level = |div: u32| {
            let (w, h) = ((px.0 / div).max(1), (px.1 / div).max(1));
            Level {
                img: linear(render_target(w, h)),
                tmp: linear(render_target(w, h)),
                size: vec2(w as f32, h as f32),
            }
        };
        Targets {
            px,
            scene: linear(render_target_msaa(px.0, px.1)),
            levels: [level(2), level(4), level(8)],
        }
    }
}

/// The field's post-processing: owned by the game loop, used by `render`.
pub struct Post {
    /// None when a shader failed to compile: effects stay off.
    mats: Option<Mats>,
    targets: Option<Targets>,
    /// Whether this frame's field went into the scene target.
    active: bool,
}

impl Post {
    pub fn new() -> Post {
        let mats = Mats::load()
            .map_err(|e| eprintln!("effects off: a post-processing shader failed: {e:?}"))
            .ok();
        Post {
            mats,
            targets: None,
            active: false,
        }
    }

    /// Start a frame. With `on`, the field's cameras draw into the scene
    /// target until `finish`; returns whether that happened.
    pub fn begin(&mut self, on: bool, view: (f32, f32), dpi: f32) -> bool {
        self.active = on && self.mats.is_some();
        if !self.active {
            set_target(None);
            return false;
        }
        let px = (
            ((view.0 * dpi).round() as u32).max(1),
            ((view.1 * dpi).round() as u32).max(1),
        );
        // Rebuilt on resize (and on first use); otherwise reused each frame.
        if self.targets.as_ref().is_none_or(|t| t.px != px) {
            self.targets = Some(Targets::new(px));
        }
        set_target(self.targets.as_ref().map(|t| t.scene.clone()));
        true
    }

    /// Draw the nebula over the current (screen-space) camera, if the
    /// effects are on this frame. Returns false so the caller can fall
    /// back to the sprite nebulae.
    pub fn nebula(&self, cam: Vec2, t: f32, view: (f32, f32)) -> bool {
        let Some(m) = self.mats.as_ref().filter(|_| self.active) else {
            return false;
        };
        let rgb = |hex: u32| {
            let c = Color::from_hex(hex);
            vec3(c.r, c.g, c.b)
        };
        let n = &m.nebula;
        n.set_uniform("Offset", cam * NEBULA_PARALLAX);
        n.set_uniform("Time", t);
        // The palette of the old sprite nebulae: blue, violet, teal.
        n.set_uniform("ColA", rgb(0x2a3a7a));
        n.set_uniform("ColB", rgb(0x4a2463));
        n.set_uniform("ColC", rgb(0x14505a));
        gl_use_material(n);
        draw_rectangle(0.0, 0.0, view.0, view.1, WHITE);
        gl_use_default_material();
        true
    }

    /// End the field: blur its bright parts and composite it onto the
    /// window, then hand the cameras back to the window. `seed` moves the
    /// grain; a constant seed holds it still.
    pub fn finish(&mut self, view: (f32, f32), seed: f32) {
        set_target(None);
        if !self.active {
            return;
        }
        let (Some(m), Some(t)) = (self.mats.as_ref(), self.targets.as_ref()) else {
            return;
        };

        // Bright pass into the half-size level, then plain downsamples
        // into the quarter and eighth; each level is blurred both ways.
        let full = vec2(t.px.0 as f32, t.px.1 as f32);
        let mut src = (&t.scene.texture, full, BLOOM_THRESHOLD);
        for l in &t.levels {
            m.down.set_uniform("Texel", vec2(1.0, 1.0) / src.1);
            m.down.set_uniform("Threshold", src.2);
            pass(&l.img, &m.down, src.0);
            m.blur.set_uniform("Dir", vec2(1.0 / l.size.x, 0.0));
            pass(&l.tmp, &m.blur, &l.img.texture);
            m.blur.set_uniform("Dir", vec2(0.0, 1.0 / l.size.y));
            pass(&l.img, &m.blur, &l.tmp.texture);
            src = (&l.img.texture, l.size, -1.0);
        }

        // Composite onto the window, covering every pixel (no clear needed).
        let c = &m.composite;
        c.set_texture("Bloom1", t.levels[0].img.texture.clone());
        c.set_texture("Bloom2", t.levels[1].img.texture.clone());
        c.set_texture("Bloom3", t.levels[2].img.texture.clone());
        c.set_uniform("Strength", BLOOM_STRENGTH);
        c.set_uniform("Seed", seed);
        crate::draw::screen_camera(view);
        gl_use_material(c);
        draw_texture_ex(
            &t.scene.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(view.0, view.1)),
                // No flip: the field went in through the same Y-down
                // camera this draws with, so the two cancel out.
                ..Default::default()
            },
        );
        gl_use_default_material();
    }
}

/// Run `mat` over all of `src`, writing every pixel of `dst`. Uniforms
/// are captured per draw call, so the next pass may change them freely.
fn pass(dst: &RenderTarget, mat: &Material, src: &Texture2D) {
    // The one deliberate use of from_display_rect (see CLAUDE.md). Every
    // bloom level must line up with the scene when composited, so each
    // pass has to keep rows in the same order; this camera alone flips a
    // render target's rows, so the draw flips them back.
    let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, 1.0, 1.0));
    cam.render_target = Some(dst.clone());
    set_camera(&cam);
    gl_use_material(mat);
    draw_texture_ex(
        src,
        0.0,
        0.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(1.0, 1.0)),
            flip_y: true,
            ..Default::default()
        },
    );
    gl_use_default_material();
}
