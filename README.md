# Mixxc
[![Crates.io](https://img.shields.io/crates/v/mixxc?logo=rust)](https://crates.io/crates/mixxc)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow)](https://opensource.org/licenses/MIT)

Mixxc is a minimalistic and customizable volume mixer, created to seamlessly complement desktop widgets.  
Currently, it supports only `pulseaudio` and `pipewire` (through the pulseaudio interface) by utilizing `libpulseaudio` to receive audio events.

![Preview](https://user-images.githubusercontent.com/2061234/270078395-6454be21-aa09-4da2-8a07-3a3c9b41138f.png)

## Usage
```
Usage: mixxc [-w <width>] [-h <height>] [-s <spacing>] [-a <anchor...>] [-m <margin...>] [-v]

Minimalistic volume mixer.

Options:
  -w, --width       window height
  -h, --height      window width
  -s, --spacing     spacing between clients
  -a, --anchor      screen anchor point: (t)op, (b)ottom, (l)eft, (r)ight
  -m, --margin      margin distance for each anchor point
  -v, --version     print version
  --help            display usage information
```

## Customization
Mixxc is built with GTK4 and uses CSS to define its appearance.  
You will find the style sheet in your config directory after the first launch.
```
${XDG_CONFIG_HOME:-$HOME/.config}/mixxc/mixxc.css
```

## Environment
Mixxc is developed and tested with: 
* Wayland
* Hyprland
* PipeWire

If your setup is different and you experience issues, feel free to file a bug report.

## Startup Time
If startup seems a bit slow, try this:
```
GSK_RENDERER=cairo GTK_USE_PORTAL=0 mixxc
```

## Dependencies
* gtk4
* gtk4-layer-shell (Wayland)
* libpulseaudio (PulseAudio)

## Installation

Can be installed from [crates.io](https://crates.io/) with `cargo`:

```sh
cargo install mixxc
```

## Building

To build this little thing, you'll need some [Rust](https://www.rust-lang.org/).

```sh
git clone --depth 1 https://github.com/Elvyria/mixxc
cd mixxc
cargo build --release
```
