//! Optional live weather from OpenWeather's current-weather API. Off unless
//! `OPENWEATHER_API_KEY` is set; the key is read from the environment only
//! and never shown, logged or written anywhere.
//!
//!     OPENWEATHER_API_KEY=…            your key
//!     ALMANAC_LOCATION=Chicago,US   a city (name,country) or "lat,lon"
//!
//! The location and units can also be set on the Settings screen.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Clear,
    Clouds,
    Rain,
    Snow,
    Fog,
    Storm,
}

/// What the sky should show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conditions {
    pub kind: Kind,
    /// Cloud cover, 0–1.
    pub cover: f32,
    /// How hard it's coming down, 0–1 (rain and snow).
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Units {
    Metric,
    Imperial,
}

impl Units {
    fn api(self) -> &'static str {
        match self {
            Units::Metric => "metric",
            Units::Imperial => "imperial",
        }
    }

    pub fn temp(self) -> &'static str {
        match self {
            Units::Metric => "°C",
            Units::Imperial => "°F",
        }
    }

    pub fn speed(self) -> &'static str {
        match self {
            Units::Metric => "m/s",
            Units::Imperial => "mph",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub conditions: Conditions,
    pub place: String,
    pub description: String,
    pub temp: f32,
    pub feels: f32,
    pub humidity: u32,
    pub wind: f32,
    /// Unix seconds.
    pub sunrise: i64,
    pub sunset: i64,
    pub units: Units,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Location {
    City(String),
    Coords(f64, f64),
}

impl Location {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Some((a, b)) = s.split_once(',')
            && let (Ok(lat), Ok(lon)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>())
        {
            return ((-90. ..=90.).contains(&lat) && (-180. ..=180.).contains(&lon)).then_some(Location::Coords(lat, lon));
        }
        Some(Location::City(s.to_string()))
    }
}

#[derive(Clone)]
pub struct Config {
    key: String,
    pub location: Location,
    pub units: Units,
}

impl std::fmt::Debug for Config {
    // Never print the key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config").field("location", &self.location).field("units", &self.units).finish_non_exhaustive()
    }
}

/// `Ok(None)` when weather is off (no key); `Err` when it's half set up.
/// `location` (from the settings screen) wins over `ALMANAC_LOCATION`.
pub fn config(location: &str, units: Units) -> Result<Option<Config>, String> {
    let Some(key) = std::env::var("OPENWEATHER_API_KEY").ok().filter(|k| !k.trim().is_empty()) else { return Ok(None) };
    let location = Location::parse(location)
        .or_else(|| std::env::var("ALMANAC_LOCATION").ok().and_then(|l| Location::parse(&l)))
        .ok_or("set a location in Settings (a city like Chicago,US, or lat,lon)")?;
    Ok(Some(Config { key: key.trim().to_string(), location, units }))
}

/// A sky to preview without a key: `ALMANAC_SKY=rain` and so on.
pub fn preview() -> Option<Conditions> {
    let kind = match std::env::var("ALMANAC_SKY").ok()?.to_lowercase().as_str() {
        "clear" => Kind::Clear,
        "clouds" | "cloudy" => Kind::Clouds,
        "rain" => Kind::Rain,
        "snow" => Kind::Snow,
        "fog" | "mist" => Kind::Fog,
        "storm" | "thunderstorm" => Kind::Storm,
        _ => return None,
    };
    let cover = match kind {
        Kind::Clear => 0.,
        Kind::Clouds => 0.6,
        _ => 0.9,
    };
    Some(Conditions { kind, cover, intensity: 0.7 })
}

/// Whether a key is present, without reading it anywhere else.
pub fn has_key() -> bool {
    std::env::var("OPENWEATHER_API_KEY").is_ok_and(|k| !k.trim().is_empty())
}

/// OpenWeather condition codes → what to draw.
/// <https://openweathermap.org/weather-conditions>
pub fn kind_of(id: u64) -> Kind {
    match id {
        200..=299 => Kind::Storm,
        300..=399 | 500..=599 => Kind::Rain,
        600..=699 => Kind::Snow,
        700..=799 => Kind::Fog,
        801..=899 => Kind::Clouds,
        _ => Kind::Clear,
    }
}

/// How hard rain or snow falls, from the code's position in its band
/// (OpenWeather orders them light → extreme).
fn intensity_of(id: u64) -> f32 {
    match id {
        300 | 500 | 520 | 600 | 615 | 620 => 0.3,
        301 | 310 | 311 | 501 | 521 | 601 | 616 | 621 => 0.6,
        200..=299 => 0.8,
        300..=699 => 0.9,
        _ => 0.,
    }
}

