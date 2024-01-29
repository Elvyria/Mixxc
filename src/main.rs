mod xdg;
mod server;
mod app;
mod anchor;
mod colors;
mod proto;
mod error;
mod style;

use std::path::PathBuf;
use std::fs;

use relm4::RelmApp;
use argh::FromArgs;

use error::{Error, ConfigError};
use anchor::Anchor;
use server::pulse::Pulse;
use app::Config;

static APP_NAME:   &str = "Mixxc";
static APP_ID:     &str = "elvy.mixxc";
static APP_BINARY: &str = "mixxc";

#[derive(FromArgs)]
///Minimalistic volume mixer.
struct Args {
    /// window height
    #[argh(option, short = 'w')]
    width: Option<u32>,

    /// window width
    #[argh(option, short = 'h')]
    height: Option<u32>,

    /// spacing between clients
    #[argh(option, short = 's')]
    spacing: Option<u16>,

    /// screen anchor point: (t)op, (b)ottom, (l)eft, (r)ight
    #[argh(option, short = 'a', long = "anchor")]
    anchors: Vec<String>,

    /// margin distance for each anchor point
    #[argh(option, short = 'm', long = "margin")]
    margins: Vec<i32>,

    /// path to the userstyle
    #[argh(option, short = 'u')]
    userstyle: Option<PathBuf>,

    /// keep window open
    #[argh(switch, short = 'k', long = "keep")]
    keep: bool,

    /// print version
    #[argh(switch, short = 'v')]
    version: bool,

    /// max volume level in percent (default: 100; 1-255)
    #[argh(option, long = "max-volume")]
    max_volume: Option<u8>,
}

fn main() -> Result<(), Error> {
    let args: Args = argh::from_env();

    if args.version {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(())
    }

    let mut anchors = Anchor::None;

    for a in args.anchors.iter().map(Anchor::try_from) {
        anchors |= a?;
    }

    warning(&args);

    let style = match args.userstyle {
        Some(p) => style::read(p),
        None    => style::find(config_dir()?),
    };

    let style = match style {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            style::default()
        }
    };

    let app = RelmApp::new(crate::APP_ID).with_args(vec![]);

    relm4::set_global_css(&style);

    app.run::<app::App>(Config {
        width:   args.width.unwrap_or(0),
        height:  args.height.unwrap_or(0),
        spacing: args.spacing,
        margins: args.margins,
        keep:    args.keep,
        max_volume: args.max_volume.unwrap_or(100).max(1) as f64 / 100.0,
        anchors,

        server: Pulse::new().into(),
    });

    Ok(())
}

#[allow(unused_variables)]
fn warning(args: &Args) {
    #[cfg(not(feature = "Wayland"))]
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        println!("{}: You are trying to use Mixxc on Wayland, but '{}' feature wasn't included at compile time!", colors::WARNING, colors::WAYLAND)
    }

    #[cfg(not(feature = "X11"))]
    if std::env::var("XDG_SESSION_TYPE") == Ok("x11".to_owned()) {
        println!("{}: You are trying to use Mixxc on X Window System, but '{}' feature wasn't included at compile time!", colors::WARNING, colors::X11);
    }

    #[cfg(not(feature = "Sass"))]
    if let Some(p) = &args.userstyle {
        let extension = p.extension().and_then(std::ffi::OsStr::to_str);
        if let Some("sass"|"scss") = extension {
            println!("{}: You have specified *.{} file as userstyle, but '{}' feature wasn't included at compile time!", colors::WARNING, extension.unwrap(), colors::SASS)
        }
    }
}

fn config_dir() -> Result<PathBuf, ConfigError> {
    let mut dir = xdg::config_dir();
    dir.push(crate::APP_BINARY);

    if !dir.exists() {
        fs::create_dir(&dir).map_err(|e| ConfigError::Create { e, path: std::mem::take(&mut dir) })?;
    }

    if !dir.is_dir() {
        return Err(ConfigError::NotDirectory(dir))
    }

    Ok(dir)
}
