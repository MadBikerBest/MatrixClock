//! Цифры как набор штрихов (отрезки и дуги) с толщиной: расстояние до контура
//! считается аналитически, поэтому цифры масштабируются на любой размер экрана.
//! Координаты глифа: x 0..1, y 0..1.6 (вниз), углы в градусах, 90° — вниз.
//! Два рисунка (digits.style): округлый — дуги и круглое «перо»; угловатый —
//! только отрезки и восьмиугольное «перо» (скошенные концы и изломы).

use crate::config::DigitStyle;

#[derive(Clone, Copy)]
enum Prim {
    Seg(f32, f32, f32, f32),
    /// центр, радиус, начальный угол, конечный угол (по часовой на экране)
    Arc(f32, f32, f32, f32, f32),
}
use Prim::*;

pub const GLYPH_H: f32 = 1.6;
const STROKE: f32 = 0.12; // полуширина штриха
const DIGIT_W: f32 = 1.0;
const COLON_W: f32 = 0.45;
const GAP: f32 = 0.2;

fn glyph(c: char, style: DigitStyle) -> &'static [Prim] {
    if style == DigitStyle::Angular {
        return angular(c);
    }
    match c {
        '0' => &[Arc(0.5, 0.5, 0.33, 180.0, 360.0), Arc(0.5, 1.1, 0.33, 0.0, 180.0), Seg(0.17, 0.5, 0.17, 1.1), Seg(0.83, 0.5, 0.83, 1.1)],
        '1' => &[Seg(0.55, 0.15, 0.55, 1.45), Seg(0.27, 0.42, 0.55, 0.15), Seg(0.28, 1.45, 0.82, 1.45)],
        '2' => &[Arc(0.5, 0.5, 0.33, 180.0, 380.0), Seg(0.81, 0.613, 0.17, 1.45), Seg(0.17, 1.45, 0.85, 1.45)],
        '3' => &[Arc(0.5, 0.47, 0.3, 200.0, 450.0), Arc(0.5, 1.12, 0.33, 270.0, 520.0)],
        '4' => &[Seg(0.65, 0.15, 0.15, 1.05), Seg(0.15, 1.05, 0.88, 1.05), Seg(0.65, 0.15, 0.65, 1.45)],
        '5' => &[Seg(0.82, 0.15, 0.25, 0.15), Seg(0.25, 0.15, 0.22, 0.72), Seg(0.22, 0.72, 0.30, 0.763), Arc(0.5, 1.05, 0.35, 235.0, 515.0)],
        '6' => &[Arc(0.5, 1.08, 0.33, 0.0, 360.0), Arc(0.95, 1.08, 0.78, 180.0, 252.0)],
        '7' => &[Seg(0.17, 0.15, 0.83, 0.15), Seg(0.83, 0.15, 0.4, 1.45)],
        '8' => &[Arc(0.5, 0.46, 0.27, 0.0, 360.0), Arc(0.5, 1.1, 0.33, 0.0, 360.0)],
        '9' => &[Arc(0.5, 0.52, 0.33, 0.0, 360.0), Arc(0.05, 0.52, 0.78, 0.0, 72.0)],
        ':' => &[Arc(0.225, 0.55, 0.035, 0.0, 360.0), Arc(0.225, 1.12, 0.035, 0.0, 360.0)],
        _ => &[],
    }
}

