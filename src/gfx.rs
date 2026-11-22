//! Графический режим: OpenGL 3.3. Сцена рисуется в два HDR-канала
//! (R — свет основного цвета, G — «белый» свет голов и бликов), затем bloom
//! и раскраска в композите — поэтому сдвиг гаммы к красному бесплатный.

use crate::config::{Config, Flow};
use crate::scene::Scene;
use crate::shape::Text;
use crate::xwin::Monitor;
use glow::HasContext;
use std::collections::HashMap;

const BUILTIN_FONT: &[u8] = include_bytes!("../assets/fonts/MatrixClockGlyphs.otf");
/// Шрифт надписей (погода, дата) к угловатым цифрам — подмножество Russo One.
const ANGULAR_TEXT_FONT: &[u8] = include_bytes!("../assets/fonts/MatrixClockText.ttf");
const TILE: usize = 64;
/// В графике цифры жирнее, чем в тексте (как на референсах).
const THICK: f32 = 1.45;

const FULLSCREEN_VS: &str = r#"#version 330 core
const vec2 P[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
out vec2 vUV;
void main() { vec2 p = P[gl_VertexID]; vUV = p * 0.5 + 0.5; gl_Position = vec4(p, 0.0, 1.0); }
"#;

/// Общие функции доступа к полю расстояний (в пикселях окна, y вверх):
/// канал 0 (R) — цифры, канал 1 (G) — надписи (погода, дата).
const SDF_COMMON: &str = r#"
uniform sampler2D uSdf;
uniform vec2 uRes;
uniform vec2 uExt;   // шаг выдавливания объёма цифр, px
uniform int uSteps;
float sdfCh(vec2 f, int c) { vec2 v = texture(uSdf, f / uRes).rg; return c == 0 ? v.r : v.g; }
float sdfAt(vec2 f) { return sdfCh(f, 0); }
// расстояние до всей объёмной фигуры; k — номер слоя боковой грани (0 — нет)
float unionCh(vec2 f, int c, vec2 ext, int steps, out float k) {
    float d = sdfCh(f, c); k = 0.0;
    for (int i = 1; i <= steps; i++) {
        float dk = sdfCh(f - ext * float(i), c);
        d = min(d, dk);
        if (dk < 0.0 && k == 0.0) k = float(i);
    }
    return d;
}
float unionD(vec2 f, out float k) { return unionCh(f, 0, uExt, uSteps, k); }
"#;

const GLYPH_VS: &str = r#"#version 330 core
layout(location = 0) in vec4 aRect;  // x, y, w, h — px, начало слева сверху
layout(location = 1) in vec4 aData;  // глиф, яркость, белизна, флаги
uniform vec2 uRes;
uniform float uCols;
out vec2 vUV;
flat out vec4 vData;
void main() {
    vec2 c = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 px = aRect.xy + c * aRect.zw;
    gl_Position = vec4(px.x / uRes.x * 2.0 - 1.0, 1.0 - px.y / uRes.y * 2.0, 0.0, 1.0);
    if (mod(aData.w, 8.0) >= 4.0) c.x = 1.0 - c.x;   // зеркальный символ
    vec2 tile = vec2(mod(aData.x, uCols), floor(aData.x / uCols));
    vUV = (tile + c) / uCols;
    vData = aData;
}
"#;

const GLYPH_FS: &str = r#"
uniform sampler2D uAtlas;
in vec2 vUV;
flat in vec4 vData;
out vec4 o;
void main() {
    float a = texture(uAtlas, vUV).r;
    float mask = mod(vData.w, 4.0);
    if (mask > 0.5) {
        float k;
        if (mask < 1.5) a *= smoothstep(0.8, -0.8, sdfAt(gl_FragCoord.xy));          // только внутри цифр
        else            a *= smoothstep(-0.8, 0.8, unionD(gl_FragCoord.xy, k));       // только снаружи
    }
    float I = vData.y, W = vData.z;
    o = vec4(a * I * (1.0 - 0.65 * W), a * I * W, 0.0, a * I);
}
"#;

const SDF_FS: &str = r#"#version 330 core
uniform vec4 uP[128];
uniform int uN;
uniform vec2 uOff;     // положение часов, px окна (слева сверху)
uniform float uScale;  // px на единицу глифа
uniform float uStroke; // полуширина штриха, единиц глифа
uniform float uOct;    // 1 — восьмиугольное «перо» (угловатые цифры), 0 — круглое
uniform float uH;
out vec4 o;
void main() {
    vec2 g = (vec2(gl_FragCoord.x, uH - gl_FragCoord.y) - uOff) / uScale;
    float d = 1e9;
    for (int i = 0; i < uN; i++) {
        vec4 A = uP[2 * i], B = uP[2 * i + 1];
        if (B.z < 0.5) {
            vec2 pa = g - A.xy, ba = A.zw - A.xy;
            float t = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
            vec2 v = abs(pa - ba * t);
            d = min(d, uOct > 0.5 ? max(max(v.x, v.y), (v.x + v.y) * 0.70710678) : length(v));
        } else {
            vec2 q = g - A.xy;
            float ang = B.x + mod(atan(q.y, q.x) - B.x, 6.2831853);
            if (ang <= B.y) d = min(d, abs(length(q) - A.z));
            else d = min(d, min(length(q - A.z * vec2(cos(B.x), sin(B.x))), length(q - A.z * vec2(cos(B.y), sin(B.y)))));
        }
    }
    o = vec4((d - uStroke) * uScale, 1.0e4, 0.0, 1.0);
}
"#;

/// Объёмные стеклянные цифры и надписи: фаска с освещением, блик, светящаяся
/// кромка, тёмная боковая грань с рёбрами, ореол. Надписи — тот же расчёт
/// со своими (пропорциональными размеру шрифта) объёмом, штрихом и ореолом.
const DIGIT_FS: &str = r#"
uniform float uStrokePx, uBevel, uSpec, uRim, uGlowR, uGlowS, uBright;
uniform vec2 uExtT;
uniform int uStepsT;
uniform float uStrokeT, uGlowT;
in vec2 vUV;
out vec4 o;
vec2 shade(vec2 f, int c, vec2 ext, int steps, float strokePx, float glowR) {
    float d = sdfCh(f, c);
    float k;
    float u = unionCh(f, c, ext, steps, k);
    float R = 0.0, G = 0.0;
    if (d < 0.0) {
        float bw = max(uBevel * strokePx, 1.5);
        vec2 grad = vec2(sdfCh(f + vec2(1.0, 0.0), c) - sdfCh(f - vec2(1.0, 0.0), c),
                         sdfCh(f + vec2(0.0, 1.0), c) - sdfCh(f - vec2(0.0, 1.0), c)) * 0.5;
        float t = clamp(-d / bw, 0.0, 1.0);
        float slope = 1.0 - t * t;                       // скругление у края
        vec3 n = normalize(vec3(grad * slope * 1.8, 1.0));
        vec3 L = normalize(vec3(-0.5, 0.7, 0.6));
        float diff = max(dot(n, L), 0.0);
        float spec = pow(max(dot(n, normalize(L + vec3(0.0, 0.0, 1.0))), 0.0), 48.0) * uSpec;
        float edge = exp(d / 1.2);                       // тонкая яркая кромка
        float tube = exp(d / (bw * 0.9));                // «неон» у краёв внутри
        R = (0.015 + 0.05 * diff) * uBright + 0.1 * tube * uRim + 0.9 * edge * uRim + 0.2 * spec;
        G = spec * 0.8 + edge * uRim * 0.12;
    } else if (u < 0.0) {
        float t = k / float(max(steps, 1));
        float back = sdfCh(f - ext * float(steps), c);
        // боковая грань: освещена у лицевой кромки, темнеет вглубь
        R = (0.16 * (1.0 - t) + 0.04) * uBright
          + 0.12 * exp(-abs(back) / 1.0) * uRim          // контур задней грани
          + 0.45 * exp(u / 1.1) * uRim;                  // силуэт
    } else {
        R = uGlowS * 0.12 * exp(-u / glowR) + 0.2 * exp(-u / 1.0) * uRim;
    }
    return vec2(R, G);
}
void main() {
    vec2 f = gl_FragCoord.xy;
    vec2 c = shade(f, 0, uExt, uSteps, uStrokePx, uGlowR) + shade(f, 1, uExtT, uStepsT, uStrokeT, uGlowT);
    o = vec4(c, 0.0, 1.0);
}
"#;

/// Поле расстояний надписи → в общее поле цифр (смешивание MIN): дальше её рисует
/// DIGIT_FS — тем же объёмом, фаской, кромкой и ореолом, что и часы.
const TEXT_VS: &str = r#"#version 330 core
uniform vec4 uRect;  // x, y, w, h — px, слева сверху
uniform vec2 uRes;
out vec2 vUV;
void main() {
    vec2 c = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 px = uRect.xy + c * uRect.zw;
    gl_Position = vec4(px.x / uRes.x * 2.0 - 1.0, 1.0 - px.y / uRes.y * 2.0, 0.0, 1.0);
    vUV = c;
}
"#;

const TEXT_FS: &str = r#"#version 330 core
uniform sampler2D uTex;
uniform float uBold;  // утолщение, px
in vec2 vUV;
out vec4 o;
void main() { o = vec4(1.0e4, texture(uTex, vUV).r - uBold, 0.0, 1.0); }
"#;

const DOWN_FS: &str = r#"#version 330 core
uniform sampler2D uSrc;
uniform vec2 uTexel;
in vec2 vUV;
out vec4 o;
vec2 s(vec2 off) { return texture(uSrc, vUV + off * uTexel).rg; }
void main() {
    vec2 a = s(vec2(-2, 2)), b = s(vec2(0, 2)), c = s(vec2(2, 2));
    vec2 d = s(vec2(-2, 0)), e = s(vec2(0, 0)), f = s(vec2(2, 0));
    vec2 g = s(vec2(-2, -2)), h = s(vec2(0, -2)), i = s(vec2(2, -2));
    vec2 j = s(vec2(-1, 1)), k = s(vec2(1, 1)), l = s(vec2(-1, -1)), m = s(vec2(1, -1));
    vec2 r = e * 0.125 + (a + c + g + i) * 0.03125 + (b + d + f + h) * 0.0625 + (j + k + l + m) * 0.125;
    o = vec4(r, 0.0, 1.0);
}
"#;

const UP_FS: &str = r#"#version 330 core
uniform sampler2D uSrc;
uniform vec2 uTexel;
in vec2 vUV;
out vec4 o;
vec2 s(vec2 off) { return texture(uSrc, vUV + off * uTexel).rg; }
void main() {
    vec2 r = s(vec2(0, 0)) * 4.0
           + (s(vec2(-1, 0)) + s(vec2(1, 0)) + s(vec2(0, -1)) + s(vec2(0, 1))) * 2.0
           + s(vec2(-1, -1)) + s(vec2(1, -1)) + s(vec2(-1, 1)) + s(vec2(1, 1));
    o = vec4(r / 16.0, 0.0, 1.0);
}
"#;

const COMPOSITE_FS: &str = r#"#version 330 core
uniform sampler2D uScene, uBloom;
uniform vec3 uBase, uHead, uBg;
uniform float uBloomS, uExposure, uVignette, uRefl, uFloor, uTime;
uniform vec4 uMon[8];  // x, y, w, h (слева сверху)
uniform int uNMon;
uniform vec2 uRes;
in vec2 vUV;
out vec4 o;
vec2 light(vec2 uv) { return texture(uScene, uv).rg + uBloomS * texture(uBloom, uv).rg; }
void main() {
    vec2 p = vec2(gl_FragCoord.x, uRes.y - gl_FragCoord.y);
    vec2 c = light(vUV);
    float vig = 0.0;
    for (int i = 0; i < uNMon; i++) {
        vec4 m = uMon[i];
        if (p.x < m.x || p.y < m.y || p.x >= m.x + m.z || p.y >= m.y + m.w) continue;
        vec2 q = (p - m.xy) / m.zw - 0.5;
        vig = smoothstep(0.35, 0.85, length(q * vec2(1.0, 0.85)));
        float fy = m.y + m.w * (1.0 - uFloor);
        if (uRefl > 0.0 && p.y > fy) {
            float t = (p.y - fy) / (m.y + m.w - fy);
            float ry = 2.0 * fy - p.y;
            float rx = p.x + sin(p.y * 0.09 + uTime * 1.6) * (1.0 + 4.0 * t) + sin(p.y * 0.031 - uTime) * 2.0 * t;
            vec2 ruv = vec2(rx, uRes.y - ry) / uRes;
            c += light(ruv) * uRefl * 0.55 * pow(1.0 - t, 1.6);
        }
    }
    // тонмаппинг по яркости, а не по каналам — оттенок не уходит в «мятный»
    float r = 1.0 - exp(-c.r * uExposure);
    float w = 1.0 - exp(-c.g * uExposure);
    float over = max(c.r * uExposure - 3.0, 0.0);      // сильный пересвет слегка белеет
    vec3 col = r * uBase + w * uHead + uHead * min(over * 0.04, 0.3);
    col *= 1.0 - uVignette * vig;
    o = vec4(col + uBg, 1.0);
}
"#;

struct Layer {
    scene: Scene,
    mon: usize,
    x0: f32,
    y0: f32,
    cw: f32,
    ch: f32,
    inten: f32,
    near: bool,  // ближний слой: обтекание, геометрия часов
    inner: bool, // мелкие символы внутри цифр
}

/// Поле расстояний надписи: текстура на весь квадрат (строка + запас pad со всех сторон).
struct TextTex {
    tex: glow::Texture,
    w: f32,
    h: f32,
    used: bool,
}

/// Точное евклидово преобразование расстояний (Felzenszwalb & Huttenlocher):
/// квадрат расстояния до ближайшего пикселя, у которого ink == to.
fn edt(ink: &[bool], w: usize, h: usize, to: bool) -> Vec<f64> {
    const INF: f64 = 1e20;
    fn pass(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
        let mut k = 0;
        (v[0], z[0], z[1]) = (0, -INF, INF);
        for q in 1..f.len() {
            let mut s;
            loop {
                let p = v[k];
                s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
                if s > z[k] || k == 0 {
                    break;
                }
                k -= 1;
            }
            k += 1;
            (v[k], z[k], z[k + 1]) = (q, s, INF);
        }
        k = 0;
        for (q, dq) in d.iter_mut().enumerate() {
            while z[k + 1] < q as f64 {
                k += 1;
            }
            *dq = (q as f64 - v[k] as f64).powi(2) + f[v[k]];
        }
    }
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0; n], vec![0.0; n + 1]);
    let mut g: Vec<f64> = ink.iter().map(|&i| if i == to { 0.0 } else { INF }).collect();
    for x in 0..w {
        for y in 0..h {
            f[y] = g[y * w + x];
        }
        pass(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&g[y * w..(y + 1) * w]);
        pass(&f[..w], &mut d[..w], &mut v, &mut z);
        g[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
    g
}

/// Ширина строки, px (с разрядкой track).
fn text_width(font: &fontdue::Font, text: &str, size: f32, track: f32) -> f32 {
    let n = text.chars().count() as f32;
    text.chars().map(|c| font.metrics(c, size).advance_width).sum::<f32>() + track * (n - 1.0).max(0.0)
}

/// Поле расстояний строки, px экрана (< 0 внутри): (значения, ширина, высота).
/// Растеризация с запасом SS по разрешению — край точнее пикселя.
fn text_sdf(font: &fontdue::Font, text: &str, size: f32, track: f32, pad: usize) -> Option<(Vec<f32>, usize, usize)> {
    const SS: usize = 2;
    let px = size * SS as f32;
    let lm = font.horizontal_line_metrics(px)?;
    let p = pad * SS;
    let w = (text_width(font, text, px, track * SS as f32).ceil() as usize + 2 * p).div_ceil(SS) * SS;
    let h = ((lm.ascent - lm.descent).ceil() as usize + 2 * p).div_ceil(SS) * SS;
    let mut ink = vec![false; w * h];
    let (mut pen, base) = (p as f32, p as f32 + lm.ascent);
    for c in text.chars() {
        let (m, bmp) = font.rasterize(c, px);
        let x0 = (pen + m.xmin as f32).round() as i64;
        let y0 = (base - m.ymin as f32 - m.height as f32).round() as i64;
        for y in 0..m.height {
            for x in 0..m.width {
                let (xx, yy) = (x0 + x as i64, y0 + y as i64);
                if bmp[y * m.width + x] >= 128 && (0..w as i64).contains(&xx) && (0..h as i64).contains(&yy) {
                    ink[yy as usize * w + xx as usize] = true;
                }
            }
        }
        pen += m.advance_width + track * SS as f32;
    }
    let (outside, inside) = (edt(&ink, w, h, true), edt(&ink, w, h, false));
    let (ow, oh) = (w / SS, h / SS);
    let mut sdf = vec![0f32; ow * oh];
    for y in 0..oh {
        for x in 0..ow {
            let i = (y * SS + SS / 2) * w + x * SS + SS / 2;
            let d = if ink[i] { 0.5 - inside[i].sqrt() } else { outside[i].sqrt() - 0.5 };
            sdf[y * ow + x] = d as f32 / SS as f32;
        }
    }
    Some((sdf, ow, oh))
}

struct Target {
    tex: glow::Texture,
    fbo: glow::Framebuffer,
    w: i32,
    h: i32,
}

pub struct Gfx {
    gl: glow::Context,
    w: i32,
    h: i32,
    mons: Vec<Monitor>,
    c: Config,
    vao_inst: glow::VertexArray,
    vao_empty: glow::VertexArray,
    vbo: glow::Buffer,
    atlas: glow::Texture,
    atlas_cols: usize,
    glyph_idx: HashMap<char, usize>,
    /// шрифт надписей (погода, дата) и их поля расстояний по (строка, размер)
    font: fontdue::Font,
    texts: HashMap<(String, u32), TextTex>,
    info_key: String,
    p_text: glow::Program,
    /// надписи: шаг объёма, число шагов, полуширина штриха, радиус ореола (px)
    text_ext: (f32, f32),
    text_steps: i32,
    text_stroke: f32,
    text_glow: f32,
    solid: usize,
    p_glyph: glow::Program,
    p_sdf: glow::Program,
    p_digit: glow::Program,
    p_down: glow::Program,
    p_up: glow::Program,
    p_comp: glow::Program,
    sdf: Target,
    scene: Target,
    mips: Vec<Target>,
    layers: Vec<Layer>,
    clock_key: String,
    stroke_px: f32,
    ext: (f32, f32),
    steps: i32,
    time: f32,
    inst: Vec<f32>,
}

unsafe fn program(gl: &glow::Context, vs: &str, fs: &str) -> Result<glow::Program, String> {
    unsafe {
        let p = gl.create_program()?;
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let s = gl.create_shader(kind)?;
            gl.shader_source(s, src);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                return Err(format!("шейдер: {}", gl.get_shader_info_log(s)));
            }
            gl.attach_shader(p, s);
        }
        gl.link_program(p);
        if !gl.get_program_link_status(p) {
            return Err(format!("программа: {}", gl.get_program_info_log(p)));
        }
        Ok(p)
    }
}

