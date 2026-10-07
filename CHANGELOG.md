# Changelog

All notable changes to Almanac are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

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

[0.1.0]: https://github.com/rwetz/ferrite-almanac/releases/tag/v0.1.0
