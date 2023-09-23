mod server;
mod app;
mod anchor;

use std::env;
use std::str::FromStr;
use std::ops::BitOr;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Error, anyhow};
use itertools::Itertools;
use relm4::RelmApp;
use argh::FromArgs;

use anchor::Anchor;
use server::pulse::Pulse;
use app::Config;

static APP_NAME:   &str = "Mixxc";
static APP_ID:     &str = "elvy.mixxc";
static APP_BINARY: &str = "mixxc";

static DEFAULT_STYLE: &str = include_str!("../style/default.css");

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

    /// print version
    #[argh(switch, short = 'v')]
    version: bool,
}

fn main() {
    let args: Args = argh::from_env();

    if args.version {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return
    }

    let anchors = args.anchors.iter()
        .map(String::as_ref)
        .map(Anchor::from_str)
        .fold_ok(Anchor::None, Anchor::bitor);

    let anchors = match anchors {
        Ok(anchors) => anchors,
        Err(e) => panic!("'{}' is not a valid anchor point", e.0),
    };

    let app = RelmApp::new(crate::APP_ID).with_args(vec![]);

    match userstyle() {
        Ok(p) => {
            relm4::set_global_css_from_file(p)
        },
        Err(e) => {
            eprintln!("{}", e);
            relm4::set_global_css(DEFAULT_STYLE);
        }
    }

    app.run::<app::App>(Config {
        width:   args.width.unwrap_or(0),
        height:  args.height.unwrap_or(0),
        spacing: args.spacing,
        margins: args.margins,
        anchors,

        server: Pulse::new().into(),
    });
}

fn userstyle() -> Result<PathBuf, Error> {
    let mut style = xdg_config_dir();
    style.push(crate::APP_BINARY);

    if !style.exists() {
        std::fs::create_dir(&style)
            .with_context(|| format!("Unable to create a config directory {:?}", style))?;
    }

    if !style.is_dir() {
        return Err(anyhow!("Unable to access a config directory.\n{:?} is not a directory", style))
    }

    style.push(crate::APP_BINARY);
    style.set_extension("css");

    #[cfg(feature = "scss")]
    style.set_extension("scss");

    if !style.exists() {
        let mut fd = File::create(&style)
            .with_context(|| format!("Unable to create a style file {:?}", style))?;

        fd.write_all(DEFAULT_STYLE.as_bytes())
            .with_context(|| format!("Unable to write a style to a file {:?}", style))?;
    }

    Ok(style)
}

fn xdg_config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|mut p| { p.push(".config"); p })
        })
    .expect("couldn't find config directory")
}
