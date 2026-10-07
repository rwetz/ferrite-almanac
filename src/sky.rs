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

pub fn frames(scene: Scene, n: usize) -> Vec<Picture> {
    (0..n).map(|f| frame(scene, f as f32 / n as f32)).collect()
}

fn frame(scene: Scene, t: f32) -> Picture {
    let c = scene.conditions();
    let overcast = c.cover > 0.75 || matches!(c.kind, Kind::Rain | Kind::Storm | Kind::Fog);
    let flash = c.kind == Kind::Storm && (0.5..0.55).contains(&t);
    Picture::from_fn(W, H, move |u, v| {
        let (x, y) = (u * W as f32, v * H as f32);
        let ground = 0.84 + 0.05 * (u * 9. + 1.).sin() + 0.025 * (u * 23.).sin();
        if v > ground {
            // Snow settles on the hills.
            return match (c.kind, scene.part) {
                (Kind::Snow, _) => 0.62,
                (_, Part::Day) => 0.38,
                _ => 0.24,
            };
        }

        // The sky behind the weather: sun, moon, stars, twilight glow.
        let behind = match scene.part {
            Part::Night => stars(x, y, t, 1.).max(moon_at(scene.moon, x, y, 0.8 * W as f32, 0.3 * H as f32)),
            Part::Day => {
                let s = scene.sun.clamp(0., 1.);
                sun(x, y, (0.1 + 0.8 * s) * W as f32, (0.75 - 0.55 * (s * PI).sin()) * H as f32, t)
            }
            Part::Dawn | Part::Dusk => {
                let cx = if scene.part == Part::Dawn { 0.22 } else { 0.78 } * W as f32;
                let high_stars = if v < 0.35 { stars(x, y, t, 0.35) } else { 0. };
                sun(x, y, cx, ground * H as f32 - 6., t).max(bands(v, ground, t)).max(high_stars)
            }
        };
        // Heavy weather hides it; the brightest bits glow faintly through.
        let mut level = if overcast { (behind * 0.25).min(0.2) } else { behind };

        level = level.max(clouds(u, v, t, c.cover, scene.part));
        level = level.max(match c.kind {
            Kind::Rain => rain(x, y, t, c.intensity),
            Kind::Storm => rain(x, y, t, 0.9).max(if flash { bolt(x, y) } else { 0. }),
            Kind::Snow => snow(x, y, t, c.intensity),
            Kind::Fog => fog(u, v, t),
            Kind::Clear | Kind::Clouds => 0.,
        });
        if flash {
            level = level.max(0.15);
        }
        level
    })
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
                assert_eq!(f.len(), 2);
            }
        }
    }

    #[test]
    fn weather_shows_in_the_sky() {
        let clear = ink(&frame(scene(Part::Day, Some(Conditions { kind: Kind::Clear, cover: 0., intensity: 0. })), 0.));
        let rainy = ink(&frame(scene(Part::Day, Some(Conditions { kind: Kind::Rain, cover: 0.9, intensity: 0.9 })), 0.));
        assert!(rainy > clear * 2., "rain {rainy} vs clear {clear}");
    }

    #[test]
    fn sun_has_a_solid_core() {
        assert_eq!(sun(100., 40., 100., 40., 0.), 1.);
        assert_eq!(sun(200., 40., 100., 40., 0.), 0.);
    }
}
