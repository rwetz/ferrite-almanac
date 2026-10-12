//! The little sky over the clock: a looping picture for the part of the
//! day and the weather, rendered as ASCII by `ascii_film`. Frames are cheap
//! to make and are rebuilt only when the scene changes (hourly, or when new
//! weather arrives). Every moving thing shifts by a whole period over the
//! loop, so the film never jumps.
//!
//! There is one picture per kind of sky ([`Screen`]): sunny, partly sunny,
//! cloudy, rain, storm, snow, blizzard, fog, and two clear nights — one with
//! the moon up at its phase, one without.

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

/// Below this much lit disc the moon doesn't count as up.
const MOON_MIN: f32 = 0.04;
/// Cloud cover at which clear becomes partly sunny, and at which the sun
/// goes in for good.
const PARTLY: f32 = 0.25;
const CLOUDY: f32 = 0.7;

/// Everything a frame depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub part: Part,
    /// How far the sun is across the day, 0 (rise) → 1 (set).
    pub sun: f32,
    pub moon: Moon,
    /// Local time of day in hours, 0–24: where the moon is.
    pub hour: f32,
    /// Live weather, or `None` for the clock's own fair-weather sky.
    pub weather: Option<Conditions>,
}

impl Scene {
    /// What to draw: the live weather, or a fair sky.
    fn conditions(&self) -> Conditions {
        self.weather.unwrap_or(Conditions { kind: Kind::Clear, cover: 0.1, intensity: 0. })
    }

    /// Where the moon is in the frame (`u`, `v`), when it's up and lit
    /// enough to see. It crosses the meridian about 50 minutes later each
    /// day — at noon when new, at midnight when full — and is on the
    /// horizon six hours either side.
    pub fn moon_pos(&self) -> Option<(f32, f32)> {
        if (self.moon.illumination() as f32) < MOON_MIN {
            return None;
        }
        let transit = 12. + 24. * self.moon.fraction() as f32;
        let ha = (self.hour - transit + 12.).rem_euclid(24.) - 12.;
        if ha.abs() >= 6. {
            return None;
        }
        let alt = (ha / 6. * PI / 2.).cos();
        Some((0.5 + ha / 6. * 0.42, 0.74 - 0.52 * alt))
    }
}

/// Which picture the sky is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// The sun, and nothing in its way.
    Sunny,
    /// The sun between a few drifting clouds that pass over it.
    PartlySunny,
    /// A low grey ceiling and scud under it; no sun at all.
    Cloudy,
    Rain,
    Storm,
    Snow,
    Blizzard,
    Fog,
    /// A clear night with the moon up, drawn at its phase.
    Moonlit,
    /// A clear night with no moon: more stars, the Milky Way, a meteor.
    Moonless,
}

impl Screen {
    pub fn of(scene: &Scene) -> Screen {
        let c = scene.conditions();
        match c.kind {
            Kind::Rain => Screen::Rain,
            Kind::Storm => Screen::Storm,
            Kind::Snow => Screen::Snow,
            Kind::Blizzard => Screen::Blizzard,
            Kind::Fog => Screen::Fog,
            Kind::Clear | Kind::Clouds if c.cover >= CLOUDY => Screen::Cloudy,
            _ if scene.part == Part::Night => {
                if scene.moon_pos().is_some() { Screen::Moonlit } else { Screen::Moonless }
            }
            _ if c.cover >= PARTLY => Screen::PartlySunny,
            _ => Screen::Sunny,
        }
    }

