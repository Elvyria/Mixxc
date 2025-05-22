# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed
- Active clients will take priority over muted and paused ones with `-P` `--per-process` flag as intended.

### Changed
- (Breaking) Flag `-k` `--keep` was removed in favor of the user provided delay option for closing `-c` `--close`.
- Window will no longer close itself without a notice by default to not confuse new users.

## [0.2.4] - 2025-01-14

### Added
- Integration with system accent color setting through [XDG Desktop Portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html).  
Requires `Accent` feature to be included at compile time, compatible `xdg-desktop-portal` set and running for `org.freedesktop.impl.portal.Settings` (like `xdg-desktop-portal-kde`) and `-C` or `--accent` flag.
- New sidebar to quickly swap between audio outputs.
- `.sass` and `.scss` styles will be compiled using the system `sass` compiler binary if `Sass` feature was not enabled at the compile time.  Style compilation time is much longer, but the resulting binary is around 2mb smaller in size. (https://sass-lang.com/install)

### Changed
- Version reported by the `-v` `--version` flag will now include git commit hash if the commit used for build wasn't tagged.
- Default `.css` style is now automatically compiled from `.scss` to reduce the amount of syntax errors and ease the maintenance.
- (CSS) Deprecated`@define-colors` and SCSS color definitions in favor of the CSS variables.
- (CSS) New boolean flags for `.scss` to toggle visibility of some elements.
- (CSS) Dimmed border accent color, reduced font size and changed volume bar color into a gradient, this should provide a slightly more interesting result with different accents.

### Fixed
- Layershell initialization before window is realized, which could prevent a successful launch under certain conditions.
- Missing bracket in the default `.scss` style.
- Audio server connection is now cleanly terminated when window is closed or if process recieves SIGINT signal. (should cure the sound popping)

## [0.2.3] - 2024-10-22

### Added
- New flag `-A` `--active` that hides paused clients.
- New flag `-P` `--per-process` that combines sinks from the same process into a single one.  
(This should help with WINE and browser applications, but might have unexpected side effects depending on the software)
- (CSS) Default style now has an animation when hovering over or clicking a volume knob.

### Fixed
- Excessive number of updates on unrelated client fields, caused by a function that lowers peak.
- Unsynchronized communications with pulse audio server that could lead to issues.

### Changed
- (CSS) Default foreground color is now less eye burning. (#FFFFFF -> #DDDDDD)
- GTK log messages will not appear if `GTK_DEBUG` variable is not set.

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

[unreleased]: https://github.com/Elvyria/Mixxc/compare/0.2.4...HEAD
[0.2.4]: https://github.com/Elvyria/Mixxc/compare/0.2.3...0.2.4
[0.2.3]: https://github.com/Elvyria/Mixxc/compare/0.2.2...0.2.3
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
