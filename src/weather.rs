//! Key-free live weather from Open-Meteo.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Clear,
    Clouds,
    Rain,
    Snow,
    /// Snow on a gale.
    Blizzard,
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

#[derive(Clone, Debug)]
pub struct Config {
    pub location: Location,
    pub units: Units,
}

pub fn config(location: &str, units: Units) -> Result<Option<Config>, String> {
    let raw = if location.trim().is_empty() { std::env::var("ALMANAC_LOCATION").unwrap_or_default() } else { location.into() };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let location = Location::parse(&raw).ok_or("invalid coordinates; latitude must be -90..90 and longitude -180..180")?;
    Ok(Some(Config { location, units }))
}

/// A sky to preview without a key: `ALMANAC_SKY=rain` and so on.
pub fn preview() -> Option<Conditions> {
    let (kind, cover) = match std::env::var("ALMANAC_SKY").ok()?.to_lowercase().as_str() {
        "clear" | "sunny" => (Kind::Clear, 0.),
        "partly" | "partly-sunny" | "partly-cloudy" => (Kind::Clouds, 0.45),
        "clouds" | "cloudy" | "overcast" => (Kind::Clouds, 0.95),
        "rain" => (Kind::Rain, 0.9),
        "snow" => (Kind::Snow, 0.9),
        "blizzard" => (Kind::Blizzard, 1.),
        "fog" | "mist" => (Kind::Fog, 0.9),
        "storm" | "thunderstorm" => (Kind::Storm, 1.),
        _ => return None,
    };
    Some(Conditions { kind, cover, intensity: 0.7 })
}

/// Wind, in m/s, that turns snow into a blizzard; heavy snow needs less.
const BLIZZARD_WIND: f64 = 15.;
const HEAVY_SNOW_WIND: f64 = 10.;

/// What the sky is doing, from the WMO code and the measurements: any
/// precipitation at all is rain (or snow, below freezing) whatever the code
/// says, and strong wind turns snow into a blizzard.
fn classify(code: u64, precipitation: f64, temp_c: f64, wind_ms: f64) -> Kind {
    let kind = match kind_of(code) {
        Kind::Clear | Kind::Clouds if precipitation > 0. => {
            if temp_c <= 0.5 { Kind::Snow } else { Kind::Rain }
        }
        kind => kind,
    };
    let heavy = matches!(code, 75 | 86);
    if kind == Kind::Snow && (wind_ms >= BLIZZARD_WIND || (heavy && wind_ms >= HEAVY_SNOW_WIND)) { Kind::Blizzard } else { kind }
}

/// Words for the sky. For the dry codes the cloud cover decides, so the
/// report never says "cloudy" over a mostly clear sky or the reverse.
fn describe(code: u64, kind: Kind, cover: f64) -> &'static str {
    match (kind, code) {
        (Kind::Blizzard, _) => "blizzard",
        (Kind::Clear | Kind::Clouds, _) if cover < 0.25 => "clear",
        (Kind::Clear | Kind::Clouds, _) if cover < 0.7 => "partly cloudy",
        (Kind::Clear | Kind::Clouds, _) => "overcast",
        (_, 45 | 48) => "fog",
        (_, 51..=55) => "drizzle",
        (_, 56 | 57) => "freezing drizzle",
        (_, 61) => "light rain",
        (_, 63) => "rain",
        (_, 65) => "heavy rain",
        (_, 66 | 67) => "freezing rain",
        (_, 71) => "light snow",
        (_, 73) => "snow",
        (_, 75) => "heavy snow",
        (_, 77) => "snow grains",
        (_, 80..=82) => "rain showers",
        (_, 85 | 86) => "snow showers",
        (_, 95) => "thunderstorm",
        (_, 96..=99) => "thunderstorm with hail",
        (Kind::Snow, _) => "light snow",
        (Kind::Rain, _) => "light rain",
        (Kind::Fog, _) => "fog",
        (Kind::Storm, _) => "thunderstorm",
    }
}

/// WMO codes used by Open-Meteo.
pub fn kind_of(code: u64) -> Kind {
    match code {
        1..=3 => Kind::Clouds,
        45 | 48 => Kind::Fog,
        51..=67 | 80..=82 => Kind::Rain,
        71..=77 | 85 | 86 => Kind::Snow,
        95..=99 => Kind::Storm,
        _ => Kind::Clear,
    }
}

pub fn parse(json: &str, units: Units) -> Result<Weather, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| "Open-Meteo sent invalid JSON")?;
    if v["error"].as_bool() == Some(true) {
        return Err("Open-Meteo refused the request".into());
    }
    let c = &v["current"];
    let number = |name: &str| c[name].as_f64().ok_or_else(|| format!("missing {name} in weather response"));
    let code = c["weather_code"].as_u64().ok_or("missing weather code")?;
    let precipitation = c["precipitation"].as_f64().unwrap_or(0.);
    let temp = number("temperature_2m")?;
    let wind = number("wind_speed_10m")?;
    let (temp_c, wind_ms) = match units {
        Units::Metric => (temp, wind),
        Units::Imperial => ((temp - 32.) * 5. / 9., wind * 0.447_04),
    };
    let kind = classify(code, precipitation, temp_c, wind_ms);
    let cover = (number("cloud_cover")? / 100.).clamp(0., 1.);
    Ok(Weather {
        conditions: Conditions {
            kind,
            cover: cover as f32,
            intensity: if matches!(kind, Kind::Rain | Kind::Snow | Kind::Blizzard | Kind::Storm) {
                (precipitation / 5.).clamp(0.2, 1.) as f32
            } else {
                0.
            },
        },
        place: String::new(),
        description: describe(code, kind, cover).into(),
        temp: temp as f32,
        feels: number("apparent_temperature")? as f32,
        humidity: number("relative_humidity_2m")? as u32,
        wind: wind as f32,
        sunrise: v["daily"]["sunrise"][0].as_i64().ok_or("missing sunrise")?,
        sunset: v["daily"]["sunset"][0].as_i64().ok_or("missing sunset")?,
        units,
    })
}

