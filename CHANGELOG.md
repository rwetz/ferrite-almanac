# Changelog

All notable changes to Almanac are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.3.0] - 2026-10-09

### Changed

- Replace OpenWeather with key-free Open-Meteo weather. Restore saved scheme and appearance at launch.
- Include current Ferrite layout, window memory, and overlay fixes.

## [0.2.0] - 2026-10-07

### Added

- The sky in colour: a yellow sun, white clouds and moon, blue rain, green
  hills, an orange glow at dawn and dusk. Fixed natural colours, tuned for
  dark and light pages; Settings → "Sky in colour" turns it back to the
  scheme's one ink.
- The window reopens at the size, place and state (maximized, fullscreen)
  it was closed in.

### Fixed

- Maximized or fullscreen, the clock no longer sits small in an empty
  window: the digits, seconds rail and date grow with it, the sky fills its
  panel, and the ticker spans the width.
- The first window fits the screen (it was cut off on small or scaled
  displays).
- The scheme picker in Settings opens (its menu was drawn under the
  drawer; fixed in ferrite-design).

## [0.1.1] - 2026-10-07

### Fixed

- Windows: the Settings button in the title bar does something when clicked. The
  whole title bar was a window-drag area, so Windows took the click as the
  start of a drag (ferrite-design's `title_bar`).
- The date rolls over at local midnight, not UTC midnight.

## [0.1.0] - 2026-10-07

The first release.

### Added

- A block-digit clock over its own dim segments, with a seconds rail, a
  blinking colon and a 12/24-hour switch.
- The moon's phase in dither, a month calendar, and a ticker of almanac facts.
- A looping ASCII sky for the part of the day; with an OpenWeather key it shows
  the real weather and follows the real sunrise and sunset.
- A Settings drawer for scheme, appearance, refresh rate, location, units and
  the OpenWeather key. The key is saved on its own in `ferrite/almanac.key`
  (owner-only on Unix); `OPENWEATHER_API_KEY` still wins when set. Settings
  open by themselves on first launch.
- The shared look from Lodestone (`FERRITE_*` variables) when launched from it.
- The pixel-art logo as the Windows executable, window and taskbar icon.

[0.1.1]: https://github.com/rwetz/ferrite-almanac/releases/tag/v0.1.1
[0.1.0]: https://github.com/rwetz/ferrite-almanac/releases/tag/v0.1.0
