//! Вывод в терминал: уровни яркости → цвет, отрисовка только изменившихся клеток.
//! full — 24-битный цвет; lite — 16 цветов, палитра консоли Linux
//! перепрограммируется последовательностью ESC ] P nrrggbb (man console_codes).

use crate::config::{Colors, Rgb, parse_color};
use crate::scene::Cell;
use std::io::Write;


fn to_hsv((r, g, b): Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max > 0.0 { d / max } else { 0.0 }, max)
}

fn from_hsv(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + v - c, g + v - c, b + v - c)
}

#[derive(Clone, Copy)]
pub struct Palette {
    base: Rgb,
    alarm: Rgb,
    head: Rgb,
    bg: Rgb,
}

impl Palette {
    pub fn new(c: &Colors) -> Result<Palette, String> {
        Ok(Palette {
            base: parse_color(&c.color)?,
            alarm: parse_color(&c.alarm_color)?,
            head: parse_color(&c.head_color)?,
            bg: parse_color(&c.background)?,
        })
    }
}

/// Цвета для графики (0..1): основной (со сдвигом к тревожному на tint), головы, фон.
pub fn tinted(p: Palette, tint: f32) -> [[f32; 3]; 3] {
    let (h0, s0, v0) = to_hsv(p.base);
    let (h1, s1, v1) = to_hsv(p.alarm);
    let mut dh = h1 - h0;
    if dh > 180.0 {
        dh -= 360.0;
    } else if dh < -180.0 {
        dh += 360.0;
    }
    let (r, g, b) = from_hsv(h0 + dh * tint, s0 + (s1 - s0) * tint, v0 + (v1 - v0) * tint);
    let f = |c: Rgb| [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0];
    [[r, g, b], f(p.head), f(p.bg)]
}

/// Таблица «уровень → цвет» для смеси основного и тревожного цвета (tint 0..1).
/// Уровень 0 — цвет фона, верх шкалы (головы струй) — head_color.
fn build_lut(p: Palette, tint: f32) -> [Rgb; 256] {
    let (base, alarm) = (p.base, p.alarm);
    let (h0, s0, v0) = to_hsv(base);
    let (h1, s1, v1) = to_hsv(alarm);
    let mut dh = h1 - h0;
    if dh > 180.0 {
        dh -= 360.0;
    } else if dh < -180.0 {
        dh += 360.0;
    }
    let (h, s, v) = (h0 + dh * tint, s0 + (s1 - s0) * tint, v0 + (v1 - v0) * tint);
    let mut lut = [(0, 0, 0); 256];
    for (i, e) in lut.iter_mut().enumerate() {
        let l = i as f32 / 255.0;
        let (r, g, b) = if l < 0.88 {
            from_hsv(h, s, v * (l / 0.88).powf(1.5))
        } else {
            // самая яркая часть (головы струй) уходит в head_color
            let (r, g, b) = from_hsv(h, s, v);
            let (hr, hg, hb) = (p.head.0 as f32 / 255.0, p.head.1 as f32 / 255.0, p.head.2 as f32 / 255.0);
            let w = (l - 0.88) / 0.12;
            (r + (hr - r) * w, g + (hg - g) * w, b + (hb - b) * w)
        };
        // поверх фона: аддитивно, с ограничением
        let add = |c: f32, b: u8| ((c * 255.0) as u16 + b as u16).min(255) as u8;
        *e = (add(r, p.bg.0), add(g, p.bg.1), add(b, p.bg.2));
    }
    lut
}

pub struct Term {
    lite: bool,
    linux_console: bool,
    pal: Palette,
    lut: [Rgb; 256],
    prev: Vec<Cell>,
    buf: Vec<u8>,
    cols: usize,
}

