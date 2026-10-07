//! Everything the clock knows about the day, as pure functions of a local
//! date-time: the big digits, the moon, the part of the day, the ticker.

use chrono::{Datelike, NaiveDateTime, Timelike};
use ferrite_design::prelude::ascii;

// ── The big digits ────────────────────────────────────────────────────────

/// Columns per glyph and the gap between glyphs, in font pixels.
const GLYPH: usize = 5;
const GAP: usize = 1;

/// The clock face as text, five glyphs wide: `09:41`, or ` 9:41` in 12-hour
/// time (a blank keeps the colon where it was).
pub fn face(t: NaiveDateTime, h24: bool) -> String {
    if h24 {
        format!("{:02}:{:02}", t.hour(), t.minute())
    } else {
        format!("{:>2}:{:02}", t.hour12().1, t.minute())
    }
}

/// Grid width of a face of `n` glyphs.
pub fn face_width(n: usize) -> usize {
    n * GLYPH + n.saturating_sub(1) * GAP
}

/// Lit (column, row) pixels of `text` in Ferrite's 5×5 block font. Unknown
/// characters (a space) are blank but keep their width, so digits never
/// shift as the time changes.
pub fn pixels(text: &str) -> Vec<(usize, usize)> {
    let mut lit = Vec::new();
    for (i, c) in text.chars().enumerate() {
        let x0 = i * (GLYPH + GAP);
        let Some((_, rows)) = ascii::block_glyph(c) else { continue };
        for (y, row) in rows.iter().enumerate() {
            for x in 0..GLYPH {
                if row & (1 << (GLYPH - 1 - x)) != 0 {
                    lit.push((x0 + x, y));
                }
            }
        }
    }
    lit
}

/// The unlit "ghost" behind each digit: every pixel any digit can light,
/// like the dead segments of an LCD. The colon slot (index 2) gets the
/// colon's own two dots, so it reads as off rather than missing.
pub fn ghost(n: usize) -> Vec<(usize, usize)> {
    let mut all: Vec<(usize, usize)> = (0..n)
        .flat_map(|i| {
            let glyphs: Vec<char> = if i == 2 { vec![':'] } else { ('0'..='9').collect() };
            glyphs.into_iter().flat_map(move |d| {
                pixels(&d.to_string()).into_iter().map(move |(x, y)| (x + i * (GLYPH + GAP), y))
            })
        })
        .collect();
    all.sort_unstable();
    all.dedup();
    all
}

// ── The moon ──────────────────────────────────────────────────────────────

pub const SYNODIC: f64 = 29.530_588_853;
/// A known new moon: 2000-01-06 18:14 UTC, in days since the Unix epoch.
const NEW_MOON_EPOCH: f64 = 10_962.759_7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moon {
    /// Days since the last new moon, 0 ≤ age < 29.53.
    pub age: f64,
}

impl Moon {
    pub fn at_unix(secs: i64) -> Self {
        let days = secs as f64 / 86_400.;
        Moon { age: (days - NEW_MOON_EPOCH).rem_euclid(SYNODIC) }
    }

    /// 0 at new, 0.5 at full, back towards 1.
    pub fn fraction(self) -> f64 {
        self.age / SYNODIC
    }

    /// The lit share of the disc, 0–1.
    pub fn illumination(self) -> f64 {
        (1. - (self.fraction() * std::f64::consts::TAU).cos()) / 2.
    }

    pub fn waxing(self) -> bool {
        self.fraction() < 0.5
    }

    pub fn name(self) -> &'static str {
        const NAMES: [&str; 8] =
            ["New moon", "Waxing crescent", "First quarter", "Waxing gibbous", "Full moon", "Waning gibbous", "Last quarter", "Waning crescent"];
        NAMES[((self.fraction() * 8.).round() as usize) % 8]
    }

    /// Whole days until the next full moon (0 = tonight).
    pub fn days_to_full(self) -> u32 {
        (SYNODIC / 2. - self.age).rem_euclid(SYNODIC).floor() as u32
    }

    /// Brightness of the disc at `x`, `y` in -1..1 (y down), or `None`
    /// outside it. Waxing light falls on the right, as seen from the north.
    pub fn shade(self, x: f32, y: f32) -> Option<f32> {
        let r2 = x * x + y * y;
        if r2 > 1. {
            return None;
        }
        let half = (1. - y * y).max(0.).sqrt();
        let c = (self.fraction() as f32 * std::f32::consts::TAU).cos();
        let lit = if self.waxing() { x > c * half } else { x < -c * half };
        // A few maria, so the full moon isn't a flat disc.
        let mare = [(-0.3f32, -0.25f32, 0.22f32), (0.25, 0.1, 0.18), (-0.05, 0.4, 0.15)]
            .iter()
            .any(|&(mx, my, r)| (x - mx).powi(2) + (y - my).powi(2) < r * r);
        Some(match (lit, mare) {
            (true, false) => 0.92,
            (true, true) => 0.62,
            (false, _) => 0.1, // earthshine
        })
    }
}

// ── The day ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Night,
    Dawn,
    Day,
    Dusk,
}

impl Part {
    /// A fixed civil split; without a location the clock can't know the
    /// real sunrise, and this reads right most of the year.
    pub fn of(hour: u32) -> Self {
        match hour {
            5..=6 => Part::Dawn,
            7..=17 => Part::Day,
            18..=20 => Part::Dusk,
            _ => Part::Night,
        }
    }

