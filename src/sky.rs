//! The little sky over the clock: a looping picture for the part of the
//! day and the weather, rendered as ASCII by `ascii_film`. Frames are cheap
//! to make and are rebuilt only when the scene changes (hourly, or when new
//! weather arrives). Every moving thing shifts by a whole period over the
//! loop, so the film never jumps.

use std::f32::consts::{PI, TAU};

use ferrite_design::prelude::ascii;
use ferrite_design::prelude::dither::Picture;

use crate::almanac::{Moon, Part};
use crate::weather::{Conditions, Kind};

const W: u32 = 320;
const H: u32 = 112;
/// Text rows per column of the rendered film (`ascii::art_fit`: rows are
/// cols × aspect / 2).
pub const ROWS_PER_COL: f32 = H as f32 / W as f32 / 2.;

/// Everything a frame depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub part: Part,
    /// How far the sun is across the day, 0 (rise) → 1 (set).
    pub sun: f32,
    pub moon: Moon,
    /// Live weather, or `None` for the clock's own fair-weather sky.
    pub weather: Option<Conditions>,
}

impl Scene {
    /// What to draw: the live weather, or a couple of clouds by day.
    fn conditions(&self) -> Conditions {
        self.weather.unwrap_or(Conditions {
            kind: if self.part == Part::Day { Kind::Clouds } else { Kind::Clear },
            cover: if self.part == Part::Day { 0.3 } else { 0. },
            intensity: 0.,
        })
    }
}

/// How the film should turn levels into characters: rain wants strokes
/// (`/`), everything else reads best as plain density.
pub fn style(weather: Option<Conditions>) -> (ascii::Charset, ascii::Fit) {
    match weather.map(|c| c.kind) {
        Some(Kind::Rain | Kind::Storm) => (ascii::Charset::Slashes, ascii::Fit::Shape),
        _ => (ascii::Charset::Classic, ascii::Fit::Tone),
    }
}

/// What a part of the sky is, for colouring it: the value an ink mask
/// holds there (0 = nothing, the film's own colour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Ink {
    Sun = 1,
    Moon,
    Star,
    Glow,
    Cloud,
    Rain,
    Snow,
    Fog,
    Bolt,
    Ground,
}

#[cfg_attr(not(test), allow(dead_code))]
impl Ink {
    /// In colour-table order (only the tests walk it).
    pub const ALL: [Ink; 10] =
        [Ink::Sun, Ink::Moon, Ink::Star, Ink::Glow, Ink::Cloud, Ink::Rain, Ink::Snow, Ink::Fog, Ink::Bolt, Ink::Ground];
}

/// Natural colours for each [`Ink`] (in [`Ink::ALL`] order), as `0xRRGGBB`,
/// for a dark page or a light one. The sky is an illustration, like a
/// photo: these stay put whatever the scheme (the setting's "off" gives
/// back the scheme's own ink). On paper every ink is darker, so it still
/// reads; at night the hills go dark.
pub fn natural_inks(dark: bool, part: Part) -> [u32; 10] {
    let night = part == Part::Night;
    if dark {
        [
            0xF2D23B,                                   // sun
            0xE8E8E6,                                   // moon
            0xA9B4C8,                                   // stars
            0xF0985A,                                   // dawn/dusk glow
            if night { 0x8A93A3 } else { 0xE8EBF0 },    // clouds
            0x5AA9F0,                                   // rain
            0xF2F6FF,                                   // snow
            0x9AA3AD,                                   // fog
            0xFFF3A0,                                   // lightning
            if night { 0x3F7A3A } else { 0x72C25E },    // hills
        ]
    } else {
        [
            0xA87A00, 0x55555A, 0x6B7385, 0xB5561A,
            if night { 0x55555A } else { 0x7A808A },
            0x1F62B0, 0x5B7A9C, 0x7A828C, 0x8A6A00,
            if night { 0x2D5A28 } else { 0x3A7A2C },
        ]
    }
}

/// A loop of the sky: the pictures, and for each the ink mask that says
/// which part of the scene every sample is.
pub struct Film {
    pub frames: Vec<Picture>,
    pub masks: Vec<Picture>,
}

pub fn frames(scene: Scene, n: usize) -> Film {
    let (frames, masks) = (0..n).map(|f| frame(scene, f as f32 / n as f32)).unzip();
    Film { frames, masks }
}

