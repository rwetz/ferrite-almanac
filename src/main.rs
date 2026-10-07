//! Desk Clock: a Ferrite clock to leave open on a second screen.
//!
//! Big block-font digits over their own dead "segments", a seconds rail,
//! the moon's phase in dither, a looping ASCII sky for the part of the
//! day, the month, and a ticker of almanac facts. With an OpenWeather key
//! the sky shows the real weather and the day follows the real sun.
//!
//!     cargo run
//!     cargo run -- --settings           # open straight into Settings
//!     OPENWEATHER_API_KEY=… cargo run   # then set a location in Settings
//!     DESK_CLOCK_SKY=snow cargo run     # preview a sky: clear clouds rain snow fog storm

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod almanac;
mod settings;
mod sky;
mod weather;

use std::rc::Rc;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDateTime, Timelike};
use ferrite_design::prelude::*;
use gpui::{
    App, AppContext as _, ClickEvent, Context, Entity, IntoElement, KeyBinding, Render, SharedString, Subscription, Task, Window, div,
    px, size,
};

use almanac::{Moon, Part};
use settings::Settings;
use weather::{Conditions, Units, Weather};

/// One font pixel of the big digits, and the gap carved out of it.
const PIXEL: f32 = 20.;
const INSET: f32 = 2.;
const FRAMES: usize = 24;
/// How often to ask for the weather, and how soon to retry after a failure.
const WEATHER_EVERY: Duration = Duration::from_secs(10 * 60);
const WEATHER_RETRY: Duration = Duration::from_secs(2 * 60);

const APPEARANCES: [(&str, &str); 3] = [("dark", "Dark"), ("light", "Light"), ("system", "System")];
const FPS: [u32; 4] = [25, 60, 120, 240];

struct DeskClock {
    now: NaiveDateTime,
    settings: Settings,
    settings_open: bool,
    ghost: Vec<(usize, usize)>,
    /// The sky film and what it was drawn for.
    sky: Rc<[dither::Picture]>,
    sky_key: Option<(Part, u32, Option<Conditions>)>,
    /// Live weather, when a key is set and the last fetch worked.
    weather: Option<Weather>,
    /// Why there's no weather, when it was asked for: setup or fetch trouble.
    weather_note: Option<String>,
    /// The polling loop; replacing it cancels the old one.
    weather_task: Option<Task<()>>,
    location: Entity<TextInput>,
    palette: Entity<CommandPalette>,
    toaster: Entity<Toaster>,
    _location: Subscription,
    _appearance: Subscription,
}