    /// From the real sunrise and sunset (Unix seconds): dawn and dusk are
    /// the 45 minutes either side of each.
    pub fn from_sun(now: i64, sunrise: i64, sunset: i64) -> Self {
        const TWILIGHT: i64 = 45 * 60;
        if (now - sunrise).abs() <= TWILIGHT {
            Part::Dawn
        } else if (now - sunset).abs() <= TWILIGHT {
            Part::Dusk
        } else if now > sunrise && now < sunset {
            Part::Day
        } else {
            Part::Night
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Part::Night => "NIGHT",
            Part::Dawn => "DAWN",
            Part::Day => "DAY",
            Part::Dusk => "DUSK",
        }
    }
}

fn year_len(year: i32) -> u32 {
    if chrono::NaiveDate::from_ymd_opt(year, 2, 29).is_some() { 366 } else { 365 }
}

/// The ticker: facts about today, separated by dots.
pub fn ticker(t: NaiveDateTime, moon: Moon) -> String {
    let (day, len) = (t.ordinal(), year_len(t.year()));
    let week = t.iso_week().week();
    let quarter = (t.month() - 1) / 3 + 1;
    let full = match moon.days_to_full() {
        0 => "FULL MOON TONIGHT".to_string(),
        1 => "FULL MOON TOMORROW".to_string(),
        n => format!("FULL MOON IN {n} DAYS"),
    };
    format!(
        "{} · DAY {day} OF {len} · WEEK {week} · Q{quarter} · {} DAYS LEFT IN {} · {} {:.0}% · {full} · ",
        t.format("%A %-d %B %Y").to_string().to_uppercase(),
        len - day,
        t.year(),
        moon.name().to_uppercase(),
        moon.illumination() * 100.,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 6).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn faces_keep_five_glyphs() {
        assert_eq!(face(at(9, 41), true), "09:41");
        assert_eq!(face(at(9, 41), false), " 9:41");
        assert_eq!(face(at(21, 5), false), " 9:05");
        assert_eq!(face(at(0, 0), false), "12:00");
        assert_eq!(face_width(5), 29);
    }

    #[test]
    fn blank_keeps_its_width() {
        let wide: Vec<_> = pixels("19:41").into_iter().filter(|&(x, _)| x >= 6).collect();
        let narrow: Vec<_> = pixels(" 9:41");
        assert_eq!(wide, narrow);
    }

    #[test]
    fn ghost_covers_every_digit() {
        let ghost = ghost(5);
        for d in ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'] {
            let face = format!("{d}{d}:{d}{d}");
            assert!(pixels(&face).iter().filter(|&&(x, _)| !(12..17).contains(&x)).all(|p| ghost.contains(p)), "{face}");
        }
        let colon: Vec<_> = ghost.iter().filter(|&&(x, _)| (12..17).contains(&x)).copied().collect();
        assert_eq!(colon, pixels("  :").into_iter().collect::<Vec<_>>(), "colon slot holds only the colon");
    }

    #[test]
    fn moon_matches_known_phases() {
        // Full moon 2026-10-26 04:12 UTC; new moon 2026-10-10 15:50 UTC.
        let full = Moon::at_unix(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(4, 12, 0).unwrap().and_utc().timestamp());
        assert!(full.illumination() > 0.98, "{full:?}");
        assert_eq!(full.name(), "Full moon");
        let new = Moon::at_unix(NaiveDate::from_ymd_opt(2026, 10, 10).unwrap().and_hms_opt(15, 50, 0).unwrap().and_utc().timestamp());
        assert!(new.illumination() < 0.02, "{new:?}");
        assert_eq!(new.name(), "New moon");
    }

    #[test]
    fn moon_lights_the_right_side_when_waxing() {
        let first_quarter = Moon { age: SYNODIC / 4. };
        assert!(first_quarter.shade(0.5, 0.).unwrap() > 0.5);
        assert!(first_quarter.shade(-0.5, 0.).unwrap() < 0.5);
        let last_quarter = Moon { age: SYNODIC * 0.75 };
        assert!(last_quarter.shade(-0.5, 0.).unwrap() > 0.5);
        assert_eq!(Moon { age: 3. }.shade(1., 1.), None);
    }

    #[test]
    fn parts_of_the_day() {
        assert_eq!([4, 5, 7, 17, 18, 21].map(Part::of), [Part::Night, Part::Dawn, Part::Day, Part::Day, Part::Dusk, Part::Night]);
    }

    #[test]
    fn parts_from_the_real_sun() {
        let (rise, set) = (1_000_000, 1_000_000 + 12 * 3600);
        assert_eq!(Part::from_sun(rise - 3 * 3600, rise, set), Part::Night);
        assert_eq!(Part::from_sun(rise + 600, rise, set), Part::Dawn);
        assert_eq!(Part::from_sun(rise + 6 * 3600, rise, set), Part::Day);
        assert_eq!(Part::from_sun(set - 600, rise, set), Part::Dusk);
        assert_eq!(Part::from_sun(set + 3 * 3600, rise, set), Part::Night);
    }

    #[test]
    fn ticker_counts_the_year() {
        let line = ticker(at(9, 0), Moon { age: 0. });
        assert!(line.starts_with("TUESDAY 6 OCTOBER 2026 · DAY 279 OF 365 · WEEK 41 · Q4 · 86 DAYS LEFT IN 2026"), "{line}");
        assert!(line.contains("FULL MOON IN 14 DAYS"), "{line}");
    }
}
