//! Key-free live weather from Open-Meteo.

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
    let kind = kind_of(code);
    let description = match kind {
        Kind::Clear => "clear",
        Kind::Clouds => "cloudy",
        Kind::Fog => "fog",
        Kind::Rain => "rain",
        Kind::Snow => "snow",
        Kind::Storm => "thunderstorm",
    };
    let precipitation = c["precipitation"].as_f64().unwrap_or(0.);
    Ok(Weather {
        conditions: Conditions {
            kind,
            cover: (number("cloud_cover")? / 100.).clamp(0., 1.) as f32,
            intensity: if matches!(kind, Kind::Rain | Kind::Snow | Kind::Storm) { (precipitation / 5.).clamp(0.2, 1.) as f32 } else { 0. },
        },
        place: String::new(),
        description: description.into(),
        temp: number("temperature_2m")? as f32,
        feels: number("apparent_temperature")? as f32,
        humidity: number("relative_humidity_2m")? as u32,
        wind: number("wind_speed_10m")? as f32,
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
    fn locations_are_validated() {
        assert_eq!(Location::parse("91,0"), None);
        assert_eq!(Location::parse("NaN,0"), None);
        assert_eq!(Location::parse("41.88,-87.63"), Some(Location::Coords(41.88, -87.63)));
    }
}
