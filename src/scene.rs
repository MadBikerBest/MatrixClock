//! Сцена: дождь символов + объёмные цифры. Работает в «уровнях яркости» 0..255,
//! цвета подставляет вывод (term.rs) — поэтому сцена одна и для full, и для lite.

use crate::config::{Config, Flow};
use crate::shape::{GLYPH_H, Text};

#[derive(Clone, Copy, PartialEq, Default)]
pub struct Cell {
    pub ch: char,
    pub fg: u8,
    pub bg: u8,
}

pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// 0.0..1.0
    pub fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

const OUT: u8 = 0;
const FACE: u8 = 1;
const RIM: u8 = 2;
const SIDE: u8 = 3;

struct Drop {
    x: i32,
    y: i32,
    acc: f32,
    speed: f32,
    len: usize,
    trail: Vec<(i32, i32)>, // [0] — голова
    dir: i32,               // направление обтекания: -1/0/1
    stopped: bool,          // голова остановилась, хвост втягивается
    ttl: i32,               // шагов до исчезновения (капли), -1 — без ограничения
}

impl Drop {
    fn new(x: i32, y: i32, speed: f32, len: usize, ttl: i32) -> Drop {
        Drop { x, y, acc: 0.0, speed, len: len.max(1), trail: Vec::with_capacity(len + 1), dir: 0, stopped: false, ttl }
    }
    fn push_head(&mut self) {
        self.trail.insert(0, (self.x, self.y));
        self.trail.truncate(self.len);
    }
}

struct Spark {
    x: i32,
    y: i32,
    life: f32,
}

pub struct Scene {
    pub cols: usize,
    pub rows: usize,
    aspect: f32,
    flow: Flow,
    depth: f32,
    glow: f32,
    fill: f32,
    density: f32,
    speed: f32,
    length: f32,
    c: Config,
    flicker_acc: f32,
    charset: Vec<char>,
    glyphs: Vec<char>,
    region: Vec<u8>,
    shade: Vec<u8>,     // FACE: 0 у края .. 255 в глубине; SIDE: глубина слоя
    glow_lvl: Vec<u8>,  // фон-свечение снаружи
    fill_mask: Vec<bool>,
    top_edges: Vec<(i32, i32)>,
    bottom_edges: Vec<(i32, i32)>,
    pub clock_bottom: usize,
    /// Верхняя строка и крайние колонки часов (с объёмом).
    pub clock_top: usize,
    pub clock_left: usize,
    pub clock_right: usize,
    /// Геометрия часов для графического режима: строка, масштаб и смещение
    /// (в «пикселях» = ширинах клетки) — графика рисует цифры точно там же.
    pub layout: Option<(String, bool, f32, f32, f32)>,
    drops: Vec<Drop>,
    inner: Vec<Drop>,
    drips: Vec<Drop>,
    sparks: Vec<Spark>,
    drip_acc: f32,
    lit_out: Vec<u8>,
    lit_in: Vec<u8>,
    head: Vec<bool>,
    head_in: Vec<bool>,
    text_key: String,
    rng: Rng,
}

pub fn charset(name: &str, ascii_only: bool) -> Vec<char> {
    let ascii: Vec<char> = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ$+-*/=%\"'#&_(),.;:?!\\|{}<>[]^~"
        .chars()
        .collect();
    let set: Vec<char> = match name {
        "matrix" => ('\u{FF66}'..='\u{FF9D}').chain("0123456789Z:.\"=*+-<>|".chars()).collect(),
        "ascii" => ascii.clone(),
        "digits" => "0123456789".chars().collect(),
        "binary" => "01".chars().collect(),
        custom => custom.chars().filter(|c| !c.is_control() && *c != ' ').collect(),
    };
    if ascii_only && set.iter().any(|c| !c.is_ascii()) {
        let only: Vec<char> = set.into_iter().filter(|c| c.is_ascii()).collect();
        return if only.len() >= 2 { only } else { ascii };
    }
    if set.is_empty() { ascii } else { set }
}

