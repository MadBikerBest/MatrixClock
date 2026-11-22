use serde::Deserialize;
use std::path::PathBuf;

pub const EXAMPLE: &str = include_str!("../config.example.toml");

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Auto,
    Full,
    Lite,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Auto,
    Ru,
    En,
    De,
    Fr,
}

/// Рисунок цифр часов.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DigitStyle {
    /// Округлые: дуги и круглые концы штрихов.
    Round,
    /// Угловатые: прямые штрихи, скошенные углы и концы.
    Angular,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Flow {
    Through,
    Around,
    Splash,
    Occlude,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    DownRight,
    DownLeft,
    UpRight,
    UpLeft,
    Down,
}

impl Direction {
    /// Единичный шаг выдавливания (x, y), y — вниз.
    pub fn vector(self) -> (f32, f32) {
        match self {
            Direction::DownRight => (1.0, 1.0),
            Direction::DownLeft => (-1.0, 1.0),
            Direction::UpRight => (1.0, -1.0),
            Direction::UpLeft => (-1.0, -1.0),
            Direction::Down => (0.0, 1.4),
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub clock: Clock,
    pub weather: Weather,
    pub rain: Rain,
    pub digits: Digits,
    pub colors: Colors,
    pub console: Console,
    pub graphics: Graphics,
    pub lock: Lock,
    /// Откуда загружен (None — значения по умолчанию).
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Weather {
    pub location: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub idle_minutes: u32,
    pub mode: Mode,
    pub language: Language,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Clock {
    pub hours: u8,
    pub seconds: bool,
    pub blink_colon: bool,
    pub size: f32,
    pub max_height: f32,
    pub position_x: f32,
    pub position_y: f32,
    pub thickness: f32,
    pub spacing: f32,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Rain {
    pub density: f32,
    pub speed: f32,
    pub speed_spread: f32,
    pub length: u32,
    pub length_spread: f32,
    pub brightness: f32,
    pub flicker: f32,
    pub charset: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Digits {
    pub style: DigitStyle,
    pub depth: u32,
    pub depth_direction: Direction,
    pub glow: u32,
    pub glow_strength: f32,
    pub fill: f32,
    pub brightness: f32,
    pub rim: f32,
    pub flow: Flow,
    pub drip_rate: f32,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub color: String,
    pub alarm_color: String,
    pub head_color: String,
    pub background: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Console {
    pub fps: u32,
    pub cell_aspect: f32,
    pub font: String,
}

impl Default for Console {
    fn default() -> Self {
        Self { fps: 15, cell_aspect: 2.0, font: String::new() }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Graphics {
    pub fps: u32,
    pub font: String,
    pub glyph_size: u32,
    pub mirror_glyphs: bool,
    pub layers: u32,
    pub bevel: f32,
    pub specular: f32,
    pub bloom: f32,
    pub exposure: f32,
    pub vignette: f32,
    pub reflection: f32,
    pub floor_height: f32,
}

impl Default for Graphics {
    fn default() -> Self {
        Self {
            fps: 60,
            font: String::new(),
            glyph_size: 26,
            mirror_glyphs: true,
            layers: 3,
            bevel: 0.6,
            specular: 0.8,
            bloom: 1.0,
            exposure: 1.0,
            vignette: 0.4,
            reflection: 0.0,
            floor_height: 0.2,
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Lock {
    pub enabled: bool,
    pub pam_service: String,
    pub prompt: String,
    pub input_timeout: u32,
    pub max_attempts: u32,
    pub lockout_seconds: u32,
    pub lock_all_vt: bool,
}

impl Default for General {
    fn default() -> Self {
        Self { idle_minutes: 5, mode: Mode::Auto, language: Language::Auto }
    }
}
impl Default for Clock {
    fn default() -> Self {
        Self {
            hours: 24,
            seconds: false,
            blink_colon: true,
            size: 0.8,
            max_height: 0.62,
            position_x: 0.5,
            position_y: 0.45,
            thickness: 1.0,
            spacing: 1.0,
        }
    }
}
impl Default for Rain {
    fn default() -> Self {
        Self {
            density: 0.6,
            speed: 14.0,
            speed_spread: 0.4,
            length: 14,
            length_spread: 0.5,
            brightness: 0.85,
            flicker: 20.0,
            charset: "matrix".into(),
        }
    }
}
impl Default for Digits {
    fn default() -> Self {
        Self {
            style: DigitStyle::Round,
            depth: 3,
            depth_direction: Direction::DownRight,
            glow: 3,
            glow_strength: 0.5,
            fill: 0.7,
            brightness: 0.6,
            rim: 0.9,
            flow: Flow::Through,
            drip_rate: 1.0,
        }
    }
}
impl Default for Colors {
    fn default() -> Self {
        Self {
            color: "#00ff41".into(),
            alarm_color: "#ff1a1a".into(),
            head_color: "#e0ffe8".into(),
            background: "#000000".into(),
        }
    }
}
impl Default for Lock {
    fn default() -> Self {
        Self {
            enabled: true,
            pam_service: "matrixclock".into(),
            prompt: "PASSWORD:".into(),
            input_timeout: 10,
            max_attempts: 5,
            lockout_seconds: 60,
            lock_all_vt: false,
        }
    }
}

pub type Rgb = (u8, u8, u8);

/// Цвет "#rrggbb" → (r, g, b).
pub fn parse_color(s: &str) -> Result<Rgb, String> {
    let h = s.strip_prefix('#').unwrap_or(s);
    let v = u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6);
    v.map(|v| ((v >> 16) as u8, (v >> 8) as u8, v as u8))
        .ok_or_else(|| format!("неверный цвет \"{s}\", нужен вид \"#rrggbb\""))
}

impl Config {
    fn validate(&self) -> Result<(), String> {
        let c = &self.colors;
        for s in [&c.color, &c.alarm_color, &c.head_color, &c.background] {
            parse_color(s)?;
        }
        let (k, r, d, l) = (&self.clock, &self.rain, &self.digits, &self.lock);
        let checks = [
            ((1..=1440).contains(&self.general.idle_minutes), "general.idle_minutes: 1–1440"),
            ((1..=60).contains(&self.console.fps), "console.fps: 1–60"),
            ((1.0..=4.0).contains(&self.console.cell_aspect), "console.cell_aspect: 1.0–4.0"),
            ((1..=144).contains(&self.graphics.fps), "graphics.fps: 1–144"),
            (k.hours == 12 || k.hours == 24, "clock.hours: 12 или 24"),
            ((0.1..=1.0).contains(&k.size), "clock.size: 0.1–1.0"),
            ((0.1..=1.0).contains(&k.max_height), "clock.max_height: 0.1–1.0"),
            ((0.0..=1.0).contains(&k.position_x), "clock.position_x: 0.0–1.0"),
            ((0.0..=1.0).contains(&k.position_y), "clock.position_y: 0.0–1.0"),
            ((0.5..=2.0).contains(&k.thickness), "clock.thickness: 0.5–2.0"),
            ((0.0..=3.0).contains(&k.spacing), "clock.spacing: 0.0–3.0"),
            ((0.0..=5.0).contains(&r.density), "rain.density: 0.0–5.0"),
            ((1.0..=200.0).contains(&r.speed), "rain.speed: 1–200"),
            ((0.0..=0.9).contains(&r.speed_spread), "rain.speed_spread: 0.0–0.9"),
            ((1..=200).contains(&r.length), "rain.length: 1–200"),
            ((0.0..=0.9).contains(&r.length_spread), "rain.length_spread: 0.0–0.9"),
            ((0.1..=1.0).contains(&r.brightness), "rain.brightness: 0.1–1.0"),
            ((0.0..=100.0).contains(&r.flicker), "rain.flicker: 0–100"),
            (!r.charset.is_empty(), "rain.charset: не может быть пустым"),
            (d.depth <= 10, "digits.depth: 0–10"),
            (d.glow <= 10, "digits.glow: 0–10"),
            ((0.0..=1.0).contains(&d.glow_strength), "digits.glow_strength: 0.0–1.0"),
            ((0.0..=1.0).contains(&d.fill), "digits.fill: 0.0–1.0"),
            ((0.1..=1.0).contains(&d.brightness), "digits.brightness: 0.1–1.0"),
            ((0.0..=1.0).contains(&d.rim), "digits.rim: 0.0–1.0"),
            ((0.0..=10.0).contains(&d.drip_rate), "digits.drip_rate: 0.0–10.0"),
            ((8..=96).contains(&self.graphics.glyph_size), "graphics.glyph_size: 8–96"),
            ((1..=4).contains(&self.graphics.layers), "graphics.layers: 1–4"),
            ((0.1..=1.0).contains(&self.graphics.bevel), "graphics.bevel: 0.1–1.0"),
            ((0.0..=1.0).contains(&self.graphics.specular), "graphics.specular: 0.0–1.0"),
            ((0.0..=3.0).contains(&self.graphics.bloom), "graphics.bloom: 0.0–3.0"),
            ((0.2..=4.0).contains(&self.graphics.exposure), "graphics.exposure: 0.2–4.0"),
            ((0.0..=1.0).contains(&self.graphics.vignette), "graphics.vignette: 0.0–1.0"),
            ((0.0..=1.0).contains(&self.graphics.reflection), "graphics.reflection: 0.0–1.0"),
            ((0.05..=0.5).contains(&self.graphics.floor_height), "graphics.floor_height: 0.05–0.5"),
            ((1..=600).contains(&l.input_timeout), "lock.input_timeout: 1–600"),
            ((1..=100).contains(&l.max_attempts), "lock.max_attempts: 1–100"),
            (l.lockout_seconds <= 3600, "lock.lockout_seconds: 0–3600"),
        ];
        match checks.iter().find(|(ok, _)| !ok) {
            Some((_, msg)) => Err(format!("недопустимое значение — {msg}")),
            None => Ok(()),
        }
    }

    /// Загрузка: явный путь, иначе ~/.config, иначе /etc, иначе значения по умолчанию
    /// (и тогда пользовательский файл создаётся из примера — см. create_user).
    pub fn load(explicit: Option<PathBuf>) -> Result<Config, String> {
        let path = explicit.or_else(|| {
            let user = user_path();
            [user.clone(), Some(PathBuf::from("/etc/matrixclock/config.toml"))]
                .into_iter()
                .flatten()
                .find(|p| p.exists())
                .or_else(|| user.and_then(create_user))
        });
        let cfg: Config = match path {
            Some(p) => {
                let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let mut c: Config = toml::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
                c.path = Some(p);
                c
            }
            None => Config::default(),
        };
        cfg.validate()?;
        Ok(cfg)
    }
}

/// Записать weather.location в файл настроек, сохранив комментарии и порядок (toml_edit).
pub fn save_location(path: &std::path::Path, location: &str) -> Result<(), String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let text = std::fs::read_to_string(path).map_err(|e| err(&e))?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| err(&e))?;
    let mut v = toml_edit::Value::from(location);
    if let Some(old) = doc.get("weather").and_then(|w| w.get("location")).and_then(|i| i.as_value()) {
        *v.decor_mut() = old.decor().clone(); // комментарий в конце строки остаётся
    }
    doc["weather"]["location"] = toml_edit::Item::Value(v);
    std::fs::write(path, doc.to_string()).map_err(|e| err(&e))
}

/// $XDG_CONFIG_HOME/matrixclock/config.toml, по умолчанию $HOME/.config/…
/// Пустые и относительные значения переменных игнорируются (XDG Base Directory).
fn user_path() -> Option<PathBuf> {
    let abs = |v: std::ffi::OsString| Some(PathBuf::from(v)).filter(|p| p.is_absolute());
    std::env::var_os("XDG_CONFIG_HOME")
        .and_then(abs)
        .or_else(|| std::env::var_os("HOME").and_then(abs).map(|h| h.join(".config")))
        .map(|d| d.join("matrixclock/config.toml"))
}

/// Первый запуск: файл настроек из примера. Не под root (его настройки — в /etc,
/// а через sudo -E файл root оказался бы в домашнем каталоге пользователя).
/// Неудача — не ошибка запуска: работаем со значениями по умолчанию.
fn create_user(path: PathBuf) -> Option<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::DirBuilderExt;
    if unsafe { libc::geteuid() } == 0 {
        return None;
    }
    let res = (|| {
        if let Some(dir) = path.parent() {
            // новые каталоги — 0700, существующие не трогаются (XDG Base Directory)
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        }
        std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?.write_all(EXAMPLE.as_bytes())
    })();
    match res {
        Ok(()) => {
            eprintln!("matrixclock: создан файл настроек {}", path.display());
            Some(path)
        }
        Err(e) => {
            eprintln!("matrixclock: не удалось создать {}: {e} — настройки по умолчанию", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_matches_defaults() {
        let parsed: Config = toml::from_str(EXAMPLE).unwrap();
        assert_eq!(parsed, Config::default());
        parsed.validate().unwrap();
    }

    #[test]
    fn partial_config() {
        let c: Config = toml::from_str("[digits]\nflow = \"splash\"\ndepth_direction = \"up-left\"\n").unwrap();
        assert_eq!(c.digits.flow, Flow::Splash);
        assert_eq!(c.digits.depth_direction, Direction::UpLeft);
        assert_eq!(c.rain, Rain::default());
    }

    #[test]
    fn save_location_keeps_comments() {
        let dir = std::env::temp_dir().join(format!("matrixclock-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        std::fs::write(&p, EXAMPLE).unwrap();
        save_location(&p, "станция Восток").unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        let c: Config = toml::from_str(&text).unwrap();
        assert_eq!(c.weather.location, "станция Восток");
        assert_eq!(text.lines().count(), EXAMPLE.lines().count());
        std::fs::write(&p, "[clock]\nhours = 12\n").unwrap();
        save_location(&p, "Москва").unwrap();
        let c: Config = toml::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!((c.weather.location.as_str(), c.clock.hours), ("Москва", 12));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn typo_is_error() {
        assert!(toml::from_str::<Config>("[rain]\ndensty = 1.0\n").is_err());
    }
}