/// Угловатые цифры: ломаные со скосами 45°, двоеточие — точки (отрезки нулевой длины).
fn angular(c: char) -> &'static [Prim] {
    match c {
        '0' => &[Seg(0.32, 0.15, 0.68, 0.15), Seg(0.68, 0.15, 0.83, 0.3), Seg(0.83, 0.3, 0.83, 1.3), Seg(0.83, 1.3, 0.68, 1.45), Seg(0.68, 1.45, 0.32, 1.45), Seg(0.32, 1.45, 0.17, 1.3), Seg(0.17, 1.3, 0.17, 0.3), Seg(0.17, 0.3, 0.32, 0.15)],
        '1' => &[Seg(0.32, 0.38, 0.55, 0.15), Seg(0.55, 0.15, 0.55, 1.45), Seg(0.3, 1.45, 0.8, 1.45)],
        '2' => &[Seg(0.17, 0.3, 0.32, 0.15), Seg(0.32, 0.15, 0.68, 0.15), Seg(0.68, 0.15, 0.83, 0.3), Seg(0.83, 0.3, 0.83, 0.62), Seg(0.83, 0.62, 0.17, 1.22), Seg(0.17, 1.22, 0.17, 1.45), Seg(0.17, 1.45, 0.83, 1.45)],
        '3' => &[Seg(0.17, 0.15, 0.68, 0.15), Seg(0.68, 0.15, 0.83, 0.3), Seg(0.83, 0.3, 0.83, 0.65), Seg(0.83, 0.65, 0.68, 0.8), Seg(0.68, 0.8, 0.38, 0.8), Seg(0.68, 0.8, 0.83, 0.95), Seg(0.83, 0.95, 0.83, 1.3), Seg(0.83, 1.3, 0.68, 1.45), Seg(0.68, 1.45, 0.17, 1.45)],
        '4' => &[Seg(0.17, 0.15, 0.17, 0.95), Seg(0.17, 0.95, 0.83, 0.95), Seg(0.66, 0.5, 0.66, 1.45)],
        '5' => &[Seg(0.83, 0.15, 0.17, 0.15), Seg(0.17, 0.15, 0.17, 0.75), Seg(0.17, 0.75, 0.68, 0.75), Seg(0.68, 0.75, 0.83, 0.9), Seg(0.83, 0.9, 0.83, 1.3), Seg(0.83, 1.3, 0.68, 1.45), Seg(0.68, 1.45, 0.17, 1.45)],
        '6' => &[Seg(0.78, 0.15, 0.32, 0.15), Seg(0.32, 0.15, 0.17, 0.3), Seg(0.17, 0.3, 0.17, 1.3), Seg(0.17, 1.3, 0.32, 1.45), Seg(0.32, 1.45, 0.68, 1.45), Seg(0.68, 1.45, 0.83, 1.3), Seg(0.83, 1.3, 0.83, 0.9), Seg(0.83, 0.9, 0.68, 0.75), Seg(0.68, 0.75, 0.17, 0.75)],
        '7' => &[Seg(0.17, 0.15, 0.83, 0.15), Seg(0.83, 0.15, 0.83, 0.38), Seg(0.83, 0.38, 0.45, 0.98), Seg(0.45, 0.98, 0.45, 1.45)],
        '8' => &[Seg(0.32, 0.15, 0.68, 0.15), Seg(0.68, 0.15, 0.83, 0.3), Seg(0.83, 0.3, 0.83, 0.65), Seg(0.83, 0.65, 0.68, 0.8), Seg(0.68, 0.8, 0.32, 0.8), Seg(0.32, 0.8, 0.17, 0.65), Seg(0.17, 0.65, 0.17, 0.3), Seg(0.17, 0.3, 0.32, 0.15), Seg(0.32, 0.8, 0.17, 0.95), Seg(0.17, 0.95, 0.17, 1.3), Seg(0.17, 1.3, 0.32, 1.45), Seg(0.32, 1.45, 0.68, 1.45), Seg(0.68, 1.45, 0.83, 1.3), Seg(0.83, 1.3, 0.83, 0.95), Seg(0.83, 0.95, 0.68, 0.8)],
        '9' => &[Seg(0.22, 1.45, 0.68, 1.45), Seg(0.68, 1.45, 0.83, 1.3), Seg(0.83, 1.3, 0.83, 0.3), Seg(0.83, 0.3, 0.68, 0.15), Seg(0.68, 0.15, 0.32, 0.15), Seg(0.32, 0.15, 0.17, 0.3), Seg(0.17, 0.3, 0.17, 0.65), Seg(0.17, 0.65, 0.32, 0.8), Seg(0.32, 0.8, 0.83, 0.8)],
        ':' => &[Seg(0.225, 0.55, 0.225, 0.55), Seg(0.225, 1.12, 0.225, 1.12)],
        _ => &[],
    }
}

fn advance(c: char) -> f32 {
    if c == ':' { COLON_W } else { DIGIT_W }
}