impl Term {
    pub fn new(lite: bool, linux_console: bool, pal: Palette) -> Term {
        let mut t = Term { lite, linux_console, pal, lut: [(0, 0, 0); 256], prev: Vec::new(), buf: Vec::new(), cols: 0 };
        // альтернативный экран, скрыть курсор, мышь (включая движение) для «пробуждения»
        t.buf.extend_from_slice(b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1003h\x1b[?1006h\x1b[0m\x1b[2J");
        t.set_tint(0.0);
        t
    }

    /// Сдвиг гаммы к тревожному цвету: 0 — основной цвет, 1 — полностью тревожный.
    pub fn set_tint(&mut self, tint: f32) {
        self.lut = build_lut(self.pal, tint);
        if self.lite {
            if self.linux_console {
                for i in 0..16usize {
                    let (r, g, b) = self.lut[i * 255 / 15];
                    let _ = write!(self.buf, "\x1b]P{i:x}{r:02x}{g:02x}{b:02x}");
                }
            }
        } else {
            self.prev.clear(); // полная перерисовка новыми цветами
        }
    }

    fn color(&mut self, fg: bool, level: u8) {
        if self.lite {
            let mut idx = (level as u32 * 15 + 127) / 255;
            if !fg {
                idx = idx.min(7); // фон в консоли Linux — только 8 цветов
            }
            let _ = match (fg, idx >= 8) {
                (true, false) => write!(self.buf, "\x1b[{}m", 30 + idx),
                (true, true) => write!(self.buf, "\x1b[{}m", 90 + idx - 8),
                (false, _) => write!(self.buf, "\x1b[{}m", 40 + idx),
            };
        } else {
            let (r, g, b) = self.lut[level as usize];
            let _ = write!(self.buf, "\x1b[{};2;{r};{g};{b}m", if fg { 38 } else { 48 });
        }
    }

    pub fn draw(&mut self, cells: &[Cell], cols: usize) -> std::io::Result<()> {
        if cols != self.cols || self.prev.len() != cells.len() {
            self.cols = cols;
            self.prev.clear();
            self.buf.extend_from_slice(b"\x1b[0m\x1b[2J");
        }
        let full = self.prev.is_empty();
        let mut cursor: Option<usize> = None;
        let (mut cur_fg, mut cur_bg): (Option<u8>, Option<u8>) = (None, None);
        // в lite уровни квантуются до 16 — сравниваем уже квантованные
        let q = |l: u8, lite: bool| if lite { ((l as u32 * 15 + 127) / 255) as u8 } else { l };
        for (i, c) in cells.iter().enumerate() {
            if !full {
                let p = self.prev[i];
                let same_bg = q(p.bg, self.lite).min(if self.lite { 7 } else { 255 })
                    == q(c.bg, self.lite).min(if self.lite { 7 } else { 255 });
                if p.ch == c.ch && same_bg && (c.ch == ' ' || q(p.fg, self.lite) == q(c.fg, self.lite)) {
                    continue;
                }
            }
            if cursor != Some(i) {
                let _ = write!(self.buf, "\x1b[{};{}H", i / cols + 1, i % cols + 1);
            }
            if c.ch != ' ' && cur_fg.map(|f| q(f, self.lite)) != Some(q(c.fg, self.lite)) {
                self.color(true, c.fg);
                cur_fg = Some(c.fg);
            }
            if cur_bg.map(|b| q(b, self.lite)) != Some(q(c.bg, self.lite)) {
                self.color(false, c.bg);
                cur_bg = Some(c.bg);
            }
            let mut tmp = [0u8; 4];
            self.buf.extend_from_slice(c.ch.encode_utf8(&mut tmp).as_bytes());
            cursor = if (i + 1) % cols == 0 { None } else { Some(i + 1) };
        }
        self.prev.clear();
        self.prev.extend_from_slice(cells);
        self.flush()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut out = std::io::stdout().lock();
        out.write_all(&self.buf)?;
        self.buf.clear();
        out.flush()
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        self.buf.extend_from_slice(b"\x1b[?1006l\x1b[?1003l\x1b[?1000l\x1b[0m\x1b[2J\x1b[H\x1b[?25h\x1b[?1049l");
        if self.lite && self.linux_console {
            self.buf.extend_from_slice(b"\x1b]R");
        }
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_endpoints() {
        let p = Palette::new(&Colors::default()).unwrap();
        let g = build_lut(p, 0.0);
        assert_eq!(g[0], (0, 0, 0));
        assert!(g[200].1 > g[200].0, "зелёный доминирует");
        let r = build_lut(p, 1.0);
        assert!(r[200].0 > r[200].1, "красный доминирует");
    }
}