/// Parse a current-weather response.
pub fn parse(json: &str, units: Units) -> Result<Weather, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    let w = v["weather"].get(0).ok_or("no weather in response")?;
    let id = w["id"].as_u64().ok_or("no condition code")?;
    let num = |p: &str| v.pointer(p).and_then(Value::as_f64);
    Ok(Weather {
        conditions: Conditions {
            kind: kind_of(id),
            cover: (num("/clouds/all").unwrap_or(0.) / 100.) as f32,
            intensity: intensity_of(id),
        },
        place: v["name"].as_str().unwrap_or("").to_string(),
        description: w["description"].as_str().unwrap_or("").to_string(),
        temp: num("/main/temp").ok_or("no temperature")? as f32,
        feels: num("/main/feels_like").unwrap_or(0.) as f32,
        humidity: num("/main/humidity").unwrap_or(0.) as u32,
        wind: num("/wind/speed").unwrap_or(0.) as f32,
        sunrise: num("/sys/sunrise").unwrap_or(0.) as i64,
        sunset: num("/sys/sunset").unwrap_or(0.) as i64,
        units,
    })
}

/// Fetch the current weather (blocking; call it off the main thread).
/// Errors are written without the request URL, which holds the key.
pub fn fetch(cfg: &Config) -> Result<Weather, String> {
    let mut req = ureq::get("https://api.openweathermap.org/data/2.5/weather")
        .query("appid", &cfg.key)
        .query("units", cfg.units.api());
    req = match &cfg.location {
        Location::City(q) => req.query("q", q),
        Location::Coords(lat, lon) => req.query("lat", lat.to_string()).query("lon", lon.to_string()),
    };
    let mut resp = req.call().map_err(|e| match e {
        ureq::Error::StatusCode(401) => "OpenWeather rejected the key (new keys take a few hours to activate)".to_string(),
        ureq::Error::StatusCode(404) => "OpenWeather doesn't know that location".to_string(),
        ureq::Error::StatusCode(429) => "OpenWeather rate limit; retrying later".to_string(),
        ureq::Error::StatusCode(code) => format!("OpenWeather returned HTTP {code}"),
        _ => "couldn't reach OpenWeather".to_string(),
    })?;
    let body = resp.body_mut().read_to_string().map_err(|_| "couldn't read OpenWeather's response".to_string())?;
    parse(&body, cfg.units)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"coord":{"lon":-87.65,"lat":41.85},
        "weather":[{"id":501,"main":"Rain","description":"moderate rain","icon":"10n"}],
        "main":{"temp":12.3,"feels_like":11.1,"humidity":82},"wind":{"speed":4.1},
        "clouds":{"all":90},"sys":{"sunrise":1791374400,"sunset":1791415800},"name":"Chicago"}"#;

    #[test]
    fn parses_a_response() {
        let w = parse(SAMPLE, Units::Metric).unwrap();
        assert_eq!(w.conditions, Conditions { kind: Kind::Rain, cover: 0.9, intensity: 0.6 });
        assert_eq!((w.place.as_str(), w.description.as_str(), w.humidity), ("Chicago", "moderate rain", 82));
        assert_eq!((w.temp, w.feels, w.wind), (12.3, 11.1, 4.1));
        assert_eq!((w.sunrise, w.sunset), (1791374400, 1791415800));
    }

    #[test]
    fn rejects_junk() {
        assert!(parse("not json", Units::Metric).is_err());
        assert!(parse(r#"{"weather":[]}"#, Units::Metric).is_err());
    }

    #[test]
    fn codes_map_to_skies() {
        assert_eq!([211, 300, 502, 601, 741, 800, 804].map(kind_of), [Kind::Storm, Kind::Rain, Kind::Rain, Kind::Snow, Kind::Fog, Kind::Clear, Kind::Clouds]);
    }

    #[test]
    fn locations() {
        assert_eq!(Location::parse("41.88, -87.63"), Some(Location::Coords(41.88, -87.63)));
        assert_eq!(Location::parse("Chicago,US"), Some(Location::City("Chicago,US".into())));
        assert_eq!(Location::parse("91,0"), None);
        assert_eq!(Location::parse("  "), None);
    }

    #[test]
    fn debug_hides_the_key() {
        let cfg = Config { key: "s3cret".into(), location: Location::City("X".into()), units: Units::Metric };
        assert!(!format!("{cfg:?}").contains("s3cret"));
    }
}