impl Scene {
    pub fn new(cfg: &Config, cols: usize, rows: usize, ascii_only: bool, seed: u64) -> Scene {
        let n = cols * rows;
        let mut s = Scene {
            cols,
            rows,
            aspect: cfg.console.cell_aspect,
            flow: cfg.digits.flow,
            depth: cfg.digits.depth as f32,
            glow: cfg.digits.glow as f32,
            fill: cfg.digits.fill,
            density: cfg.rain.density,
            speed: cfg.rain.speed,
            length: cfg.rain.length as f32,
            c: cfg.clone(),
            flicker_acc: 0.0,
            charset: charset(&cfg.rain.charset, ascii_only),
            glyphs: vec![' '; n],
            region: vec![OUT; n],
            shade: vec![0; n],
            glow_lvl: vec![0; n],
            fill_mask: vec![false; n],
            top_edges: Vec::new(),
            bottom_edges: Vec::new(),
            clock_bottom: rows / 2,
            clock_top: rows / 2,
            clock_left: 0,
            clock_right: cols.saturating_sub(1),
            layout: None,
            drops: Vec::new(),
            inner: Vec::new(),
            drips: Vec::new(),
            sparks: Vec::new(),
            drip_acc: 0.0,
            lit_out: vec![0; n],
            lit_in: vec![0; n],
            head: vec![false; n],
            head_in: vec![false; n],
            text_key: String::new(),
            rng: Rng::new(seed),
        };
        for i in 0..n {
            s.glyphs[i] = s.rand_glyph();
            s.fill_mask[i] = s.rng.f() < s.fill;
        }
        // стартовое заполнение экрана дождём
        let target = s.target_drops();
        while s.drops.len() < target {
            let y = s.rng.below(rows * 2) as i32 - rows as i32;
            s.spawn_drop(y);
        }
        s
    }

    fn rand_glyph(&mut self) -> char {
        self.charset[self.rng.below(self.charset.len())]
    }

    fn target_drops(&self) -> usize {
        (self.density * self.cols as f32).round() as usize
    }

