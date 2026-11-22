//! Надписи над и под часами: место, температура, погода (Open-Meteo) и дата.
//! Сеть — в фоновом потоке; нет ответа (нет интернета) — строка погоды не выводится.
//! Место: weather.location из конфига или по IP (ipwho.is) — тогда записывается в конфиг.
//! Язык надписей: general.language, по умолчанию — по локали системы.

use crate::config::Language;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Строка над часами: [место, температура, погода].
pub type Weather = Arc<Mutex<Option<[String; 3]>>>;

/// Дни недели с воскресенья, по языкам в порядке Language::Ru, En, De, Fr.
const WEEKDAYS: [[&str; 7]; 4] = [
    ["Воскресенье", "Понедельник", "Вторник", "Среда", "Четверг", "Пятница", "Суббота"],
    ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
    ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"],
    ["Dimanche", "Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi"],
];

/// Язык из конфига; "auto" — по локали, как её выбирает POSIX (LC_ALL → LC_MESSAGES → LANG,
/// первая непустая): ru_*, de_*, fr_* → этот язык, любая другая (и C/POSIX) → английский.
pub fn resolve(l: Language) -> Language {
    if l != Language::Auto {
        return l;
    }
    let loc = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).find(|v| !v.is_empty());
    match loc.as_deref().map(|v| v.get(..2).unwrap_or(v)) {
        Some("ru") => Language::Ru,
        Some("de") => Language::De,
        Some("fr") => Language::Fr,
        _ => Language::En,
    }
}

/// Номер языка в таблицах (Auto сюда не попадает — его заменяет resolve).
fn idx(lang: Language) -> usize {
    match lang {
        Language::Ru => 0,
        Language::De => 2,
        Language::Fr => 3,
        Language::En | Language::Auto => 1,
    }
}

/// «День недели - дд.мм.гггг» по местному времени.
pub fn date_text(lang: Language) -> String {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let wd = WEEKDAYS[idx(lang)][tm.tm_wday.rem_euclid(7) as usize];
        format!("{wd} - {:02}.{:02}.{}", tm.tm_mday, tm.tm_mon + 1, tm.tm_year + 1900)
    }
}

/// Строка погоды для вывода: «место   температура   погода».
pub fn weather_line(w: &[String; 3]) -> String {
    w.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("   ")
}

/// Код погоды WMO (таблица из документации Open-Meteo) → описание на языке надписей.
fn sky(code: u32, lang: Language) -> &'static str {
    let t: [&str; 4] = match code {
        0 => ["Ясно", "Clear", "Klar", "Ciel dégagé"],
        1 => ["Малооблачно", "Mainly clear", "Überwiegend klar", "Peu nuageux"],
        2 => ["Переменная облачность", "Partly cloudy", "Teilweise bewölkt", "Partiellement nuageux"],
        3 => ["Пасмурно", "Overcast", "Bedeckt", "Couvert"],
        45 | 48 => ["Туман", "Fog", "Nebel", "Brouillard"],
        51..=55 => ["Морось", "Drizzle", "Nieselregen", "Bruine"],
        56 | 57 => ["Ледяная морось", "Freezing drizzle", "Gefrierender Nieselregen", "Bruine verglaçante"],
        61 => ["Небольшой дождь", "Light rain", "Leichter Regen", "Pluie faible"],
        63 => ["Дождь", "Rain", "Regen", "Pluie"],
        65 => ["Сильный дождь", "Heavy rain", "Starker Regen", "Forte pluie"],
        66 | 67 => ["Ледяной дождь", "Freezing rain", "Gefrierender Regen", "Pluie verglaçante"],
        71 => ["Небольшой снег", "Light snow", "Leichter Schneefall", "Neige faible"],
        73 => ["Снег", "Snow", "Schneefall", "Neige"],
        75 => ["Сильный снег", "Heavy snow", "Starker Schneefall", "Forte neige"],
        77 => ["Снежная крупа", "Snow grains", "Schneegriesel", "Neige en grains"],
        80 | 81 => ["Ливень", "Rain showers", "Regenschauer", "Averses"],
        82 => ["Сильный ливень", "Violent rain showers", "Heftige Regenschauer", "Fortes averses"],
        85 => ["Снегопад", "Snow showers", "Schneeschauer", "Averses de neige"],
        86 => ["Сильный снегопад", "Heavy snow showers", "Starke Schneeschauer", "Fortes averses de neige"],
        95 => ["Гроза", "Thunderstorm", "Gewitter", "Orage"],
        96 | 99 => ["Гроза с градом", "Thunderstorm with hail", "Gewitter mit Hagel", "Orage avec grêle"],
        97 => ["Сильная гроза", "Heavy thunderstorm", "Schweres Gewitter", "Violent orage"],
        _ => [""; 4],
    };
    t[idx(lang)]
}

