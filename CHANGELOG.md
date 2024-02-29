# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- (CSS) Animation for removed sinks `.client.removed`.
- Added a warning for Wayland compositors that don't support [wlr-layer-shell-unstable-v1](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) protocol.
- Added a [CHANGELOG.md](/CHANGELOG.md).

### Fixed
- `X11` and `Wayland` features now can both be included into the binary with runtime checks.

### Changed

- (CSS) Animation for new sinks is now inside `.client.new` class.
- Window now has a fixed default size of 350x30 instead of being dynamic.
- Window autoclosing is now more reliable and closes only when window looses focus.
- List of sinks will now grow from bottom to top if window is anchored to bottom.
- Minimal `rustc` version for compilation is now `1.75.0` due to [FileTimes](https://doc.rust-lang.org/std/fs/struct.FileTimes.html) stabilization.

[unreleased]: https://github.com/Elvyria/Mixxc/compare/0.1.9...HEAD