unsafe fn target(gl: &glow::Context, w: i32, h: i32, internal: u32, format: u32) -> Result<Target, String> {
    unsafe {
        let tex = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, w, h, 0, format, glow::FLOAT, glow::PixelUnpackData::Slice(None));
        for (p, v) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
        }
        let fbo = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            return Err("framebuffer не создан".into());
        }
        Ok(Target { tex, fbo, w, h })
    }
}

/// Атлас глифов: квадратная сетка плиток TILE×TILE, последняя плитка — сплошная (для рамок).
/// Атлас: (пиксели R8, плиток в ряду, символ → номер плитки, номер сплошной плитки).
type Atlas = (Vec<u8>, usize, HashMap<char, usize>, usize);

fn build_atlas(font_bytes: &[u8], chars: &[char]) -> Result<Atlas, String> {
    let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default()).map_err(|e| format!("шрифт: {e}"))?;
    let n = chars.len() + 1;
    let cols = (n as f32).sqrt().ceil() as usize;
    let size = cols * TILE;
    let mut img = vec![0u8; size * size];
    let px = TILE as f32 * 0.78;
    let lm = font.horizontal_line_metrics(px).ok_or("шрифт без горизонтальных метрик")?;
    let baseline = (TILE as f32 - (lm.ascent - lm.descent)) / 2.0 + lm.ascent;
    let mut idx = HashMap::new();
    for (i, &c) in chars.iter().enumerate() {
        let (m, bmp) = font.rasterize(c, px);
        let (tx, ty) = ((i % cols) * TILE, (i / cols) * TILE);
        let x0 = (TILE as i32 - m.width as i32) / 2;
        let y0 = baseline as i32 - (m.height as i32 + m.ymin);
        for y in 0..m.height {
            for x in 0..m.width {
                let (ax, ay) = (x0 + x as i32, y0 + y as i32);
                if (0..TILE as i32).contains(&ax) && (0..TILE as i32).contains(&ay) {
                    img[(ty + ay as usize) * size + tx + ax as usize] = bmp[y * m.width + x];
                }
            }
        }
        idx.insert(c, i);
    }
    let solid = chars.len();
    let (tx, ty) = ((solid % cols) * TILE, (solid / cols) * TILE);
    for y in 4..TILE - 4 {
        img[(ty + y) * size + tx + 4..(ty + y) * size + tx + TILE - 4].fill(255);
    }
    Ok((img, cols, idx, solid))
}

