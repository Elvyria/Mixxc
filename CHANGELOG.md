# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.2] - 2024-04-13

### Added
- Added a man page.
- Vertical orientation for volume sliders is now available via `-b v` or `--bar vertical`.
- Added a short flag for `--max-volume` -> `-x`.
- Added an optional master slider for current device volume under `-M`, `--master` flags.

### Fixed
- (CSS) Icon wasn't affected by style changes in `default.css`, because class selector was invalid.
- (CSS) Name and description used white color instead of the foreground.
- Peakers will always start unmuted, in case something forced them to mute and state was saved by audio server.
- Experimental fix for X when window flickers in the middle of the screen for a single frame on startup.

## [0.2.1] - 2024-03-22

### Fixed
- Animation no longer plays multiple times when a new client is addded.

## [0.2.0] - 2024-03-21

### Added
- Audio client icons can now be desplayed with `-i` `--icon` flag.
- Automated dynamically linked release builds for general linux distributions (not NixOS) with `glibc` that include all features.

### Fixed
- (CSS) GTK system theme was unintentionally affecting style.
- Window quickly resizing because sink buffer was not populated fast enough.
- Peaker no longer breaks when volume slider is set to 0.
- Window no longer steals keyboard focus if `--keep` was provided and it's not necessarily.

## [0.1.10] - 2024-03-03

### Deprecated
- (CSS) `.client { animation: ... }` is now deprecated in favor of `.client.new { animation: ... }`

### Added

- (CSS) Animation for removed sinks `.client.removed { animation: ... }`.
- Added a warning for Wayland compositors that don't support [wlr-layer-shell-unstable-v1](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) protocol.
- Added a [CHANGELOG.md](/CHANGELOG.md).

### Fixed
- `X11` and `Wayland` features now can both be included into the binary with runtime checks.

### Changed

- Window now has a fixed default size of `350x30` instead of being dynamic.
- Window autoclosing is now more reliable and closes only when window looses focus.
- List of sinks will now grow from bottom to top if window is anchored to bottom.
- Minimal `rustc` version for compilation is now `1.75.0` due to [FileTimes](https://doc.rust-lang.org/std/fs/struct.FileTimes.html) stabilization.

[0.2.2]: https://github.com/Elvyria/Mixxc/compare/0.2.1...0.2.2
[0.2.1]: https://github.com/Elvyria/Mixxc/compare/0.2.0...0.2.1
[0.2.0]: https://github.com/Elvyria/Mixxc/compare/0.1.10...0.2.0
[0.1.10]: https://github.com/Elvyria/Mixxc/compare/0.1.9...0.1.10
[0.1.9]: https://github.com/Elvyria/Mixxc/compare/0.1.8...0.1.9
[0.1.8]: https://github.com/Elvyria/Mixxc/compare/0.1.7...0.1.8
[0.1.7]: https://github.com/Elvyria/Mixxc/compare/0.1.6...0.1.7
[0.1.6]: https://github.com/Elvyria/Mixxc/compare/0.1.5...0.1.6
[0.1.5]: https://github.com/Elvyria/Mixxc/compare/0.1.4...0.1.5
[0.1.4]: https://github.com/Elvyria/Mixxc/compare/0.1.3...0.1.4
[0.1.3]: https://github.com/Elvyria/Mixxc/compare/0.1.2...0.1.3
[0.1.2]: https://github.com/Elvyria/Mixxc/compare/0.1.1...0.1.2
[0.1.1]: https://github.com/Elvyria/Mixxc/compare/0.1.0...0.1.1
[0.1.0]: https://github.com/Elvyria/Mixxc/releases/tag/0.1.0