/// Расстояние до штриха; oct — восьмиугольное «перо» (угловатый рисунок) вместо круглого.
fn dist(p: Prim, x: f32, y: f32, oct: bool) -> f32 {
    match p {
        Seg(ax, ay, bx, by) => {
            let (dx, dy) = (bx - ax, by - ay);
            let l2 = dx * dx + dy * dy;
            let t = if l2 > 0.0 { (((x - ax) * dx + (y - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let (vx, vy) = ((x - ax - t * dx).abs(), (y - ay - t * dy).abs());
            if oct { vx.max(vy).max((vx + vy) * std::f32::consts::FRAC_1_SQRT_2) } else { vx.hypot(vy) }
        }
        Arc(cx, cy, r, a0, a1) => {
            let (dx, dy) = (x - cx, y - cy);
            let mut a = dy.atan2(dx).to_degrees();
            while a < a0 {
                a += 360.0;
            }
            if a <= a1 {
                ((dx * dx + dy * dy).sqrt() - r).abs()
            } else {
                let end = |deg: f32| {
                    let t = deg.to_radians();
                    ((x - cx - r * t.cos()).powi(2) + (y - cy - r * t.sin()).powi(2)).sqrt()
                };
                end(a0).min(end(a1))
            }
        }
    }
}

/// Строка времени, разложенная в штрихи со смещением по x.
pub struct Text {
    prims: Vec<Prim>,
    pub width: f32,
    stroke: f32,
    /// Восьмиугольное «перо» (угловатый рисунок).
    pub oct: bool,
}

impl Text {
    /// thickness, spacing — множители толщины штриха и промежутка между знаками.
    pub fn new(s: &str, colon_visible: bool, thickness: f32, spacing: f32, style: DigitStyle) -> Text {
        let mut prims = Vec::new();
        let mut x = 0.0;
        for (i, c) in s.chars().enumerate() {
            if i > 0 {
                x += GAP * spacing;
            }
            let strokes: &[Prim] = if c == ':' && !colon_visible { &[] } else { glyph(c, style) };
            for &p in strokes {
                prims.push(match p {
                    Seg(ax, ay, bx, by) => Seg(ax + x, ay, bx + x, by),
                    Arc(cx, cy, r, a0, a1) => Arc(cx + x, cy, r, a0, a1),
                });
            }
            x += advance(c);
        }
        Text { prims, width: x, stroke: STROKE * thickness, oct: style == DigitStyle::Angular }
    }

    /// Штрихи для шейдера: по два vec4 на штрих.
    /// Отрезок: (ax, ay, bx, by), (0, 0, 0, 0); дуга: (cx, cy, r, 0), (a0, a1, 1, 0) — углы в радианах.
    pub fn gpu_prims(&self) -> Vec<[f32; 8]> {
        self.prims
            .iter()
            .map(|&p| match p {
                Seg(ax, ay, bx, by) => [ax, ay, bx, by, 0.0, 0.0, 0.0, 0.0],
                Arc(cx, cy, r, a0, a1) => [cx, cy, r, 0.0, a0.to_radians(), a1.to_radians(), 1.0, 0.0],
            })
            .collect()
    }

    /// Знаковое расстояние до контура (в единицах глифа): < 0 — внутри цифры.
    pub fn sdf(&self, x: f32, y: f32) -> f32 {
        self.prims.iter().map(|&p| dist(p, x, y, self.oct)).fold(f32::MAX, f32::min) - self.stroke
    }

    /// Полуширина штриха — для оценки «глубины» внутри.
    pub fn stroke(&self) -> f32 {
        self.stroke
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_solid_and_bounded() {
        for (style, c) in [DigitStyle::Round, DigitStyle::Angular].into_iter().flat_map(|s| "0123456789:".chars().map(move |c| (s, c))) {
            let t = Text::new(&c.to_string(), true, 1.0, 1.0, style);
            let mut inside = 0;
            for iy in 0..160 {
                for ix in 0..100 {
                    let (x, y) = (ix as f32 / 100.0 * t.width, iy as f32 / 100.0);
                    if t.sdf(x, y) < 0.0 {
                        inside += 1;
                    }
                }
            }
            assert!(inside > 200, "{style:?} {c}: {inside}");
            // края глифа пустые
            assert!(t.sdf(-0.05, 0.8) > 0.0 && t.sdf(t.width + 0.05, 0.8) > 0.0, "{style:?} {c}");
        }
    }
}

#[cfg(test)]
mod colon_tests {
    use super::*;

    #[test]
    fn colon_present() {
        for style in [DigitStyle::Round, DigitStyle::Angular] {
            let t = Text::new("12:45", true, 1.0, 1.0, style);
            let x = 1.0 + GAP + 1.0 + GAP + 0.225;
            assert!(t.sdf(x, 0.55) < 0.0, "{style:?}: верхняя точка: {}", t.sdf(x, 0.55));
            assert!(t.sdf(x, 1.12) < 0.0, "{style:?}: нижняя точка");
            let t2 = Text::new("12:45", false, 1.0, 1.0, style);
            assert!(t2.sdf(x, 0.55) > 0.0);
        }
    }
}