/// One moment of the loop, as levels and as inks.
fn frame(scene: Scene, t: f32) -> (Picture, Picture) {
    let (mut levels, mut inks) = (Vec::with_capacity((W * H) as usize), Vec::with_capacity((W * H) as usize));
    for py in 0..H {
        for px in 0..W {
            let (level, ink) = point(scene, t, (px as f32 + 0.5) / W as f32, (py as f32 + 0.5) / H as f32);
            levels.push((level.clamp(0., 1.) * 255.).round() as u8);
            inks.push(if level > 0.01 { ink.map_or(0, |i| i as u8) } else { 0 });
        }
    }
    (Picture::new(W, H, levels), Picture::new(W, H, inks))
}

/// The level at `u`, `v`, and what put it there: the layer that wins.
fn point(scene: Scene, t: f32, u: f32, v: f32) -> (f32, Option<Ink>) {
    let c = scene.conditions();
    let overcast = c.cover > 0.75 || matches!(c.kind, Kind::Rain | Kind::Storm | Kind::Fog);
    let flash = c.kind == Kind::Storm && (0.5..0.55).contains(&t);
    let (x, y) = (u * W as f32, v * H as f32);
    let ground = 0.84 + 0.05 * (u * 9. + 1.).sin() + 0.025 * (u * 23.).sin();
    if v > ground {
        // Snow settles on the hills.
        return match (c.kind, scene.part) {
            (Kind::Snow, _) => (0.62, Some(Ink::Snow)),
            (_, Part::Day) => (0.38, Some(Ink::Ground)),
            _ => (0.24, Some(Ink::Ground)),
        };
    }

    // Keep the brighter of what's there and `layer`, and whose it is.
    let top = |(a, ia): (f32, Option<Ink>), (b, ib): (f32, Option<Ink>)| if b > a { (b, ib) } else { (a, ia) };

    // The sky behind the weather: sun, moon, stars, twilight glow.
    let behind = match scene.part {
        Part::Night => top((stars(x, y, t, 1.), Some(Ink::Star)), (moon_at(scene.moon, x, y, 0.8 * W as f32, 0.3 * H as f32), Some(Ink::Moon))),
        Part::Day => {
            let s = scene.sun.clamp(0., 1.);
            (sun(x, y, (0.1 + 0.8 * s) * W as f32, (0.75 - 0.55 * (s * PI).sin()) * H as f32, t), Some(Ink::Sun))
        }
        Part::Dawn | Part::Dusk => {
            let cx = if scene.part == Part::Dawn { 0.22 } else { 0.78 } * W as f32;
            let high_stars = if v < 0.35 { stars(x, y, t, 0.35) } else { 0. };
            [(bands(v, ground, t), Some(Ink::Glow)), (high_stars, Some(Ink::Star))]
                .into_iter()
                .fold((sun(x, y, cx, ground * H as f32 - 6., t), Some(Ink::Sun)), top)
        }
    };
    // Heavy weather hides it; the brightest bits glow faintly through.
    let mut out = if overcast { ((behind.0 * 0.25).min(0.2), behind.1) } else { behind };

    out = top(out, (clouds(u, v, t, c.cover, scene.part), Some(Ink::Cloud)));
    out = top(
        out,
        match c.kind {
            Kind::Rain => (rain(x, y, t, c.intensity), Some(Ink::Rain)),
            Kind::Storm => top((rain(x, y, t, 0.9), Some(Ink::Rain)), (if flash { bolt(x, y) } else { 0. }, Some(Ink::Bolt))),
            Kind::Snow => (snow(x, y, t, c.intensity), Some(Ink::Snow)),
            Kind::Fog => (fog(u, v, t), Some(Ink::Fog)),
            Kind::Clear | Kind::Clouds => (0., None),
        },
    );
    if flash && out.0 < 0.15 {
        out = (0.15, Some(Ink::Bolt));
    }
    out
}

/// A cheap integer hash for placing things.
fn hash(a: i32, b: i32) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Stars on a 16px grid, one in roughly a third of the cells; a few twinkle.
fn stars(x: f32, y: f32, t: f32, density: f32) -> f32 {
    let (cx, cy) = ((x / 16.) as i32, (y / 16.) as i32);
    let h = hash(cx, cy);
    if (h % 100) as f32 >= 33. * density {
        return 0.;
    }
    let (sx, sy) = (cx as f32 * 16. + 3. + (h >> 8) as f32 % 10., cy as f32 * 16. + 3. + (h >> 16) as f32 % 10.);
    let twinkle = if (h >> 4).is_multiple_of(4) { 0.5 + 0.5 * ((t + (h % 7) as f32 / 7.) * TAU).sin().abs() } else { 0.75 };
    if (x - sx).abs() < 2. && (y - sy).abs() < 2. { twinkle } else { 0. }
}