fn temp_text(t: f32) -> String {
    match t.round() as i32 {
        0 => "0°C".into(),
        t => format!("{t:+}°C"),
    }
}

#[derive(Deserialize)]
struct IpGeo {
    success: bool,
    city: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct GeoList {
    #[serde(default)]
    results: Vec<Geo>,
}

#[derive(Deserialize)]
struct Geo {
    name: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct Forecast {
    current: Current,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f32,
    weather_code: u32,
}

type Place = (String, f64, f64);

/// Код языка для запросов к геокодерам.
fn code(lang: Language) -> &'static str {
    ["ru", "en", "de", "fr"][idx(lang)]
}

/// Место по названию (геокодер Open-Meteo, название — на языке надписей).
fn geocode(agent: &ureq::Agent, name: &str, lang: Language) -> Result<Option<Place>, ureq::Error> {
    let list: GeoList = agent
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", name)
        .query("count", "1")
        .query("language", code(lang))
        .call()?
        .body_mut()
        .read_json()?;
    Ok(list.results.into_iter().next().map(|g| (g.name, g.latitude, g.longitude)))
}

/// Место по IP; найденный город записывается в конфиг (weather.location).
fn by_ip(agent: &ureq::Agent, cfg_path: Option<&PathBuf>, lang: Language) -> Result<Option<Place>, ureq::Error> {
    let g: IpGeo = agent
        .get("https://ipwho.is/")
        .query("lang", code(lang))
        .query("fields", "success,city,latitude,longitude")
        .call()?
        .body_mut()
        .read_json()?;
    if !g.success || g.city.is_empty() {
        return Ok(None);
    }
    if let Some(p) = cfg_path
        && let Err(e) = crate::config::save_location(p, &g.city)
    {
        eprintln!("matrixclock: {e}");
    }
    Ok(Some((g.city, g.latitude, g.longitude)))
}

fn current(agent: &ureq::Agent, lat: f64, lon: f64, lang: Language) -> Result<[String; 3], ureq::Error> {
    let f: Forecast = agent
        .get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query("current", "temperature_2m,weather_code")
        .call()?
        .body_mut()
        .read_json()?;
    Ok([String::new(), temp_text(f.current.temperature_2m), sky(f.current.weather_code, lang).into()])
}

/// Фоновый опрос: погода раз в 15 минут (так обновляет Open-Meteo), при ошибке — повтор через минуту.
pub fn start(location: String, cfg_path: Option<PathBuf>, lang: Language) -> Weather {
    let out: Weather = Arc::new(Mutex::new(None));
    let shared = out.clone();
    std::thread::spawn(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
        let mut place: Option<Place> = None;
        let mut warned = false;
        loop {
            if place.is_none() {
                place = match location.trim() {
                    "" => by_ip(&agent, cfg_path.as_ref(), lang).ok().flatten(),
                    name => match geocode(&agent, name, lang) {
                        Ok(None) if !warned => {
                            warned = true;
                            eprintln!("matrixclock: weather.location = \"{name}\" — место не найдено");
                            None
                        }
                        r => r.ok().flatten(),
                    },
                };
            }
            let w = place.as_ref().and_then(|(name, lat, lon)| {
                current(&agent, *lat, *lon, lang).ok().map(|[_, t, s]| [name.clone(), t, s])
            });
            let ok = w.is_some();
            *shared.lock().unwrap_or_else(|e| e.into_inner()) = w;
            std::thread::sleep(Duration::from_secs(if ok { 15 * 60 } else { 60 }));
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature() {
        assert_eq!(temp_text(-0.3), "0°C");
        assert_eq!(temp_text(1.6), "+2°C");
        assert_eq!(temp_text(-12.4), "-12°C");
    }

    #[test]
    fn date_format() {
        for lang in [Language::Ru, Language::En, Language::De, Language::Fr] {
            let d = date_text(lang);
            let (wd, dmy) = d.split_once(" - ").unwrap();
            assert!(WEEKDAYS[idx(lang)].contains(&wd));
            assert_eq!(dmy.len(), 10);
        }
    }
}
