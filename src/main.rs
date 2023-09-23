mod xdg;
mod server;
mod app;
mod anchor;

use std::borrow::Cow;
use std::{env, fs};
use std::str::FromStr;
use std::ops::BitOr;
use std::fs::File;
use std::io::Write;

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

    let style = userstyle().map_err(|e| eprintln!("{}", e)).unwrap_or(Cow::Borrowed(DEFAULT_STYLE));
    relm4::set_global_css(&style);

    app.run::<app::App>(Config {
        width:   args.width.unwrap_or(0),
        height:  args.height.unwrap_or(0),
        spacing: args.spacing,
        margins: args.margins,
        anchors,

        server: Pulse::new().into(),
    });
}

fn userstyle() -> Result<Cow<'static, str>, Error> {
    let mut style_path = xdg::config_dir();
    style_path.push(crate::APP_BINARY);

    if !style_path.exists() {
        fs::create_dir(&style_path)
            .with_context(|| format!("Unable to create a config directory {:?}", style_path))?;
    }

    if !style_path.is_dir() {
        return Err(anyhow!("Unable to access a config directory.\n{:?} is not a directory", style_path))
    }

    style_path.push(crate::APP_BINARY);

    #[cfg(feature = "Sass")]
    match sass(&mut style_path) {
        Ok(style) if !style.is_empty() => return Ok(Cow::Owned(style)),
        Err(e) => eprintln!("{}", e),
        Ok(_) => {},
    }

    style_path.set_extension("css");

    if !style_path.exists() {
        let mut fd = File::create(&style_path)
            .with_context(|| format!("Unable to create a style file {:?}", style_path))?;

        fd.write_all(DEFAULT_STYLE.as_bytes())
            .with_context(|| format!("Unable to write a style to a file {:?}", style_path))?;

        return Ok(Cow::Borrowed(DEFAULT_STYLE));
    }

    fs::read_to_string(&style_path)
        .map(Cow::Owned)
        .with_context(|| format!("Unable to read a style file {:?}", style_path))
}

#[cfg(feature = "Sass")]
fn sass(style: &mut std::path::PathBuf) -> Result<String, Error> {
    style.set_extension("scss");

    let style_meta = match fs::metadata(&style) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => {
            return Err(anyhow!("Error while trying to get metadata of {:?}: {}", style, e))
        }
    };

    let style_mtime = style_meta.modified()
        .with_context(|| format!("Unable to read mtime of a style {:?}", style))?;

    let mut cache = xdg::cache_dir();
    cache.push(crate::APP_BINARY);
    cache.set_extension("css");

    if let Ok(cache_meta) = fs::metadata(&cache) {
        if Some(style_mtime) == cache_meta.modified().ok() {
            return fs::read_to_string(cache).map_err(Into::into);
        }
    }

    let compiled = grass::from_path(&style, &grass::Options::default())?;
    if let Err(e) = fs::write(&cache, &compiled) {
        eprintln!("Unable to cache sass: {}", e);
    }

    if let Err(e) = filetime::set_file_mtime(&cache, filetime::FileTime::from_system_time(style_mtime)) {
        eprintln!("Unable to update mtime for cache: {}", e);
    }

    Ok(compiled)
}