fn moon_at(moon: Moon, x: f32, y: f32, cx: f32, cy: f32) -> f32 {
    const R: f32 = 14.;
    moon.shade((x - cx) / R, (y - cy) / R).unwrap_or(0.)
}

/// A solid disc with eight rays turning slowly through the loop.
fn sun(x: f32, y: f32, cx: f32, cy: f32, t: f32) -> f32 {
    const R: f32 = 10.;
    let (dx, dy) = (x - cx, y - cy);
    let d = (dx * dx + dy * dy).sqrt();
    if d < R {
        return 1.;
    }
    if !(R + 4. ..R + 12.).contains(&d) {
        return 0.;
    }
    let step = TAU / 8.;
    let a = (dy.atan2(dx) - t * step).rem_euclid(step);
    if a < 0.12 || a > step - 0.12 { 0.7 } else { 0. }
}

/// Up to six lumpy clouds, more and lower as the cover grows, drifting
/// right and wrapping. Night clouds are dimmer.
fn clouds(u: f32, v: f32, t: f32, cover: f32, part: Part) -> f32 {
    const CLOUDS: [(f32, f32, f32); 6] =
        [(0.15, 0.2, 0.08), (0.6, 0.3, 0.06), (0.85, 0.14, 0.07), (0.38, 0.1, 0.08), (0.02, 0.32, 0.06), (0.72, 0.36, 0.07)];
    let n = (cover * CLOUDS.len() as f32).round() as usize;
    let ink = if part == Part::Night { 0.3 } else { 0.45 };
    CLOUDS[..n]
        .iter()
        .map(|&(u0, v0, r)| {
            // Heavier cover: bigger clouds.
            let r = r * (0.85 + cover * 0.2);
            let cu = (u0 + t * 0.2).rem_euclid(1.2) - 0.1;
            let (du, dv) = ((u - cu) * 4., v - v0);
            let lump = (du * du / (r * r * 16.) + dv * dv / (r * r)).min(
                ((u - cu - r * 0.8) * 4.).powi(2) / (r * r * 9.) + (dv + r * 0.4).powi(2) / (r * r * 0.6),
            );
            if lump < 1. { ink } else { 0. }
        })
        .fold(0., f32::max)
}

/// Slanted streaks falling twice per loop; more columns as it gets heavier.
fn rain(x: f32, y: f32, t: f32, intensity: f32) -> f32 {
    let slant = x + y * 0.35;
    let col = (slant / 8.) as i32;
    if slant.rem_euclid(8.) >= 3. {
        return 0.;
    }
    let h = hash(col, 7);
    if (h % 100) as f32 >= 20. + intensity * 60. {
        return 0.;
    }
    let y0 = ((h >> 8) % H) as f32 + t * H as f32 * 2.;
    if (y - y0).rem_euclid(H as f32 / 2.) < 14. { 0.8 } else { 0. }
}

/// Flakes swaying down one screen per loop, in three layers.
fn snow(x: f32, y: f32, t: f32, intensity: f32) -> f32 {
    let col = (x / 12.) as i32;
    (0..3)
        .map(|k| {
            let h = hash(col, k);
            if (h % 100) as f32 >= 35. + intensity * 55. {
                return 0.;
            }
            let fx = col as f32 * 12. + 6. + 3. * ((t + (h % 13) as f32 / 13.) * TAU).sin();
            let fy = (((h >> 8) % H) as f32 + t * H as f32).rem_euclid(H as f32);
            if (x - fx).abs() < 1.6 && (y - fy).abs() < 1.6 { 0.85 } else { 0. }
        })
        .fold(0., f32::max)
}

/// Long thin bands drifting sideways, thicker near the ground.
fn fog(u: f32, v: f32, t: f32) -> f32 {
    let row = (v * 16.) as i32;
    let dash = (u * 3. + t + (hash(row, 3) % 100) as f32 / 100.).fract();
    if (v * 16.).fract() < 0.35 + v * 0.3 && dash < 0.7 { 0.2 + v * 0.15 } else { 0. }
}

/// Thin horizontal glow bands above the horizon, rising through the loop.
fn bands(v: f32, ground: f32, t: f32) -> f32 {
    let above = ground - v;
    if !(0. ..0.3).contains(&above) {
        return 0.;
    }
    let phase = (above * 10. + t).fract();
    if phase < 0.15 { 0.35 * (1. - above / 0.3) } else { 0. }
}