    /// For the panel's header.
    pub fn label(self, part: Part) -> &'static str {
        let day = part == Part::Day;
        match self {
            Screen::Sunny if day => "SUNNY",
            Screen::Sunny => "CLEAR",
            Screen::PartlySunny if day => "PARTLY SUNNY",
            Screen::PartlySunny => "PARTLY CLOUDY",
            Screen::Cloudy => "CLOUDY",
            Screen::Rain => "RAIN",
            Screen::Storm => "STORM",
            Screen::Snow => "SNOW",
            Screen::Blizzard => "BLIZZARD",
            Screen::Fog => "FOG",
            Screen::Moonlit => "MOONLIT",
            Screen::Moonless => "MOONLESS",
        }
    }

    /// For weather that covers the whole sky: how low the cloud ceiling
    /// reaches (0 top → 1 bottom) and how dense it is.
    fn deck(self) -> Option<(f32, f32)> {
        match self {
            Screen::Cloudy => Some((0.44, 0.3)),
            Screen::Rain => Some((0.4, 0.38)),
            Screen::Storm => Some((0.48, 0.46)),
            Screen::Snow => Some((0.34, 0.3)),
            Screen::Blizzard => Some((0.5, 0.34)),
            _ => None,
        }
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
    /// The grey ceiling of a cloudy, rainy or snowy sky.
    Overcast,
}

#[cfg_attr(not(test), allow(dead_code))]
impl Ink {
    /// In colour-table order (only the tests walk it).
    pub const ALL: [Ink; 11] = [
        Ink::Sun,
        Ink::Moon,
        Ink::Star,
        Ink::Glow,
        Ink::Cloud,
        Ink::Rain,
        Ink::Snow,
        Ink::Fog,
        Ink::Bolt,
        Ink::Ground,
        Ink::Overcast,
    ];
}