    fn spawn_drop(&mut self, y: i32) {
        let x = self.rng.below(self.cols) as i32;
        let (ss, ls) = (self.c.rain.speed_spread, self.c.rain.length_spread);
        let speed = self.speed * (1.0 - ss + 2.0 * ss * self.rng.f());
        let len = (self.length * (1.0 - ls + 2.0 * ls * self.rng.f())).round() as usize;
        self.drops.push(Drop::new(x, y, speed, len, -1));
    }

    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && (x as usize) < self.cols && (y as usize) < self.rows)
            .then(|| y as usize * self.cols + x as usize)
    }

    fn region_at(&self, x: i32, y: i32) -> u8 {
        self.idx(x, y).map_or(OUT, |i| self.region[i])
    }

    fn solid(&self, x: i32, y: i32) -> bool {
        self.region_at(x, y) != OUT
    }

    /// Пересчёт геометрии цифр, если строка времени изменилась.
    pub fn set_text(&mut self, text: &str, colon_visible: bool) {
        let key = format!("{text}{colon_visible}");
        if key == self.text_key {
            return;
        }
        self.text_key = key;
        let k = &self.c.clock;
        let t = Text::new(text, colon_visible, k.thickness, k.spacing, self.c.digits.style);
        let (cols, rows, a) = (self.cols, self.rows, self.aspect);
        // пиксель = ширина клетки; высота клетки = aspect пикселей
        let (vx, vy) = self.c.digits.depth_direction.vector();
        let (vx, vy) = (vx * 0.8, vy * 0.8); // шаг выдавливания, px
        let ext = self.depth;
        let s = (k.size * cols as f32 / t.width).min(k.max_height * rows as f32 * a / GLYPH_H).max(1.0);
        let ox = k.position_x * cols as f32 - (t.width * s + ext * vx) / 2.0;
        let oy = k.position_y * rows as f32 * a - (GLYPH_H * s + ext * vy) / 2.0;
        self.layout = Some((text.to_string(), colon_visible, s, ox, oy));

        // точное SDF лицевой поверхности в центрах клеток (в px)
        let n = cols * rows;
        let mut face = vec![0f32; n];
        for y in 0..rows {
            for x in 0..cols {
                let (px, py) = (x as f32 + 0.5, (y as f32 + 0.5) * a);
                face[y * cols + x] = t.sdf((px - ox) / s, (py - oy) / s) * s;
            }
        }
        // билинейная выборка SDF со смещением (для слоёв объёма)
        let sample = |fx: f32, fy: f32| -> f32 {
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - x0, fy - y0);
            let get = |x: f32, y: f32| {
                if x < 0.0 || y < 0.0 || x >= cols as f32 || y >= rows as f32 {
                    1e3
                } else {
                    face[y as usize * cols + x as usize]
                }
            };
            let top = get(x0, y0) * (1.0 - tx) + get(x0 + 1.0, y0) * tx;
            let bot = get(x0, y0 + 1.0) * (1.0 - tx) + get(x0 + 1.0, y0 + 1.0) * tx;
            top * (1.0 - ty) + bot * ty
        };
        let steps = (self.depth * 2.0) as usize;
        let stroke_px = t.stroke() * s;
        self.clock_bottom = 0;
        (self.clock_top, self.clock_left, self.clock_right) = (rows, cols, 0);
        for y in 0..rows {
            for x in 0..cols {
                let i = y * cols + x;
                let d = face[i];
                let mut union = d;
                let mut side_k = 0.0;
                if d >= 0.0 {
                    for k in 1..=steps {
                        let kk = k as f32 * 0.5;
                        let dk = sample(x as f32 - vx * kk, y as f32 - vy * kk / a);
                        if dk < union {
                            union = dk;
                        }
                        if dk < 0.0 && side_k == 0.0 {
                            side_k = kk;
                        }
                    }
                }
                self.region[i] = if d < 0.0 {
                    FACE
                } else if union < 0.0 {
                    SIDE
                } else {
                    OUT
                };
                self.shade[i] = if d < 0.0 {
                    ((-d / stroke_px).clamp(0.0, 1.0) * 255.0) as u8
                } else if union < 0.0 {
                    (side_k / self.depth.max(0.5) * 255.0).min(255.0) as u8
                } else {
                    0
                };
                self.glow_lvl[i] = if union >= 0.0 && union < self.glow {
                    ((1.0 - union / self.glow).powi(2) * 140.0 * self.c.digits.glow_strength) as u8
                } else {
                    0
                };
                if union < 0.0 {
                    self.clock_bottom = self.clock_bottom.max(y);
                    self.clock_top = self.clock_top.min(y);
                    self.clock_left = self.clock_left.min(x);
                    self.clock_right = self.clock_right.max(x);
                }
            }
        }
        // кромка — клетки лица, у которых сосед не лицо
        let mut rim = Vec::new();
        for y in 0..rows as i32 {
            for x in 0..cols as i32 {
                let i = self.idx(x, y).unwrap();
                if self.region[i] != FACE {
                    continue;
                }
                let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| self.region_at(x + dx, y + dy) != FACE);
                if edge {
                    rim.push(i);
                }
            }
        }
        for i in rim {
            self.region[i] = RIM;
        }
        self.top_edges.clear();
        self.bottom_edges.clear();
        for y in 0..rows as i32 {
            for x in 0..cols as i32 {
                let r = self.region_at(x, y);
                if (r == FACE || r == RIM) && !matches!(self.region_at(x, y - 1), FACE | RIM) {
                    self.top_edges.push((x, y));
                }
                if r != OUT && !self.solid(x, y + 1) {
                    self.bottom_edges.push((x, y));
                }
            }
        }
        let mut inner = std::mem::take(&mut self.inner);
        inner.retain(|d| matches!(self.region_at(d.x, d.y), FACE | RIM));
        self.inner = inner;
    }

    /// Один шаг внешней струи с учётом режима обтекания.
    fn step_outer(&mut self, k: usize) {
        let flow = self.flow;
        let (x, y) = (self.drops[k].x, self.drops[k].y);
        if self.drops[k].stopped {
            self.drops[k].trail.pop();
            return;
        }
        let blocked = self.solid(x, y + 1);
        match flow {
            Flow::Around if blocked => {
                let mut dir = self.drops[k].dir;
                if dir == 0 {
                    // к ближайшему краю препятствия
                    let mut d = 1;
                    dir = loop {
                        if !self.solid(x - d, y + 1) {
                            break -1;
                        }
                        if !self.solid(x + d, y + 1) {
                            break 1;
                        }
                        d += 1;
                        if d > self.cols as i32 {
                            break 1;
                        }
                    };
                }
                let dr = &mut self.drops[k];
                dr.dir = dir;
                if !self.solid(x + dir, y + 1) {
                    let dr = &mut self.drops[k];
                    dr.x += dir;
                    dr.y += 1;
                } else if !self.solid(x + dir, y) {
                    self.drops[k].x += dir;
                } else {
                    self.drops[k].stopped = true; // зажата в «кармане»
                    return;
                }
            }
            Flow::Splash if blocked => {
                self.drops[k].stopped = true;
                for _ in 0..2 + self.rng.below(3) {
                    let sx = x + self.rng.below(5) as i32 - 2;
                    let sy = y - self.rng.below(2) as i32;
                    if !self.solid(sx, sy) {
                        self.sparks.push(Spark { x: sx, y: sy, life: 0.25 + 0.3 * self.rng.f() });
                    }
                }
                return;
            }
            _ => {
                let dr = &mut self.drops[k];
                dr.y += 1;
                if !blocked {
                    dr.dir = 0;
                }
            }
        }
        let g = self.rand_glyph();
        self.drops[k].push_head();
        if let Some(i) = self.idx(self.drops[k].x, self.drops[k].y) {
            self.glyphs[i] = g;
        }
    }

    pub fn update(&mut self, dt: f32) {
        let rows = self.rows as i32;
        // внешний дождь
        for k in 0..self.drops.len() {
            self.drops[k].acc += self.drops[k].speed * dt;
            while self.drops[k].acc >= 1.0 {
                self.drops[k].acc -= 1.0;
                self.step_outer(k);
            }
        }
        let cols = self.cols as i32;
        self.drops.retain(|d| {
            let gone = d.trail.last().is_some_and(|&(_, ty)| ty >= rows) || (d.stopped && d.trail.is_empty());
            !gone && d.x >= 0 && d.x < cols
        });
        let target = self.target_drops();
        while self.drops.len() < target {
            let y = -(self.rng.below(self.rows / 2 + 1) as i32);
            self.spawn_drop(y);
        }

        // дождь внутри цифр
        let inner_target = (self.fill * self.top_edges.len() as f32 * 0.35) as usize;
        while self.inner.len() < inner_target && !self.top_edges.is_empty() {
            let (x, y) = self.top_edges[self.rng.below(self.top_edges.len())];
            let speed = self.speed * (0.4 + 0.5 * self.rng.f());
            let len = (self.length * (0.3 + 0.4 * self.rng.f())).max(1.0) as usize;
            self.inner.push(Drop::new(x, y, speed, len, -1));
        }
        for k in 0..self.inner.len() {
            self.inner[k].acc += self.inner[k].speed * dt;
            while self.inner[k].acc >= 1.0 {
                self.inner[k].acc -= 1.0;
                let (x, y) = (self.inner[k].x, self.inner[k].y);
                if self.inner[k].stopped || !matches!(self.region_at(x, y + 1), FACE | RIM) {
                    let d = &mut self.inner[k];
                    d.stopped = true;
                    d.trail.pop();
                } else {
                    let g = self.rand_glyph();
                    let i = self.idx(x, y + 1).unwrap();
                    self.glyphs[i] = g;
                    let d = &mut self.inner[k];
                    d.y += 1;
                    d.push_head();
                }
            }
        }
        self.inner.retain(|d| !(d.stopped && d.trail.is_empty()));

        // капли с нижних кромок (режим "splash")
        if self.flow == Flow::Splash && !self.bottom_edges.is_empty() {
            self.drip_acc += self.density * self.c.digits.drip_rate * self.bottom_edges.len() as f32 * 0.04 * dt;
            while self.drip_acc >= 1.0 {
                self.drip_acc -= 1.0;
                let (x, y) = self.bottom_edges[self.rng.below(self.bottom_edges.len())];
                let len = 2 + self.rng.below(4);
                let ttl = 3 + self.rng.below(10) as i32;
                let speed = self.speed * (0.4 + 0.4 * self.rng.f());
                self.drips.push(Drop::new(x, y, speed, len, ttl));
            }
        }
        for k in 0..self.drips.len() {
            self.drips[k].acc += self.drips[k].speed * dt;
            while self.drips[k].acc >= 1.0 {
                self.drips[k].acc -= 1.0;
                let g = self.rand_glyph();
                let d = &mut self.drips[k];
                if d.ttl <= 0 {
                    d.stopped = true;
                    d.trail.pop();
                    continue;
                }
                d.ttl -= 1;
                d.y += 1;
                d.push_head();
                if let Some(i) = self.idx(self.drips[k].x, self.drips[k].y) {
                    self.glyphs[i] = g;
                }
            }
        }
        self.drips.retain(|d| !(d.stopped && d.trail.is_empty()) && d.y < rows + d.len as i32);

        for s in &mut self.sparks {
            s.life -= dt;
        }
        self.sparks.retain(|s| s.life > 0.0);

        // символы понемногу мерцают
        self.flicker_acc += self.glyphs.len() as f32 * self.c.rain.flicker / 100.0 * dt;
        while self.flicker_acc >= 1.0 {
            self.flicker_acc -= 1.0;
            let i = self.rng.below(self.glyphs.len());
            self.glyphs[i] = self.rand_glyph();
        }
    }

    fn light(lit: &mut [u8], head: &mut [bool], cols: usize, rows: usize, d: &Drop, gain: f32) {
        for (k, &(x, y)) in d.trail.iter().enumerate() {
            if x < 0 || y < 0 || x as usize >= cols || y as usize >= rows {
                continue;
            }
            let i = y as usize * cols + x as usize;
            let is_head = k == 0 && !d.stopped;
            let v = if is_head { 255.0 } else { 230.0 * (1.0 - k as f32 / d.len as f32).powf(1.4) * gain };
            lit[i] = lit[i].max(v as u8);
            if is_head {
                head[i] = true;
            }
        }
    }

    /// Собирает кадр в уровнях яркости.
    /// Подсветка клеток струями (общая часть текстового и графического режимов).
    fn light_all(&mut self) {
        let (cols, rows) = (self.cols, self.rows);
        self.lit_out.fill(0);
        self.lit_in.fill(0);
        self.head.fill(false);
        self.head_in.fill(false);
        for d in self.drops.iter().chain(self.drips.iter()) {
            Self::light(&mut self.lit_out, &mut self.head, cols, rows, d, 1.0);
        }
        for d in &self.inner {
            Self::light(&mut self.lit_in, &mut self.head_in, cols, rows, d, 1.0);
        }
        for s in &self.sparks {
            if let Some(i) = self.idx(s.x, s.y) {
                self.lit_out[i] = self.lit_out[i].max((s.life * 500.0).min(200.0) as u8);
            }
        }
    }

    /// Для графического режима: видимые символы дождя.
    /// outside/inside — выдавать символы снаружи / внутри цифр.
    /// f(x, y, символ, яркость 0..1, голова, внутри_цифры)
    pub fn glyphs(&mut self, outside: bool, inside: bool, mut f: impl FnMut(usize, usize, char, f32, bool, bool)) {
        self.light_all();
        let through = self.flow == Flow::Through;
        let (rain_b, dig_b) = (self.c.rain.brightness, self.c.digits.brightness);
        for i in 0..self.cols * self.rows {
            let (x, y) = (i % self.cols, i / self.cols);
            match self.region[i] {
                OUT if outside => {
                    if self.lit_out[i] > 10 {
                        f(x, y, self.glyphs[i], self.lit_out[i] as f32 / 255.0 * rain_b, self.head[i], false);
                    }
                }
                FACE | RIM if inside => {
                    let fill = if self.fill_mask[i] { 0.45 * dig_b } else { 0.0 };
                    let pass = if through { self.lit_out[i] as f32 / 255.0 * rain_b } else { 0.0 };
                    let v = fill.max(self.lit_in[i] as f32 / 255.0).max(pass);
                    if v > 0.04 {
                        f(x, y, self.glyphs[i], v, self.head_in[i] || (through && self.head[i]), true);
                    }
                }
                _ => {}
            }
        }
    }

    /// Собирает кадр в уровнях яркости.
    pub fn compose(&mut self, out: &mut Vec<Cell>) {
        let (cols, rows) = (self.cols, self.rows);
        self.light_all();
        out.resize(cols * rows, Cell::default());
        let through = self.flow == Flow::Through;
        let (rain_b, dig_b, rim) = (self.c.rain.brightness, self.c.digits.brightness, self.c.digits.rim);
        for i in 0..cols * rows {
            let lo = self.lit_out[i] as f32 * rain_b;
            let li = self.lit_in[i] as f32;
            let sh = self.shade[i] as f32 / 255.0;
            let fillv = if self.fill_mask[i] { 1.0 } else { 0.0 };
            let pass = if through { lo } else { 0.0 };
            let face = |sh: f32| {
                // «стекло»: края ярче, середина темнее
                let base = fillv * 255.0 * dig_b * (1.0 - 0.4 * sh);
                (base.max(li).max(pass * 1.2), 22.0 + 18.0 * (1.0 - sh))
            };
            let (fg, bg) = match self.region[i] {
                FACE => face(sh),
                RIM if rim > 0.0 => ((255.0 * rim).max(li), 105.0 * rim),
                RIM => face(0.0),
                SIDE => {
                    let fade = 1.0 - 0.6 * sh;
                    (fillv * 160.0 * dig_b * fade + pass * 0.4, 50.0 * fade)
                }
                _ => (lo, self.glow_lvl[i] as f32),
            };
            let region = self.region[i];
            let lit_head = match region {
                OUT => self.head[i],
                FACE | RIM => self.head_in[i] || (through && self.head[i]),
                _ => false,
            };
            let fg = if lit_head { 255 } else { fg.min(255.0) as u8 };
            let ch = if fg > 14 { self.glyphs[i] } else { ' ' };
            out[i] = Cell { ch, fg, bg: bg as u8 };
        }
    }

    /// Текстовый режим: погода над часами, дата под ними — по центру часов,
    /// посередине между часами и краем экрана. Возвращает строку даты.
    pub fn overlay(&self, out: &mut [Cell], weather: Option<&[String; 3]>, date: &str) -> usize {
        let (cols, rows) = (self.cols, self.rows);
        let mut put = |x: usize, y: usize, text: &str, fg: u8| {
            // пробел по краям — отступ от дождя
            for (k, ch) in std::iter::once(' ').chain(text.chars()).chain(std::iter::once(' ')).enumerate() {
                if let Some(xx) = (x + k).checked_sub(1).filter(|&xx| xx < cols && y < rows) {
                    out[y * cols + xx] = Cell { ch, fg: if ch == ' ' { 0 } else { fg }, bg: 0 };
                }
            }
        };
        let (l, r) = (self.clock_left, self.clock_right.max(self.clock_left));
        let center = |len: usize| (l + r).div_ceil(2).saturating_sub(len / 2);
        if let Some(w) = weather
            && self.clock_top >= 2
        {
            let line = crate::info::weather_line(w);
            put(center(line.chars().count()), self.clock_top / 2, &line, 210);
        }
        let y = (self.clock_bottom + rows).div_ceil(2);
        if y < rows {
            put(center(date.chars().count()), y, date, 255);
        }
        y
    }
}