impl DeskClock {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Wake on each whole second, so the digits change with the wall clock.
        cx.spawn(async move |this, cx| {
            loop {
                let wait = 1000 - Local::now().timestamp_subsec_millis().min(999);
                cx.background_executor().timer(Duration::from_millis(wait as u64 + 5)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        let settings = Settings::load();
        let location = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).placeholder("Chicago,US or 41.88,-87.63").prompt(">");
            input.set_value(settings.location.clone(), cx);
            input
        });
        let _location = cx.subscribe(&location, |this: &mut Self, input, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Submit) {
                let value = input.read(cx).value().trim().to_string();
                this.settings.location = value;
                this.save(cx);
                this.restart_weather(cx);
            }
        });

        let now = Local::now().naive_local();
        let mut clock = Self {
            now,
            settings,
            settings_open: std::env::args().any(|a| a == "--settings"),
            ghost: almanac::ghost(5),
            sky: Rc::from(Vec::new()),
            sky_key: None,
            weather: None,
            weather_note: None,
            weather_task: None,
            location,
            palette: cx.new(|cx| CommandPalette::new(window, cx)),
            toaster: cx.new(|_| Toaster::new()),
            _location,
            _appearance: theme::follow_system(window),
        };
        clock.apply_look(window, cx);
        // Lodestone hands over its shared look as FERRITE_* variables; when
        // launched that way, those win over the saved settings.
        theme::apply_env(cx);
        clock.set_commands(cx);
        clock.restart_weather(cx);
        clock.redraw_sky();
        clock
    }

    // ── Settings ─────────────────────────────────────────────────────────

    fn apply_look(&self, window: &mut Window, cx: &mut App) {
        let s = &self.settings;
        if let Some(scheme) = schemes::by_key(&s.scheme) {
            theme::set_scheme(scheme, cx);
        }
        let appearance = match s.appearance.as_str() {
            "light" => Appearance::Light,
            "system" => Appearance::System,
            _ => Appearance::Dark,
        };
        theme::set_appearance(appearance, window, cx);
        motion::set_fps(s.fps);
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = self.settings.save() {
            self.toaster.update(cx, |t, cx| t.push(toast("Couldn't save settings").danger().message(err), cx));
        }
        cx.notify();
    }

    /// Change a setting, apply it, save it.
    fn change(&mut self, f: impl FnOnce(&mut Settings), window: &mut Window, cx: &mut Context<Self>) {
        let before = self.settings.clone();
        f(&mut self.settings);
        self.apply_look(window, cx);
        if before.units != self.settings.units {
            self.restart_weather(cx);
        }
        self.save(cx);
    }

    fn set_commands(&self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let run = |f: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            let weak = weak.clone();
            move |window: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |this, cx| f(this, window, cx));
            }
        };
        let mut commands = vec![
            command("Settings").group("Clock").icon(Icon::Sliders).shortcut("Ctrl+,").on_run(run(|this, _, cx| {
                this.settings_open = true;
                cx.notify();
            })),
            command("24-hour clock").group("Clock").icon(Icon::Calendar).on_run(run(|this, window, cx| this.change(|s| s.h24 = true, window, cx))),
            command("12-hour clock").group("Clock").icon(Icon::Calendar).on_run(run(|this, window, cx| this.change(|s| s.h24 = false, window, cx))),
            command("Refresh weather").group("Weather").icon(Icon::Refresh).on_run(run(|this, _, cx| this.restart_weather(cx))),
            command("Dark theme").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.appearance = "dark".into(), window, cx))),
            command("Light theme").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.appearance = "light".into(), window, cx))),
        ];
        for scheme in SCHEMES {
            let weak = weak.clone();
            commands.push(command(format!("Scheme: {}", scheme.name)).group("Theme").on_run(move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.change(|s| s.scheme = scheme.key.into(), window, cx));
            }));
        }
        self.palette.update(cx, |p, cx| p.set_commands(commands, cx));
    }

    // ── Weather ──────────────────────────────────────────────────────────

    /// (Re)start polling with the current settings; stops if weather is off.
    fn restart_weather(&mut self, cx: &mut Context<Self>) {
        self.weather_task = None;
        match weather::config(&self.settings.location, self.settings.units) {
            Ok(Some(config)) => {
                self.weather_note = None;
                self.weather_task = Some(cx.spawn(async move |this, cx| {
                    loop {
                        let cfg = config.clone();
                        let result = cx.background_executor().spawn(async move { weather::fetch(&cfg) }).await;
                        let ok = result.is_ok();
                        if this
                            .update(cx, |this, cx| {
                                match result {
                                    Ok(w) => {
                                        this.weather = Some(w);
                                        this.weather_note = None;
                                    }
                                    // Keep showing the last good reading through a blip.
                                    Err(why) => this.weather_note = Some(why),
                                }
                                this.redraw_sky();
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                        cx.background_executor().timer(if ok { WEATHER_EVERY } else { WEATHER_RETRY }).await;
                    }
                }));
            }
            Ok(None) => {
                self.weather = None;
                self.weather_note = None;
            }
            Err(why) => {
                self.weather = None;
                self.weather_note = Some(why);
            }
        }
        self.redraw_sky();
        cx.notify();
    }

    /// The part of the day: from the real sun when the weather knows it.
    fn part(&self) -> Part {
        match &self.weather {
            Some(w) if w.sunset > w.sunrise => Part::from_sun(Local::now().timestamp(), w.sunrise, w.sunset),
            _ => Part::of(self.now.hour()),
        }
    }

    /// How far the sun is across the sky, 0 → 1.
    fn sun_progress(&self) -> f32 {
        match &self.weather {
            Some(w) if w.sunset > w.sunrise => (Local::now().timestamp() - w.sunrise) as f32 / (w.sunset - w.sunrise) as f32,
            _ => (self.now.hour() as f32 + self.now.minute() as f32 / 60. - 7.) / 11.,
        }
    }

    fn moon(&self) -> Moon {
        Moon::at_unix(Local::now().timestamp())
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        self.now = Local::now().naive_local();
        if self.sky_key != Some(self.scene_key()) {
            self.redraw_sky();
        }
        cx.notify();
    }

    /// The weather the sky shows: live, or a `DESK_CLOCK_SKY` preview.
    fn conditions(&self) -> Option<Conditions> {
        self.weather.as_ref().map(|w| w.conditions).or_else(weather::preview)
    }

    /// The sky is redrawn when the part of the day, the hour (the sun
    /// moves) or the weather changes.
    fn scene_key(&self) -> (Part, u32, Option<Conditions>) {
        (self.part(), self.now.hour(), self.conditions())
    }

    fn redraw_sky(&mut self) {
        let scene = sky::Scene { part: self.part(), sun: self.sun_progress(), moon: self.moon(), weather: self.conditions() };
        self.sky = sky::frames(scene, FRAMES).into();
        self.sky_key = Some(self.scene_key());
    }

    // ── Views ────────────────────────────────────────────────────────────

    /// The big digits: every possible segment drawn dim, the lit ones bright.
    fn face(&self, cx: &App) -> impl IntoElement {
        let p = palette(cx);
        let lit = almanac::pixels(&almanac::face(self.now, self.settings.h24));
        // The colon blinks with the seconds, unless that's turned off.
        let colon = !self.settings.blink || self.now.second().is_multiple_of(2);
        let cell = |(x, y): (usize, usize), color| {
            div()
                .absolute()
                .left(px(x as f32 * PIXEL + INSET / 2.))
                .top(px(y as f32 * PIXEL + INSET / 2.))
                .size(px(PIXEL - INSET))
                .bg(color)
        };
        div()
            .relative()
            .flex_none()
            .w(px(almanac::face_width(5) as f32 * PIXEL))
            .h(px(5. * PIXEL))
            .children(self.ghost.iter().map(|&xy| cell(xy, hsla(p.line))))
            .children(lit.into_iter().filter(|&(x, _)| colon || !(12..17).contains(&x)).map(|xy| cell(xy, hsla(p.fg))))
    }

    /// Sixty cells: the seconds gone, the one now, the ones to come.
    fn seconds(&self, cx: &App) -> impl IntoElement {
        let p = palette(cx);
        let s = self.now.second() as usize;
        div().flex().flex_row().items_end().gap(px(2.)).children((0..60).map(|i| {
            let color = match i.cmp(&s) {
                std::cmp::Ordering::Less => p.line_strong,
                std::cmp::Ordering::Equal => p.accent,
                std::cmp::Ordering::Greater => p.line,
            };
            div().w(px(7.)).h(px(if i.is_multiple_of(5) { 12. } else { 6. })).bg(hsla(color))
        }))
    }

    fn settings_drawer(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let s = &self.settings;
        let scheme_index = SCHEMES.iter().position(|sc| sc.key == s.scheme);
        let close = {
            let weak = cx.weak_entity();
            move |_: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |this, cx| {
                    this.settings_open = false;
                    cx.notify();
                });
            }
        };
        let weather_status: SharedString = match (weather::has_key(), &self.weather, &self.weather_note) {
            (false, _, _) => "Off: no OPENWEATHER_API_KEY in the environment.".into(),
            (true, _, Some(note)) => note.clone().into(),
            (true, Some(w), None) => format!("On: {} at {}.", w.description, w.place).into(),
            (true, None, None) => "Fetching…".into(),
        };

        drawer("settings")
            .open(self.settings_open)
            .title("Settings")
            .width(px(420.))
            .on_close(close)
            .child(rule(Some("look"), window, cx))
            .child(
                field("scheme", "Scheme").child(
                    select("scheme-select")
                        .options(SCHEMES.iter().map(|sc| sc.name))
                        .selected(scheme_index)
                        .width(px(220.))
                        .on_change(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.scheme = SCHEMES[*i].key.into(), window, cx))),
                ),
            )
            .child(
                field("appearance", "Appearance").child(
                    APPEARANCES.iter().fold(segmented("appearance-seg"), |seg, (_, label)| seg.option(*label))
                        .selected(APPEARANCES.iter().position(|(k, _)| *k == s.appearance).unwrap_or(0))
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.appearance = APPEARANCES[*i].0.into(), window, cx))),
                ),
            )
            .child(
                field("fps", "Refresh rate").hint("25 is the classic stepped look").child(
                    FPS.iter().fold(segmented("fps-seg"), |seg, f| seg.option(f.to_string()))
                        .selected(FPS.iter().position(|f| *f == s.fps).unwrap_or(FPS.len() - 1))
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.fps = FPS[*i], window, cx))),
                ),
            )
            .child(rule(Some("clock"), window, cx))
            .child(
                field("hours", "Hours").child(
                    segmented("hours-seg")
                        .option("24-hour")
                        .option("12-hour")
                        .selected(if s.h24 { 0 } else { 1 })
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.h24 = *i == 0, window, cx))),
                ),
            )
            .child(
                switch("blink")
                    .label("Blinking colon")
                    .checked(s.blink)
                    .on_change(cx.listener(|this, on: &bool, window, cx| this.change(|s| s.blink = *on, window, cx))),
            )
            .child(
                switch("seconds")
                    .label("Seconds rail")
                    .checked(s.seconds)
                    .on_change(cx.listener(|this, on: &bool, window, cx| this.change(|s| s.seconds = *on, window, cx))),
            )
            .child(rule(Some("weather"), window, cx))
            .child(div().body(text::SM).text_color(hsla(p.fg_dim)).child(weather_status))
            .child(field("location", "Location").hint("Press Enter to apply").stacked().child(self.location.clone()))
            .child(
                field("units", "Units").child(
                    segmented("units-seg")
                        .option("Metric")
                        .option("Imperial")
                        .selected(if s.units == Units::Imperial { 1 } else { 0 })
                        .on_select(cx.listener(|this, i: &usize, window, cx| {
                            this.change(|s| s.units = if *i == 1 { Units::Imperial } else { Units::Metric }, window, cx)
                        })),
                ),
            )
    }
}