fn get(req: ureq::RequestBuilder<ureq::typestate::WithoutBody>) -> Result<String, String> {
    let mut response = req
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .call()
        .map_err(|_| "couldn't reach Open-Meteo".to_string())?;
    response.body_mut().read_to_string().map_err(|_| "couldn't read Open-Meteo response".into())
}

pub fn fetch(cfg: &Config) -> Result<Weather, String> {
    let (place, lat, lon) = match &cfg.location {
        Location::Coords(lat, lon) => (format!("{lat:.2}, {lon:.2}"), *lat, *lon),
        Location::City(query) => {
            let mut parts = query.split(',');
            let name = parts.next().unwrap_or(query).trim();
            let country = parts.next().map(str::trim).filter(|s| s.len() == 2);
            let mut req = ureq::get("https://geocoding-api.open-meteo.com/v1/search").query("name", name).query("count", "1");
            if let Some(country) = country {
                req = req.query("countryCode", country.to_uppercase());
            }
            let json: Value = serde_json::from_str(&get(req)?).map_err(|_| "invalid geocoding response")?;
            let r = json["results"].get(0).ok_or("no matching location")?;
            (
                r["name"].as_str().unwrap_or(name).to_string(),
                r["latitude"].as_f64().ok_or("missing latitude")?,
                r["longitude"].as_f64().ok_or("missing longitude")?,
            )
        }
    };
    let req = ureq::get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query("current", "temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code,cloud_cover,precipitation")
        .query("daily", "sunrise,sunset")
        .query("timezone", "auto")
        .query("timeformat", "unixtime")
        .query("forecast_days", "1")
        .query("temperature_unit", if cfg.units == Units::Imperial { "fahrenheit" } else { "celsius" })
        .query("wind_speed_unit", if cfg.units == Units::Imperial { "mph" } else { "ms" });
    let mut weather = parse(&get(req)?, cfg.units)?;
    weather.place = place;
    Ok(weather)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_current_and_unix_sun_times() {
        let json = r#"{"current":{"temperature_2m":12.3,"apparent_temperature":11.1,"relative_humidity_2m":82,"wind_speed_10m":4.1,"weather_code":63,"cloud_cover":90,"precipitation":3},"daily":{"sunrise":[1791374400],"sunset":[1791415800]}}"#;
        let w = parse(json, Units::Metric).unwrap();
        assert_eq!(w.conditions, Conditions { kind: Kind::Rain, cover: 0.9, intensity: 0.6 });
        assert_eq!((w.temp, w.feels, w.wind), (12.3, 11.1, 4.1));
        assert_eq!((w.sunrise, w.sunset), (1791374400, 1791415800));
        assert!(parse("{}", Units::Metric).is_err());
        assert!(parse("not JSON", Units::Metric).is_err());
    }
    #[test]
    fn wmo_codes_cover_each_sky() {
        assert_eq!([0, 3, 45, 63, 73, 95].map(kind_of), [Kind::Clear, Kind::Clouds, Kind::Fog, Kind::Rain, Kind::Snow, Kind::Storm]);
    }
    #[test]
    fn the_sky_is_read_from_the_measurements() {
        // A dry code with rain falling is rain; below freezing, snow.
        assert_eq!(classify(3, 0.2, 8., 3.), Kind::Rain);
        assert_eq!(classify(2, 0.2, -3., 3.), Kind::Snow);
        assert_eq!(classify(3, 0., 8., 3.), Kind::Clouds);
        // Snow on a gale is a blizzard; heavy snow needs less wind.
        assert_eq!(classify(73, 1., -5., 18.), Kind::Blizzard);
        assert_eq!(classify(75, 1., -5., 11.), Kind::Blizzard);
        assert_eq!(classify(73, 1., -5., 11.), Kind::Snow);
        // "Mainly clear" under a grey sky is reported as it looks.
        assert_eq!(describe(1, Kind::Clouds, 0.93), "overcast");
        assert_eq!(describe(3, Kind::Clouds, 0.1), "clear");
        assert_eq!(describe(65, Kind::Rain, 1.), "heavy rain");
    }
    #[test]
    fn imperial_wind_and_temperature_are_converted() {
        // Drizzle at 30°: below freezing in Fahrenheit, warm in Celsius.
        let json = r#"{"current":{"temperature_2m":30,"apparent_temperature":25,"relative_humidity_2m":80,"wind_speed_10m":20,"weather_code":3,"cloud_cover":100,"precipitation":0.4},"daily":{"sunrise":[1791374400],"sunset":[1791415800]}}"#;
        // 20 mph is a breeze; 20 m/s is a gale.
        assert_eq!(parse(json, Units::Imperial).unwrap().conditions.kind, Kind::Snow);
        assert_eq!(parse(json, Units::Metric).unwrap().conditions.kind, Kind::Rain);
        let snowing = json.replace(r#""weather_code":3"#, r#""weather_code":73"#);
        assert_eq!(parse(&snowing, Units::Imperial).unwrap().conditions.kind, Kind::Snow);
        assert_eq!(parse(&snowing, Units::Metric).unwrap().conditions.kind, Kind::Blizzard);
    }
    #[test]
    fn locations_are_validated() {
        assert_eq!(Location::parse("91,0"), None);
        assert_eq!(Location::parse("NaN,0"), None);
        assert_eq!(Location::parse("41.88,-87.63"), Some(Location::Coords(41.88, -87.63)));
    }
}
