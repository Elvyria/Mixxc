# Mixxc
[![Crates.io](https://img.shields.io/crates/v/mixxc?logo=rust)](https://crates.io/crates/mixxc)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow)](https://opensource.org/licenses/MIT)

Mixxc is a minimalistic and customizable volume mixer, created to seamlessly complement desktop widgets.  
Currently, it supports only `pulseaudio` and `pipewire` (through the pulseaudio interface) by utilizing `libpulseaudio` to receive audio events.

![Preview - Vertical](https://github.com/Elvyria/Mixxc/assets/2061234/cc7c1849-ec4b-4e58-840e-cf66db99be76)
![Preview](https://user-images.githubusercontent.com/2061234/270078395-6454be21-aa09-4da2-8a07-3a3c9b41138f.png)

## Usage
```sh
Usage: mixxc [-w <width>] [-h <height>] [-s <spacing>] [-a <anchor...>] [-A] [-m <margin...>] [-M] [-b <bar>] [-u <userstyle>] [-k] [-i] [-x <max-volume>] [-P] [-v]

Minimalistic volume mixer.

Options:
  -w, --width       window height
  -h, --height      window width
  -s, --spacing     spacing between clients
  -a, --anchor      screen anchor point: (t)op, (b)ottom, (l)eft, (r)ight
  -A, --active      show only active sinks
  -m, --margin      margin distance for each anchor point
  -M, --master      enable master volume slider
  -b, --bar         volume slider orientation: (h)orizontal, (v)ertical
  -u, --userstyle   path to the userstyle
  -k, --keep        keep window open
  -i, --icon        enable client icons
  -x, --max-volume  max volume level in percent (default: 100; 1-255)
  -P, --per-process use only one volume slider for each system process
  -v, --version     print version
  --help            display usage information
```

### Anchoring
It is often desirable to be able to position widgets relatively to a screen side.  
Two flags will help with this: `-a --anchor` and `-m --margin`.  
Each margin value provided will match every anchor point respectively.  
```sh
mixxc --anchor left --anchor bottom --margin 20 --margin 30
```

## Customization
Mixxc is built with GTK4 and uses CSS to define its appearance.  
You will find the style sheet in your config directory after the first launch.
```sh
${XDG_CONFIG_HOME:-$HOME/.config}/mixxc/style.css
```
If you have enabled the Sass feature, it will also look for *.scss and *.sass files.
```sh
${XDG_CONFIG_HOME:-$HOME/.config}/mixxc/style.sass
${XDG_CONFIG_HOME:-$HOME/.config}/mixxc/style.scss
```

## Environment
Mixxc is developed and tested with: 
* Wayland (Hyprland): `0.44.1`
* PipeWire: `1.2.5`

If your setup is different and you experience issues, feel free to file a bug report.

## Tips
### Startup Time
If startup seems a bit slow or memory usage seems a bit too high try this:
```sh
GSK_RENDERER=cairo GTK_USE_PORTAL=0 mixxc
```
### Manual Closing
By default window will be closed after it looses focus.  
This is a workaround for WM's that don't send `close` request to layer-shells under Wayland.  
If this behavior is not desirable, you can disable it:
```sh
mixxc --keep
```
### Toggle Window
If you want to toggle window with a click of a button, Unix way is the way:
```sh
pkill mixxc | mixxc
```

## Troubleshooting
To get GTK related messages a specific environment variable must be non empty.
```sh
GTK_DEBUG=1 mixxc
```

## Dependencies
* [GTK4](https://www.gtk.org/)
* [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) (Feature: Wayland)
* [libpulseaudio](https://www.freedesktop.org/wiki/Software/PulseAudio)
* [libxcb](https://xcb.freedesktop.org/) (Feature: X11)

## Features
Some features can be enabled at compile time.
* [Sass](https://sass-lang.com/) - Allows you to use SCSS instead of CSS.
* [Wayland](https://wayland.freedesktop.org/) - Uses wlr-layer-shell to imitate window positioning.
* [X11](https://www.x.org/) - Sets WM hints and properties, and repositions the window.

## Installation
Can be installed from [crates.io](https://crates.io/) with `cargo`:

```sh
cargo install mixxc --locked --features Sass,Wayland...
```

## Building
To build this little thing, you'll need some [Rust](https://www.rust-lang.org/).

```sh
git clone --depth 1 https://github.com/Elvyria/mixxc
cd mixxc
cargo build --locked --release --features Sass,Wayland...
```