/// Natural colours for each [`Ink`] (in [`Ink::ALL`] order), as `0xRRGGBB`,
/// for a dark page or a light one. The sky is an illustration, like a
/// photo: these stay put whatever the scheme (the setting's "off" gives
/// back the scheme's own ink). On paper every ink is darker, so it still
/// reads; at night the hills go dark.
pub fn natural_inks(dark: bool, part: Part) -> [u32; 11] {
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
            if night { 0x6E7480 } else { 0x9AA0A8 },    // overcast: dreary grey
        ]
    } else {
        [
            0xA87A00, 0x55555A, 0x6B7385, 0xB5561A,
            if night { 0x55555A } else { 0x7A808A },
            0x1F62B0, 0x5B7A9C, 0x7A828C, 0x8A6A00,
            if night { 0x2D5A28 } else { 0x3A7A2C },
            if night { 0x50545C } else { 0x666C76 },
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

type Layer = (f32, Option<Ink>);

/// Keep the brighter of two layers, and whose it is.
fn top((a, ia): Layer, (b, ib): Layer) -> Layer {
    if b > a { (b, ib) } else { (a, ia) }
}

/// The level at `u`, `v`, and what put it there: the layer that wins.
fn point(scene: Scene, t: f32, u: f32, v: f32) -> Layer {
    let screen = Screen::of(&scene);
    let c = scene.conditions();
    let night = scene.part == Part::Night;
    let (x, y) = (u * W as f32, v * H as f32);
    let ground = 0.84 + 0.05 * (u * 9. + 1.).sin() + 0.025 * (u * 23.).sin();
    let strike = if screen == Screen::Storm { strike(t) } else { None };
    if v > ground {
        let lit = if night { 0.24 } else { 0.38 };
        return match screen {
            // Snow settles on the hills.
            Screen::Snow | Screen::Blizzard => (0.62, Some(Ink::Snow)),
            Screen::Rain | Screen::Storm if splash(x, y, t, ground * H as f32, c.intensity) > 0. => (0.8, Some(Ink::Rain)),
            _ if strike.is_some() => (0.55, Some(Ink::Ground)),
            Screen::Cloudy | Screen::Rain | Screen::Storm | Screen::Fog => (lit * 0.75, Some(Ink::Ground)),
            _ => (lit, Some(Ink::Ground)),
        };
    }

    // Behind the weather: sun, moon, stars, twilight glow. A ceiling of
    // cloud hides all of it; fog lets the brightest bits glow through.
    let behind = match screen {
        Screen::Sunny | Screen::PartlySunny | Screen::Moonlit | Screen::Moonless => heavens(scene, x, y, v, ground, t),
        Screen::Fog => {
            let b = heavens(scene, x, y, v, ground, t);
            ((b.0 * 0.25).min(0.2), b.1)
        }
        _ => (0., None),
    };

    // Clouds are solid: they pass in front of the sun and the moon.
    let dim = if night { 0.6 } else { 1. };
    let cloud: Layer = match screen.deck() {
        Some((base, level)) => {
            let flash = if strike.is_some() { 0.35 } else { 0. };
            let ceiling = deck(u, v, t, base, level * dim + flash);
            if ceiling > 0. {
                (ceiling, Some(Ink::Overcast))
            } else if screen == Screen::Cloudy && clouds(u, v - 0.3, t, 0.34, scene.part) > 0. {
                // Ragged scud hurrying along under the ceiling.
                ((level + 0.12) * dim, Some(Ink::Overcast))
            } else {
                (0., None)
            }
        }
        None => match screen {
            Screen::PartlySunny => (clouds(u, v, t, c.cover.clamp(0.34, 0.67), scene.part), Some(Ink::Cloud)),
            Screen::Moonlit | Screen::Moonless => (clouds(u, v, t, c.cover.min(0.67), scene.part), Some(Ink::Cloud)),
            _ => (0., None),
        },
    };
    let clear = cloud.0 <= 0.;
    let mut out = if clear { behind } else { cloud };

    // What falls out of it, from the cloud base down.
    out = match screen {
        Screen::Rain if clear => top(out, (rain(x, y, t, c.intensity), Some(Ink::Rain))),
        Screen::Storm => {
            let out = if clear { top(out, (rain(x, y, t, c.intensity.max(0.85)), Some(Ink::Rain))) } else { out };
            match strike {
                Some(at) => top(out, (bolt(x, y, at * W as f32), Some(Ink::Bolt))),
                None => out,
            }
        }
        Screen::Snow if clear => top(out, (snow(x, y, t, c.intensity), Some(Ink::Snow))),
        Screen::Blizzard => {
            let out = top(out, (gale(x, y, t), Some(Ink::Snow)));
            let out = if clear { top(out, (snow(x, y, t, 1.), Some(Ink::Snow))) } else { out };
            // Whiteout: a haze over everything.
            if out.0 < 0.12 { (0.12, Some(Ink::Fog)) } else { out }
        }
        Screen::Fog => top(out, (fog(u, v, t), Some(Ink::Fog))),
        _ => out,
    };
    if strike.is_some() && out.0 < 0.15 {
        out = (0.15, Some(Ink::Bolt));
    }
    out
}

/// The sky with nothing in front of it.
fn heavens(scene: Scene, x: f32, y: f32, v: f32, ground: f32, t: f32) -> Layer {
    match scene.part {
        Part::Night => match scene.moon_pos() {
            Some((mu, mv)) => {
                // A bright moon washes the faint stars out and wears a halo.
                let lit = scene.moon.illumination() as f32;
                let (cx, cy) = (mu * W as f32, mv * H as f32);
                [(moon_at(scene.moon, x, y, cx, cy), Some(Ink::Moon)), (halo(x, y, cx, cy, lit), Some(Ink::Moon))]
                    .into_iter()
                    .fold((stars(x, y, t, 1. - 0.6 * lit), Some(Ink::Star)), top)
            }
            None => [(milky_way(x, y), Some(Ink::Star)), (meteor(x, y, t), Some(Ink::Star))]
                .into_iter()
                .fold((stars(x, y, t, 1.3), Some(Ink::Star)), top),
        },
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
    }
}

/// A cheap integer hash for placing things.
fn hash(a: i32, b: i32) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Stars on a 16px grid, one in roughly a third of the cells (at density
/// 1); a few twinkle.
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

const MOON_R: f32 = 16.;

fn moon_at(moon: Moon, x: f32, y: f32, cx: f32, cy: f32) -> f32 {
    moon.shade((x - cx) / MOON_R, (y - cy) / MOON_R).unwrap_or(0.)
}

/// A dotted glow around the moon, as wide and bright as it is full.
fn halo(x: f32, y: f32, cx: f32, cy: f32, lit: f32) -> f32 {
    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    let reach = MOON_R * (1.4 + lit);
    if d <= MOON_R + 2. || d >= MOON_R + reach || (x as i32 + 2 * y as i32) % 4 != 0 {
        return 0.;
    }
    0.35 * lit * (1. - (d - MOON_R) / reach)
}

/// A diagonal river of faint, close-packed stars.
fn milky_way(x: f32, y: f32) -> f32 {
    let (u, v) = (x / W as f32, y / H as f32);
    let d = (v - (0.08 + 0.5 * u)).abs();
    if d >= 0.12 {
        return 0.;
    }
    let (cx, cy) = ((x / 5.) as i32, (y / 5.) as i32);
    let h = hash(cx, cy + 1000);
    if (h % 100) as f32 >= 45. * (1. - d / 0.12) {
        return 0.;
    }
    let (sx, sy) = (cx as f32 * 5. + 1. + (h >> 8) as f32 % 3., cy as f32 * 5. + 1. + (h >> 16) as f32 % 3.);
    if (x - sx).abs() < 1. && (y - sy).abs() < 1. { 0.4 } else { 0. }
}

/// One shooting star a loop, streaking down and to the right.
fn meteor(x: f32, y: f32, t: f32) -> f32 {
    const START: f32 = 0.55;
    const SPAN: f32 = 0.15;
    const TAIL: f32 = 28.;
    if !(START..START + SPAN).contains(&t) {
        return 0.;
    }
    let p = (t - START) / SPAN;
    let (dx, dy) = (0.5 * W as f32, 0.3 * H as f32);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = (dx / len, dy / len);
    let (hx, hy) = (0.15 * W as f32 + p * dx, 0.08 * H as f32 + p * dy);
    let (rx, ry) = (hx - x, hy - y);
    let back = rx * ux + ry * uy;
    let off = (rx * uy - ry * ux).abs();
    if (0. ..TAIL).contains(&back) && off < 1.2 { 0.9 * (1. - back / TAIL) } else { 0. }
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

/// A ceiling of cloud across the whole sky, down to about `base`, its
/// ragged underside rolling by. Long streaks drift through it so the grey
/// isn't flat, and the belly is heavier.
fn deck(u: f32, v: f32, t: f32, base: f32, level: f32) -> f32 {
    let edge = base + 0.035 * ((u * 5. - t) * TAU).sin() + 0.02 * ((u * 13. + 2. * t) * TAU).sin();
    if v > edge {
        return 0.;
    }
    let streak = ((u * 4. - t) * TAU + v * 37.).sin();
    let belly = if edge - v < 0.06 { 0.08 } else { 0. };
    (level + 0.06 * streak.signum() + belly).max(0.05)
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

/// Drops landing on the hills: a ring spreading and gone, twice a loop per
/// spot; more spots the harder it rains.
fn splash(x: f32, y: f32, t: f32, ground_y: f32, intensity: f32) -> f32 {
    let col = (x / 10.) as i32;
    let h = hash(col, 11);
    if (h % 100) as f32 >= 25. + intensity * 50. {
        return 0.;
    }
    let phase = (t * 2. + ((h >> 8) % 100) as f32 / 100.).fract();
    let cx = col as f32 * 10. + 5.;
    let spread = 1. + phase * 10.;
    if phase < 0.18 && y - ground_y < 2.5 && ((x - cx).abs() - spread * 0.5).abs() < 1.2 { 0.8 } else { 0. }
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

/// Snow driven flat by the wind: streaks racing across in rows, each row
/// crossing the frame a whole number of times a loop.
fn gale(x: f32, y: f32, t: f32) -> f32 {
    let row = (y / 5.) as i32;
    if y.rem_euclid(5.) >= 1.5 {
        return 0.;
    }
    let h = hash(row, 21);
    if h % 100 >= 70 {
        return 0.;
    }
    let speed = (2 + (h >> 4) % 2) as f32;
    let len = (6 + (h >> 8) % 11) as f32;
    let s = (x - t * W as f32 * speed + ((h >> 12) % 40) as f32).rem_euclid(40.);
    if s < len { 0.85 } else { 0. }
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

/// Where lightning strikes, as a share of the width, on its flash frames:
/// twice a loop, in different places. Each window holds one of 24 frames.
fn strike(t: f32) -> Option<f32> {
    if (0.32..0.35).contains(&t) {
        Some(0.3)
    } else if (0.74..0.77).contains(&t) {
        Some(0.66)
    } else {
        None
    }
}

/// A forked bolt from the cloud base to the ground at `cx`.
fn bolt(x: f32, y: f32, cx: f32) -> f32 {
    let v = y / H as f32;
    if !(0.3..0.86).contains(&v) {
        return 0.;
    }
    let path = cx + 9. * ((v - 0.3) * 22.).sin() + 14. * (v - 0.3);
    let fork = cx + 20. * (v - 0.5) * 3.;
    let on = |cx: f32| (x - cx).abs() < 1.8;
    if on(path) || (v > 0.5 && v < 0.72 && on(fork)) { 1. } else { 0. }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(part: Part, weather: Option<Conditions>) -> Scene {
        Scene { part, sun: 0.5, moon: Moon { age: 10. }, hour: 12., weather }
    }

    fn sky(kind: Kind, cover: f32) -> Option<Conditions> {
        Some(Conditions { kind, cover, intensity: 0.6 })
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

    fn count(p: &Picture, ink: Ink) -> usize {
        let (w, h) = p.size();
        (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| p.raw(x, y) == ink as u8).count()
    }

    #[test]
    fn every_scene_renders() {
        for part in [Part::Night, Part::Dawn, Part::Day, Part::Dusk] {
            for kind in [Kind::Clear, Kind::Clouds, Kind::Rain, Kind::Snow, Kind::Blizzard, Kind::Fog, Kind::Storm] {
                let f = frames(scene(part, sky(kind, 0.8)), 2);
                assert_eq!((f.frames.len(), f.masks.len()), (2, 2));
            }
        }
    }

    #[test]
    fn each_sky_gets_its_screen() {
        let day = |kind, cover| Screen::of(&scene(Part::Day, sky(kind, cover)));
        assert_eq!(day(Kind::Clear, 0.05), Screen::Sunny);
        assert_eq!(day(Kind::Clouds, 0.4), Screen::PartlySunny);
        // The code says "mainly clear" but the sky is grey: the sky wins.
        assert_eq!(day(Kind::Clouds, 0.93), Screen::Cloudy);
        assert_eq!(day(Kind::Rain, 0.2), Screen::Rain);
        assert_eq!(day(Kind::Storm, 1.), Screen::Storm);
        assert_eq!(day(Kind::Snow, 1.), Screen::Snow);
        assert_eq!(day(Kind::Blizzard, 1.), Screen::Blizzard);
        assert_eq!(day(Kind::Fog, 1.), Screen::Fog);
        // No live weather: a fair sky.
        assert_eq!(Screen::of(&scene(Part::Day, None)), Screen::Sunny);
    }

    #[test]
    fn the_moon_rises_and_sets_with_its_phase() {
        let night = |age: f64, hour: f32| Scene { part: Part::Night, sun: 0., moon: Moon { age }, hour, weather: None };
        let full = crate::almanac::SYNODIC / 2.;
        // Full moon: up all night, highest at midnight.
        assert_eq!(Screen::of(&night(full, 0.)), Screen::Moonlit);
        assert_eq!(Screen::of(&night(full, 22.)), Screen::Moonlit);
        let (_, v) = night(full, 0.).moon_pos().unwrap();
        assert!(v < 0.3, "full moon high at midnight: {v}");
        // New moon: nothing to see.
        assert_eq!(Screen::of(&night(0.5, 0.)), Screen::Moonless);
        // First quarter sets around midnight: up in the evening, gone at 3.
        let quarter = crate::almanac::SYNODIC / 4.;
        assert_eq!(Screen::of(&night(quarter, 20.)), Screen::Moonlit);
        assert_eq!(Screen::of(&night(quarter, 3.)), Screen::Moonless);
        // Rising in the east (left), setting in the west (right).
        let (rising, _) = night(full, 20.).moon_pos().unwrap();
        let (setting, _) = night(full, 4.).moon_pos().unwrap();
        assert!(rising < 0.5 && setting > 0.5);
    }

    #[test]
    fn weather_shows_in_the_sky() {
        let clear = ink(&frame(scene(Part::Day, sky(Kind::Clear, 0.)), 0.).0);
        let rainy = ink(&frame(scene(Part::Day, Some(Conditions { kind: Kind::Rain, cover: 0.9, intensity: 0.9 })), 0.).0);
        assert!(rainy > clear * 2., "rain {rainy} vs clear {clear}");
    }

    #[test]
    fn cloudy_hides_the_sun_and_sunny_has_no_cloud() {
        let sunny = frame(scene(Part::Day, sky(Kind::Clear, 0.1)), 0.).1;
        assert!(count(&sunny, Ink::Sun) > 100);
        assert_eq!(count(&sunny, Ink::Cloud) + count(&sunny, Ink::Overcast), 0);
        let cloudy = frame(scene(Part::Day, sky(Kind::Clouds, 0.93)), 0.).1;
        assert_eq!(count(&cloudy, Ink::Sun), 0);
        // The ceiling covers well over a third of the frame.
        assert!(count(&cloudy, Ink::Overcast) > (W * H / 3) as usize);
        let partly = frame(scene(Part::Day, sky(Kind::Clouds, 0.45)), 0.).1;
        assert!(count(&partly, Ink::Sun) > 0 && count(&partly, Ink::Cloud) > 0);
    }

    #[test]
    fn storms_strike_and_blizzards_blow() {
        let storm = scene(Part::Day, sky(Kind::Storm, 1.));
        let flashes = (0..24).filter(|&f| count(&frame(storm, f as f32 / 24.).1, Ink::Bolt) > 0).count();
        assert_eq!(flashes, 2, "two strikes a loop");
        let blizzard = frame(scene(Part::Day, sky(Kind::Blizzard, 1.)), 0.).1;
        let snow = frame(scene(Part::Day, sky(Kind::Snow, 1.)), 0.).1;
        assert!(count(&blizzard, Ink::Snow) > count(&snow, Ink::Snow));
    }

    #[test]
    fn moonless_nights_show_more_stars() {
        let at = |age: f64| Scene { part: Part::Night, sun: 0., moon: Moon { age }, hour: 0., weather: None };
        let moonless = frame(at(0.5), 0.).1;
        let full = frame(at(crate::almanac::SYNODIC / 2.), 0.).1;
        assert!(count(&full, Ink::Moon) > 0);
        assert_eq!(count(&moonless, Ink::Moon), 0);
        assert!(count(&moonless, Ink::Star) > count(&full, Ink::Star));
    }

    #[test]
    fn every_part_is_inked_as_what_it_is() {
        let at = |scene: Scene, u: f32, v: f32| frame(scene, 0.).1.raw((u * W as f32) as u32, (v * H as f32) as u32);
        let day = scene(Part::Day, sky(Kind::Clear, 0.));
        // Noon: the sun is at the top middle; the hills at the bottom.
        assert_eq!(at(day, 0.5, 0.2 + 5. / H as f32), Ink::Sun as u8);
        assert_eq!(at(day, 0.5, 0.98), Ink::Ground as u8);
        // Empty sky is unmarked.
        assert_eq!(at(day, 0.05, 0.05), 0);
        let rainy = frames(scene(Part::Day, Some(Conditions { kind: Kind::Rain, cover: 0.9, intensity: 0.9 })), 1);
        assert!(rainy.masks[0].raw(0, 0) <= Ink::Overcast as u8);
        assert!(count(&rainy.masks[0], Ink::Rain) > 100, "rain is inked as rain");
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