impl Render for DeskClock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let now = self.now;
        let moon = self.moon();
        let part = self.part();

        let mut readout = div().flex().flex_row().items_end().gap_4().child(self.face(cx));
        if !self.settings.h24 {
            readout = readout.child(div().display(Scale::X2, window).text_color(hsla(p.fg_dim)).child(if now.hour() < 12 { "AM" } else { "PM" }));
        }
        let offset = Local::now().format("%:z").to_string();
        let clock = panel("Now").meta(format!("UTC{offset}")).flex_1().min_w_0().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_4()
                .py_4()
                .child(readout)
                .when(self.settings.seconds, |col| col.child(self.seconds(cx)))
                .child(div().display(Scale::X1, window).text_color(hsla(p.fg_dim)).child(now.format("%A · %d %b %Y").to_string().to_uppercase())),
        );

        let disc = dither::Picture::from_fn(64, 64, move |u, v| moon.shade(u * 2. - 1., v * 2. - 1.).unwrap_or(0.));
        let moon_panel = panel("Moon").meta(format!("{:.0}%", moon.illumination() * 100.)).w(px(300.)).child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .py_2()
                .child(develop("moon", moon.name(), dither(disc).ink(hsla(p.fg)).size(px(128.))))
                .child(div().display(Scale::X1, window).child(moon.name().to_uppercase()))
                .child(div().body(text::SM).text_color(hsla(p.fg_dim)).child(match moon.days_to_full() {
                    0 => "full tonight".to_string(),
                    1 => "full tomorrow".to_string(),
                    n => format!("full in {n} days"),
                })),
        );

        let sky_meta = match &self.weather {
            Some(w) => format!("{} · {:.0}{}", part.label(), w.temp, w.units.temp()),
            None => part.label().to_string(),
        };
        let report: Option<(bool, String)> = match (&self.weather, &self.weather_note) {
            (Some(w), note) => Some((
                note.is_some(),
                format!(
                    "{} · {} · feels {:.0}{} · wind {:.0} {} · {}% humidity",
                    w.place,
                    w.description,
                    w.feels,
                    w.units.temp(),
                    w.wind,
                    w.units.speed(),
                    w.humidity
                ),
            )),
            (None, Some(note)) => Some((true, format!("weather: {note}"))),
            (None, None) => None,
        };
        let (charset, fit) = sky::style(self.conditions());
        let sky_panel = panel("Sky")
            .meta(sky_meta)
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .justify_center()
                    .overflow_hidden()
                    .child(ascii_film("sky", self.sky.clone()).cols(72).charset(charset).fit(fit).fps(6).color(hsla(p.fg_dim))),
            )
            .when_some(report, |panel, (warn, line)| {
                panel.child(div().flex_1()).child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .body(text::SM)
                        .text_color(hsla(p.fg_dim))
                        .when(warn, |row| row.child(icon(Icon::Warning).fit(px(14.)).color(hsla(p.warning))))
                        .child(line),
                )
            });
        let month = panel(now.format("%B").to_string()).meta(now.year().to_string()).w(px(300.)).child(
            div().flex().justify_center().child(ascii_cal(now.year(), now.month()).today(Some(now.day()))),
        );

        let gear = Button::new("open-settings").icon(Icon::Sliders).ghost().small().tooltip("Settings · Ctrl+,").on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.settings_open = !this.settings_open;
                cx.notify();
            },
        ));
        let drawer = self.settings_drawer(window, cx);

        window_frame().child(power_on_in(
            "power",
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(hsla(p.bg))
                .text_color(hsla(p.fg))
                .body(text::BASE)
                .on_action(cx.listener(|this, _: &TogglePalette, window, cx| this.palette.update(cx, |p, cx| p.toggle(window, cx))))
                .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                    this.settings_open = true;
                    cx.notify();
                }))
                .child(self.palette.clone())
                .child(self.toaster.clone())
                .child(title_bar("Desk Clock").child(gear))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .gap(space::ROW)
                        .p(space::ROW)
                        .child(div().flex().flex_row().gap(space::ROW).child(clock).child(moon_panel))
                        .child(div().flex().flex_row().flex_1().min_h_0().gap(space::ROW).child(sky_panel).child(month))
                        .child(div().flex().justify_center().child(marquee("ticker", almanac::ticker(now, moon)).cells(110).color(hsla(p.fg_dim)))),
                )
                .child(drawer)
                .child(
                    status_bar()
                        .left(if self.settings.h24 { "24H" } else { "12H" })
                        .left_live(now.format("%H:%M:%S").to_string())
                        .right(theme::scheme(cx).name.to_uppercase())
                        .right_live(format!("{}FPS", motion::fps())),
                ),
        ))
    }
}

gpui::actions!(desk_clock, [OpenSettings]);

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        ferrite_design::init(Appearance::Dark, cx);
        cx.bind_keys([KeyBinding::new("ctrl-shift-p", TogglePalette, None), KeyBinding::new("ctrl-,", OpenSettings, None)]);
        let options = chrome::window_options("Desk Clock", size(px(1120.), px(760.)), cx);
        cx.open_window(options, |window, cx| {
            chrome::square_corners(window);
            chrome::power_off_on_close(window, cx);
            cx.new(|cx| DeskClock::new(window, cx))
        })
        .expect("failed to open the window");
        cx.activate(true);
    });
}