/// A forked bolt from the cloud base to the ground, for the flash frames.
fn bolt(x: f32, y: f32) -> f32 {
    let v = y / H as f32;
    if !(0.18..0.86).contains(&v) {
        return 0.;
    }
    let path = 0.42 * W as f32 + 9. * ((v - 0.18) * 22.).sin() + 14. * (v - 0.18);
    let fork = 0.42 * W as f32 + 20. * (v - 0.45) * 3.;
    let on = |cx: f32| (x - cx).abs() < 1.8;
    if on(path) || (v > 0.45 && v < 0.7 && on(fork)) { 1. } else { 0. }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(part: Part, weather: Option<Conditions>) -> Scene {
        Scene { part, sun: 0.5, moon: Moon { age: 10. }, weather }
    }

    fn ink(p: &Picture) -> f32 {
        // Mean level of the sky band (above the hills), sampled.
        let mut sum = 0.;
        for y in 0..60 {
            for x in 0..W {
                sum += p.sample((x as f32 + 0.5) / W as f32, (y as f32 + 0.5) / H as f32);
            }
        }
        sum / (60 * W) as f32
    }

    #[test]
    fn every_scene_renders() {
        for part in [Part::Night, Part::Dawn, Part::Day, Part::Dusk] {
            for kind in [Kind::Clear, Kind::Clouds, Kind::Rain, Kind::Snow, Kind::Fog, Kind::Storm] {
                let f = frames(scene(part, Some(Conditions { kind, cover: 0.8, intensity: 0.6 })), 2);
                assert_eq!((f.frames.len(), f.masks.len()), (2, 2));
            }
        }
    }

    #[test]
    fn weather_shows_in_the_sky() {
        let clear = ink(&frame(scene(Part::Day, Some(Conditions { kind: Kind::Clear, cover: 0., intensity: 0. })), 0.).0);
        let rainy = ink(&frame(scene(Part::Day, Some(Conditions { kind: Kind::Rain, cover: 0.9, intensity: 0.9 })), 0.).0);
        assert!(rainy > clear * 2., "rain {rainy} vs clear {clear}");
    }

    #[test]
    fn every_part_is_inked_as_what_it_is() {
        let at = |scene: Scene, u: f32, v: f32| frame(scene, 0.).1.raw((u * W as f32) as u32, (v * H as f32) as u32);
        let day = scene(Part::Day, Some(Conditions { kind: Kind::Clear, cover: 0., intensity: 0. }));
        // Noon: the sun is at the top middle; the hills at the bottom.
        assert_eq!(at(day, 0.5, 0.2 + 5. / H as f32), Ink::Sun as u8);
        assert_eq!(at(day, 0.5, 0.98), Ink::Ground as u8);
        // Empty sky is unmarked.
        assert_eq!(at(day, 0.05, 0.05), 0);
        let rainy = frames(scene(Part::Day, Some(Conditions { kind: Kind::Rain, cover: 0.9, intensity: 0.9 })), 1);
        assert!(rainy.masks[0].raw(0, 0) <= Ink::Ground as u8);
        let (w, h) = rainy.masks[0].size();
        let rain = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| rainy.masks[0].raw(x, y) == Ink::Rain as u8).count();
        assert!(rain > 100, "rain is inked as rain: {rain}");
    }

    #[test]
    fn inks_are_numbered_in_table_order() {
        // natural_inks()[k - 1] is ink k; the film relies on it.
        for (i, ink) in Ink::ALL.iter().enumerate() {
            assert_eq!(*ink as usize, i + 1);
        }
        assert_eq!(Ink::ALL.len(), natural_inks(true, Part::Day).len());
    }

    #[test]
    fn natural_inks_read_on_their_page() {
        // WCAG-style contrast against the iron and paper pages.
        fn lum(hex: u32) -> f64 {
            let ch = |s: u32| {
                let c = ((hex >> s) & 0xFF) as f64 / 255.;
                if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
        }
        let contrast = |a: u32, b: u32| {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        };
        for part in [Part::Night, Part::Dawn, Part::Day, Part::Dusk] {
            for c in natural_inks(true, part) {
                assert!(contrast(c, 0x0B0B0C) >= 3., "{c:06X} on iron");
            }
            for c in natural_inks(false, part) {
                assert!(contrast(c, 0xEDEBE6) >= 3., "{c:06X} on paper");
            }
        }
    }

    #[test]
    fn sun_has_a_solid_core() {
        assert_eq!(sun(100., 40., 100., 40., 0.), 1.);
        assert_eq!(sun(200., 40., 100., 40., 0.), 0.);
    }
}