impl Gfx {
    pub fn new(gl: glow::Context, c: &Config, w: i32, h: i32, mons: Vec<Monitor>, seed: u64) -> Result<Gfx, String> {
        let font_bytes = if c.graphics.font.is_empty() {
            BUILTIN_FONT.to_vec()
        } else {
            std::fs::read(&c.graphics.font).map_err(|e| format!("{}: {e}", c.graphics.font))?
        };
        let mut chars = crate::scene::charset(&c.rain.charset, false);
        chars.extend((0x20u8..0x7f).map(|b| b as char));
        chars.sort();
        chars.dedup();
        let (img, atlas_cols, glyph_idx, solid) = build_atlas(&font_bytes, &chars)?;
        // надписи — шрифтом в тон цифрам: к угловатым — свой, к округлым — общий
        let text_font = if c.digits.style == crate::config::DigitStyle::Angular { ANGULAR_TEXT_FONT } else { font_bytes.as_slice() };
        let font = fontdue::Font::from_bytes(text_font, fontdue::FontSettings::default()).map_err(|e| format!("шрифт: {e}"))?;
        unsafe {
            let vs_sdf = format!("#version 330 core\n{SDF_COMMON}");
            let p_glyph = program(&gl, GLYPH_VS, &format!("{vs_sdf}{GLYPH_FS}"))?;
            let p_sdf = program(&gl, FULLSCREEN_VS, SDF_FS)?;
            let p_digit = program(&gl, FULLSCREEN_VS, &format!("{vs_sdf}{DIGIT_FS}"))?;
            let p_down = program(&gl, FULLSCREEN_VS, DOWN_FS)?;
            let p_up = program(&gl, FULLSCREEN_VS, UP_FS)?;
            let p_comp = program(&gl, FULLSCREEN_VS, COMPOSITE_FS)?;
            let p_text = program(&gl, TEXT_VS, TEXT_FS)?;

            let atlas = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            let side = (atlas_cols * TILE) as i32;
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::R8 as i32, side, side, 0, glow::RED, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&img)));
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);

            let vbo = gl.create_buffer()?;
            let vao_inst = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao_inst));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            for a in 0..2u32 {
                gl.enable_vertex_attrib_array(a);
                gl.vertex_attrib_pointer_f32(a, 4, glow::FLOAT, false, 32, a as i32 * 16);
                gl.vertex_attrib_divisor(a, 1);
            }
            let vao_empty = gl.create_vertex_array()?;

            let sdf = target(&gl, w, h, glow::RG16F, glow::RG)?;
            let scene = target(&gl, w, h, glow::RGBA16F, glow::RGBA)?;
            let mut mips = Vec::new();
            let (mut mw, mut mh) = (w / 2, h / 2);
            while mips.len() < 6 && mw >= 8 && mh >= 8 {
                mips.push(target(&gl, mw, mh, glow::RGBA16F, glow::RGBA)?);
                mw /= 2;
                mh /= 2;
            }

            let mut g = Gfx {
                gl,
                w,
                h,
                mons,
                c: c.clone(),
                vao_inst,
                vao_empty,
                vbo,
                atlas,
                atlas_cols,
                glyph_idx,
                font,
                texts: HashMap::new(),
                info_key: String::new(),
                p_text,
                text_ext: (0.0, 0.0),
                text_steps: 0,
                text_stroke: 1.0,
                text_glow: 1.0,
                solid,
                p_glyph,
                p_sdf,
                p_digit,
                p_down,
                p_up,
                p_comp,
                sdf,
                scene,
                mips,
                layers: Vec::new(),
                clock_key: String::new(),
                stroke_px: 1.0,
                ext: (0.0, 0.0),
                steps: 0,
                time: 0.0,
                inst: Vec::new(),
            };
            g.make_layers(seed);
            Ok(g)
        }
    }

    fn rain_height(&self, m: &Monitor) -> f32 {
        let g = &self.c.graphics;
        m.h as f32 * if g.reflection > 0.0 { 1.0 - g.floor_height } else { 1.0 }
    }

    fn make_layers(&mut self, seed: u64) {
        const SCALE: [f32; 4] = [1.0, 0.7, 0.5, 0.36];
        const INTEN: [f32; 4] = [1.0, 0.5, 0.28, 0.16];
        const INNER_SCALE: f32 = 0.6;
        for (mi, m) in self.mons.clone().iter().enumerate() {
            let rain_h = self.rain_height(m);
            // (масштаб, яркость, ближний, внутренний); последний — мелкий слой внутри цифр
            let mut kinds: Vec<(f32, f32, bool, bool)> =
                (0..self.c.graphics.layers as usize).rev().map(|li| (SCALE[li], INTEN[li], li == 0, false)).collect();
            kinds.push((INNER_SCALE, 1.0, false, true));
            for (n, &(scale, inten, near, inner)) in kinds.iter().enumerate() {
                let ch = self.c.graphics.glyph_size as f32 * scale;
                let cw = ch * 0.62;
                let mut lc = self.c.clone();
                lc.console.cell_aspect = ch / cw;
                lc.clock.thickness *= THICK;
                lc.rain.speed *= scale.sqrt();
                lc.rain.density *= if near || inner { 1.0 } else { 1.0 + 0.5 * (1.0 / scale - 1.0) };
                if inner {
                    lc.rain.density = 0.0;
                }
                let cols = (m.w as f32 / cw).ceil() as usize;
                let rows = (rain_h / ch).ceil() as usize;
                let mut scene = Scene::new(&lc, cols, rows, false, seed ^ ((mi * 8 + n) as u64 * 0x9E37_79B9));
                if !near && !inner {
                    scene.set_text("", false);
                }
                self.layers.push(Layer { scene, mon: mi, x0: m.x as f32, y0: m.y as f32, cw, ch, inten, near, inner });
            }
        }
    }

    unsafe fn u(&self, p: glow::Program, name: &str) -> Option<glow::UniformLocation> {
        unsafe { self.gl.get_uniform_location(p, name) }
    }

    /// Перерисовка поля расстояний цифр (раз в секунду — смена времени/мигание).
    unsafe fn render_sdf(&mut self, info: (Option<&[String; 3]>, &str)) {
        unsafe {
            let gl = &self.gl;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.sdf.fbo));
            gl.viewport(0, 0, self.w, self.h);
            gl.disable(glow::BLEND);
            gl.clear_color(1.0e4, 1.0e4, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(self.p_sdf));
            gl.bind_vertex_array(Some(self.vao_empty));
            gl.enable(glow::SCISSOR_TEST);
            let (vx, vy) = self.c.digits.depth_direction.vector();
            for l in self.layers.iter().filter(|l| l.near) {
                let Some((text, colon, s, ox, oy)) = l.scene.layout.clone() else { continue };
                let t = Text::new(&text, colon, self.c.clock.thickness * THICK, self.c.clock.spacing, self.c.digits.style);
                let prims: Vec<f32> = t.gpu_prims().into_iter().take(64).flatten().collect();
                let scale = s * l.cw;
                self.stroke_px = t.stroke() * scale;
                // объём: как в текстовой сцене, depth × 0.8 ширины клетки
                let total = self.c.digits.depth as f32 * 0.8 * l.cw;
                self.steps = (self.c.digits.depth * 4) as i32;
                if self.steps > 0 {
                    self.ext = (vx * total / self.steps as f32, -vy * total / self.steps as f32);
                }
                let m = self.mons[l.mon];
                gl.scissor(m.x, self.h - m.y - m.h, m.w, m.h);
                gl.uniform_4_f32_slice(self.u(self.p_sdf, "uP").as_ref(), &prims);
                gl.uniform_1_i32(self.u(self.p_sdf, "uN").as_ref(), (prims.len() / 8) as i32);
                gl.uniform_2_f32(self.u(self.p_sdf, "uOff").as_ref(), l.x0 + ox * l.cw, l.y0 + oy * l.cw);
                gl.uniform_1_f32(self.u(self.p_sdf, "uScale").as_ref(), scale);
                gl.uniform_1_f32(self.u(self.p_sdf, "uStroke").as_ref(), t.stroke());
                gl.uniform_1_f32(self.u(self.p_sdf, "uOct").as_ref(), if t.oct { 1.0 } else { 0.0 });
                gl.uniform_1_f32(self.u(self.p_sdf, "uH").as_ref(), self.h as f32);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }
            gl.disable(glow::SCISSOR_TEST);
            let items = self.info_items(info);
            let gl = &self.gl;
            gl.use_program(Some(self.p_text));
            gl.uniform_2_f32(self.u(self.p_text, "uRes").as_ref(), self.w as f32, self.h as f32);
            gl.uniform_1_i32(self.u(self.p_text, "uTex").as_ref(), 0);
            gl.active_texture(glow::TEXTURE0);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::MIN);
            for (rect, tex, bold) in items {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.uniform_4_f32_slice(self.u(self.p_text, "uRect").as_ref(), &rect);
                gl.uniform_1_f32(self.u(self.p_text, "uBold").as_ref(), bold);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }
            gl.blend_equation(glow::FUNC_ADD);
            gl.disable(glow::BLEND);
        }
    }

    unsafe fn sdf_uniforms(&self, p: glow::Program) {
        unsafe {
            let gl = &self.gl;
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.sdf.tex));
            gl.uniform_1_i32(self.u(p, "uSdf").as_ref(), 1);
            gl.uniform_2_f32(self.u(p, "uRes").as_ref(), self.w as f32, self.h as f32);
            gl.uniform_2_f32(self.u(p, "uExt").as_ref(), self.ext.0, self.ext.1);
            gl.uniform_1_i32(self.u(p, "uSteps").as_ref(), self.steps);
            gl.active_texture(glow::TEXTURE0);
        }
    }

    fn push(inst: &mut Vec<f32>, rect: [f32; 4], glyph: usize, inten: f32, white: f32, flags: f32) {
        inst.extend_from_slice(&rect);
        inst.extend_from_slice(&[glyph as f32, inten, white, flags]);
    }

    /// Габариты часов ближнего слоя, px окна: (слева, сверху, справа, снизу).
    fn clock_box(l: &Layer) -> (f32, f32, f32, f32) {
        let s = &l.scene;
        let (x, y) = (|c: usize| l.x0 + c as f32 * l.cw, |r: usize| l.y0 + r as f32 * l.ch);
        (x(s.clock_left), y(s.clock_top), x(s.clock_right + 1), y(s.clock_bottom + 1))
    }

    /// Размер шрифта погоды и даты для часов высотой h, px; не больше, чем
    /// помещается в промежутки над часами (above) и под ними (below).
    fn info_sizes(&self, h: f32, above: f32, below: f32) -> (f32, f32) {
        let fit = |gap: f32| gap / 2.2;
        let w = (h * 0.11).max(self.c.graphics.glyph_size as f32);
        (w.min(fit(above)), (w * 1.25).min(fit(below)))
    }

    /// Середины промежутков: между верхом экрана и часами, между часами и низом экрана.
    fn info_rows(m: &Monitor, top: f32, bottom: f32) -> (f32, f32) {
        ((m.y as f32 + top) / 2.0, (bottom + (m.y + m.h) as f32) / 2.0)
    }

    /// Вылет объёма (выдавливание) цифр, px — тот же у надписей.
    fn extrusion(&self) -> f32 {
        self.c.digits.depth as f32 * 0.8 * self.c.graphics.glyph_size as f32 * 0.62
    }

    /// Квадрат надписи на экране и её текстура. x — точка выравнивания
    /// (align: 0 — левый край, 0.5 — центр, 1 — правый край), y — середина строки.
    unsafe fn place(&mut self, text: &str, size: f32, x: f32, align: f32, y: f32) -> Option<([f32; 4], glow::Texture)> {
        // запас вокруг строки: объём + ореол, иначе их обрежет край квадрата
        let ext = self.text_steps as f32 * self.text_ext.0.hypot(self.text_ext.1);
        let pad = (ext + 3.0 * self.text_glow + 4.0).ceil() as usize;
        let key = (text.to_string(), size.to_bits());
        if !self.texts.contains_key(&key) {
            let (sdf, w, h) = text_sdf(&self.font, text, size, size * 0.08, pad)?;
            unsafe {
                let gl = &self.gl;
                let tex = gl.create_texture().ok()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                let bytes = std::slice::from_raw_parts(sdf.as_ptr() as *const u8, sdf.len() * 4);
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::R32F as i32, w as i32, h as i32, 0, glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(bytes)));
                for (p, v) in [
                    (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                    (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
                }
                self.texts.insert(key.clone(), TextTex { tex, w: w as f32, h: h as f32, used: false });
            }
        }
        let t = self.texts.get_mut(&key)?;
        t.used = true;
        let pad = pad as f32;
        let ink_w = t.w - 2.0 * pad;
        Some(([x - align * ink_w - pad, y - t.h / 2.0, t.w, t.h], t.tex))
    }

    /// Надписи всех мониторов: погода над часами, дата под ними — по центру часов,
    /// посередине между часами и краем экрана.
    unsafe fn info_items(&mut self, info: (Option<&[String; 3]>, &str)) -> Vec<([f32; 4], glow::Texture, f32)> {
        for t in self.texts.values_mut() {
            t.used = false;
        }
        let boxes: Vec<(Monitor, (f32, f32, f32, f32))> =
            self.layers.iter().filter(|l| l.near).map(|l| (self.mons[l.mon], Self::clock_box(l))).collect();
        let mut out = Vec::new();
        for (m, (left, top, right, bottom)) in boxes {
            let (ws, ds) = self.info_sizes(bottom - top, top - m.y as f32, (m.y + m.h) as f32 - bottom);
            let (wy, dy) = Self::info_rows(&m, top, bottom);
            // объём, штрих и ореол надписей — в той же пропорции к размеру, что у цифр
            let k = ds * 0.72 / (bottom - top);
            let len = self.extrusion() * k;
            let (vx, vy) = self.c.digits.depth_direction.vector();
            let n = (vx * vx + vy * vy).sqrt();
            self.text_steps = if self.c.digits.depth > 0 { (len / 1.0).ceil().max(2.0) as i32 } else { 0 };
            let step = len / self.text_steps.max(1) as f32;
            self.text_ext = (vx / n * step, -vy / n * step);
            self.text_stroke = ds * 0.085;
            self.text_glow = (self.c.digits.glow as f32 * 0.8 * self.c.graphics.glyph_size as f32 * 0.62 * k * 3.0).max(2.0);
            let mut items: Vec<(String, f32, f32, f32, f32)> = Vec::new(); // текст, размер, x, align, y
            if let Some(w) = info.0
                && ws >= 8.0
            {
                items.push((crate::info::weather_line(w), ws, (left + right) / 2.0, 0.5, wy));
            }
            if ds >= 8.0 {
                items.push((info.1.to_string(), ds, (left + right) / 2.0, 0.5, dy));
            }
            for (text, size, x, align, y) in items.into_iter().filter(|i| !i.0.is_empty()) {
                if let Some((rect, tex)) = unsafe { self.place(&text, size, x, align, y) } {
                    out.push((rect, tex, size * 0.035));
                }
            }
        }
        let gl = &self.gl;
        self.texts.retain(|_, t| {
            if !t.used {
                unsafe { gl.delete_texture(t.tex) };
            }
            t.used
        });
        out
    }

    unsafe fn draw_inst(&self, data: &[f32]) {
        if data.is_empty() {
            return;
        }
        unsafe {
            let gl = &self.gl;
            gl.use_program(Some(self.p_glyph));
            self.sdf_uniforms(self.p_glyph);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas));
            gl.uniform_1_i32(self.u(self.p_glyph, "uAtlas").as_ref(), 0);
            gl.uniform_1_f32(self.u(self.p_glyph, "uCols").as_ref(), self.atlas_cols as f32);
            gl.bind_vertex_array(Some(self.vao_inst));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4);
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, (data.len() / 8) as i32);
        }
    }

    unsafe fn pass(&self, p: glow::Program, dst: Option<&Target>, src: glow::Texture, src_w: i32, src_h: i32) {
        unsafe {
            let gl = &self.gl;
            match dst {
                Some(t) => {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
                    gl.viewport(0, 0, t.w, t.h);
                }
                None => {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    gl.viewport(0, 0, self.w, self.h);
                }
            }
            gl.use_program(Some(p));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src));
            gl.uniform_1_i32(self.u(p, "uSrc").as_ref(), 0);
            gl.uniform_2_f32(self.u(p, "uTexel").as_ref(), 1.0 / src_w as f32, 1.0 / src_h as f32);
            gl.bind_vertex_array(Some(self.vao_empty));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    /// Кадр. text/colon — время; ui — содержимое поля пароля; info — (погода, дата);
    /// colors — [основной, головы, фон].
    pub fn frame(
        &mut self,
        dt: f32,
        text: &str,
        colon: bool,
        ui: Option<(String, String)>,
        info: (Option<&[String; 3]>, &str),
        colors: [[f32; 3]; 3],
    ) {
        self.time += dt;
        let mut changed = false;
        for l in self.layers.iter_mut().filter(|l| l.near || l.inner) {
            let before = l.scene.layout.clone();
            l.scene.set_text(text, colon);
            changed |= l.near && before != l.scene.layout;
        }
        // поле пароля встаёт на место даты
        let info = (info.0, if ui.is_some() { "" } else { info.1 });
        let info_key = format!("{:?}{}", info.0, info.1);
        unsafe {
            if changed || self.clock_key.is_empty() || info_key != self.info_key {
                self.clock_key = format!("{text}{colon}");
                self.info_key = info_key;
                self.render_sdf(info);
            }
        }
        // символы дождя
        let mirror = if self.c.graphics.mirror_glyphs { 4.0 } else { 0.0 };
        let through = self.c.digits.flow == Flow::Through;
        let mut inst = std::mem::take(&mut self.inst);
        inst.clear();
        for l in &mut self.layers {
            l.scene.update(dt);
            let (x0, y0, cw, ch, k) = (l.x0, l.y0, l.cw, l.ch, l.inten);
            let q = ch * 1.05;
            let idx = &self.glyph_idx;
            // внутренний слой — только внутри цифр (ярче), остальные — только снаружи
            let k = if l.inner { k * 2.6 } else { k };
            l.scene.glyphs(!l.inner, l.inner, |x, y, c, v, head, inside| {
                let Some(&g) = idx.get(&c) else { return };
                let (px, py) = (x0 + (x as f32 + 0.5) * cw, y0 + (y as f32 + 0.5) * ch);
                let mask = if inside { 1.0 } else if through { 0.0 } else { 2.0 };
                let (i, w) = if head { (1.0, 1.0) } else { (v, 0.0) };
                Self::push(&mut inst, [px - q / 2.0, py - q / 2.0, q, q], g, i * k, w, mask + mirror);
            });
        }
        let (mut occ, mut ui_add) = (Vec::new(), Vec::new());
        // поле пароля: подложка (затемняет) + рамка и текст (светят)
        if let Some((line, status)) = &ui {
            let ch = self.c.graphics.glyph_size as f32 * 1.3;
            let cw = ch * 0.62;
            let cols = 34.max(line.chars().count() + 4);
            for l in self.layers.iter().filter(|l| l.near) {
                let m = self.mons[l.mon];
                let bw = cols as f32 * cw;
                let bx = m.x as f32 + (m.w as f32 - bw) / 2.0;
                let (_, top, _, bottom) = Self::clock_box(l);
                let bh = ch * 2.2;
                // на месте даты: середина между часами и низом экрана
                let by = (Self::info_rows(&m, top, bottom).1 - bh / 2.0 - ch * 0.45).min(m.y as f32 + m.h as f32 - ch * 5.0);
                Self::push(&mut occ, [bx - cw, by - ch * 0.5, bw + 2.0 * cw, bh + ch * 2.0], self.solid, 0.88, 0.0, 0.0);
                let t = 2.0;
                for r in [[bx, by, bw, t], [bx, by + bh - t, bw, t], [bx, by, t, bh], [bx + bw - t, by, t, bh]] {
                    Self::push(&mut ui_add, r, self.solid, 0.9, 0.0, 0.0);
                }
                let q = ch * 1.05;
                let ty = by + bh / 2.0;
                for (i, c) in line.chars().enumerate() {
                    if let Some(&g) = self.glyph_idx.get(&c) {
                        let px = bx + cw * (1.5 + i as f32);
                        Self::push(&mut ui_add, [px - q / 2.0, ty - q / 2.0, q, q], g, 1.0, 0.25, 0.0);
                    }
                }
                let sx = bx + (bw - status.chars().count() as f32 * cw) / 2.0;
                for (i, c) in status.chars().enumerate() {
                    if let Some(&g) = self.glyph_idx.get(&c) {
                        let px = sx + cw * (i as f32 + 0.5);
                        Self::push(&mut ui_add, [px - q / 2.0, by + bh + ch * 0.9 - q / 2.0, q, q], g, 0.9, 0.0, 0.0);
                    }
                }
            }
        }

        unsafe {
            let gl = &self.gl;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.scene.fbo));
            gl.viewport(0, 0, self.w, self.h);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE);
            self.draw_inst(&inst);
            // цифры
            let g = &self.c.graphics;
            let d = &self.c.digits;
            gl.use_program(Some(self.p_digit));
            self.sdf_uniforms(self.p_digit);
            for (n, v) in [
                ("uStrokePx", self.stroke_px),
                ("uBevel", g.bevel),
                ("uSpec", g.specular),
                ("uRim", d.rim),
                ("uGlowR", (d.glow as f32 * 0.8 * self.c.graphics.glyph_size as f32 * 0.62).max(1.0)),
                ("uGlowS", d.glow_strength * (d.glow > 0) as u8 as f32),
                ("uBright", d.brightness * 2.0),
                ("uStrokeT", self.text_stroke),
                ("uGlowT", self.text_glow),
            ] {
                gl.uniform_1_f32(self.u(self.p_digit, n).as_ref(), v);
            }
            gl.uniform_2_f32(self.u(self.p_digit, "uExtT").as_ref(), self.text_ext.0, self.text_ext.1);
            gl.uniform_1_i32(self.u(self.p_digit, "uStepsT").as_ref(), self.text_steps);
            gl.bind_vertex_array(Some(self.vao_empty));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            // интерфейс
            if !occ.is_empty() {
                gl.blend_func(glow::ZERO, glow::ONE_MINUS_SRC_ALPHA);
                self.draw_inst(&occ);
                gl.blend_func(glow::ONE, glow::ONE);
                self.draw_inst(&ui_add);
            }
            gl.disable(glow::BLEND);

            // bloom: уменьшение, затем увеличение со сложением
            if !self.mips.is_empty() {
                self.pass(self.p_down, Some(&self.mips[0]), self.scene.tex, self.w, self.h);
                for i in 1..self.mips.len() {
                    let s = &self.mips[i - 1];
                    self.pass(self.p_down, Some(&self.mips[i]), s.tex, s.w, s.h);
                }
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE);
                for i in (0..self.mips.len() - 1).rev() {
                    let s = &self.mips[i + 1];
                    self.pass(self.p_up, Some(&self.mips[i]), s.tex, s.w, s.h);
                }
                gl.disable(glow::BLEND);
            }

            // композит
            let p = self.p_comp;
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, self.w, self.h);
            gl.use_program(Some(p));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.scene.tex));
            gl.uniform_1_i32(self.u(p, "uScene").as_ref(), 0);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, self.mips.first().map(|m| m.tex));
            gl.uniform_1_i32(self.u(p, "uBloom").as_ref(), 1);
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_3_f32_slice(self.u(p, "uBase").as_ref(), &colors[0]);
            gl.uniform_3_f32_slice(self.u(p, "uHead").as_ref(), &colors[1]);
            gl.uniform_3_f32_slice(self.u(p, "uBg").as_ref(), &colors[2]);
            for (n, v) in [
                ("uBloomS", g.bloom * 0.55),
                ("uExposure", g.exposure * 1.6),
                ("uVignette", g.vignette),
                ("uRefl", g.reflection),
                ("uFloor", g.floor_height),
                ("uTime", self.time),
            ] {
                gl.uniform_1_f32(self.u(p, n).as_ref(), v);
            }
            let mons: Vec<f32> = self.mons.iter().take(8).flat_map(|m| [m.x as f32, m.y as f32, m.w as f32, m.h as f32]).collect();
            gl.uniform_4_f32_slice(self.u(p, "uMon").as_ref(), &mons);
            gl.uniform_1_i32(self.u(p, "uNMon").as_ref(), (mons.len() / 4) as i32);
            gl.uniform_2_f32(self.u(p, "uRes").as_ref(), self.w as f32, self.h as f32);
            gl.bind_vertex_array(Some(self.vao_empty));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        self.inst = inst;
    }
}
