# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed
- (CSS) GTK system theme was unintentionally affecting style.
- Window quickly resizing because sink buffer was not populated fast enough.

## [0.1.10] - 2024-03-03

### Deprecated
- (CSS) `.client { animation: ... }` is not deprecated in favor of `.client.new { animation: ... }`

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

[unreleased]: https://github.com/Elvyria/Mixxc/compare/0.1.10...HEAD
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
