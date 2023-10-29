mod xdg;
mod server;
mod app;
mod anchor;
mod colors;
mod proto;
mod error;

use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{PathBuf, Path};
use std::fs::{self, File};
use std::io::Write;

use relm4::RelmApp;
use argh::FromArgs;

use error::{Error, CLIError, ConfigError, StyleError};
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

    /// path to the userstyle
    #[argh(option, short = 'u')]
    userstyle: Option<PathBuf>,

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
        Some(p) if !p.exists() => {
            return Err(CLIError::FileNotFound(p).into());
        }
        Some(p) => userstyle(p),
        None => {
            config_dir()
            .map_err(Into::into)
            .and_then(|mut style_path| {
                style_path.push("style");

                #[cfg(feature = "Sass")]
                for ext in ["scss", "sass"] {
                    style_path.set_extension(ext);

                    match userstyle(&style_path) {
                        Ok(style) => return Ok(style),
                        Err(Error::Style(StyleError::NotFound(_))) => continue,
                        Err(e) => eprintln!("{}", e)
                    }
                }
                
                style_path.set_extension("css");

                userstyle(style_path)
            })
        },
    };

    let style = match style {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}\nFalling back to default style...", e);
            Cow::Borrowed(DEFAULT_STYLE)
        }
    };

    let app = RelmApp::new(crate::APP_ID).with_args(vec![]);

    relm4::set_global_css(&style);

    app.run::<app::App>(Config {
        width:   args.width.unwrap_or(0),
        height:  args.height.unwrap_or(0),
        spacing: args.spacing,
        margins: args.margins,
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
        let extension = p.extension().and_then(OsStr::to_str);
        if let Some("sass"|"scss") = extension {
            println!("{}: You have specified *.{} file as userstyle, but '{}' feature wasn't included at compile time!", colors::WARNING, extension.unwrap(), colors::SASS)
        }
    }
}

fn config_dir() -> Result<PathBuf, ConfigError> {
    let mut dir = xdg::config_dir();
    dir.push(crate::APP_BINARY);

    if !dir.exists() {
        fs::create_dir(&dir).map_err(|e| ConfigError::Create { e, path: dir.clone() })?;
    }

    if !dir.is_dir() {
        return Err(ConfigError::NotDirectory(dir))
    }

    Ok(dir)
}

fn userstyle(path: impl AsRef<Path>) -> Result<Cow<'static, str>, Error> {
    let path = path.as_ref();

    match path.extension().and_then(OsStr::to_str)
    {
        #[cfg(feature = "Sass")]
        Some("sass" | "scss") => sass(path).map(Cow::Owned),
        Some("css") if !path.exists() => {
            let mut fd = File::create(path).map_err(|e| StyleError::Create { e, path: path.to_owned() })?;
            fd.write_all(DEFAULT_STYLE.as_bytes()).map_err(|e| StyleError::Write { e, path: path.to_owned() })?;

            Ok(Cow::Borrowed(DEFAULT_STYLE))
        },
        Some("css") => {
            fs::read_to_string(path)
                .map(Cow::Owned)
                .map_err(|e| StyleError::Read { e, path: path.to_owned() })
                .map_err(Into::into)
        },
        None | Some(_) => {
            #[allow(unused_variables)]
            let expected = "css";

            #[cfg(feature = "Sass")]
            let expected = "css, sass, scss";

            Err(StyleError::Extension { expected }.into())
        },
    }
}

#[cfg(feature = "Sass")]
fn sass(style_path: impl AsRef<std::path::Path>) -> Result<String, Error> {
    let style_path = style_path.as_ref();

    let style_meta = match fs::metadata(style_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(StyleError::NotFound(e).into()),
        Err(e) => {
            return Err(StyleError::Meta { e, path: style_path.to_owned() }.into())
        }
    };

    let style_mtime = style_meta.modified().map_err(|e| StyleError::MTime { e, path: style_path.to_owned() })?;

    let mut cache = xdg::cache_dir();
    cache.push(crate::APP_BINARY);
    cache.set_extension("css");

    use error::CacheError;

    if let Ok(cache_meta) = fs::metadata(&cache) {
        if Some(style_mtime) == cache_meta.modified().ok() {
            return fs::read_to_string(&cache)
                .map_err(|e| CacheError::Read { e, path: cache })
                .map_err(Into::into);
        }
    }

    let compiled = grass::from_path(style_path, &grass::Options::default())?;
    if let Err(e) = fs::write(&cache, &compiled) {
        eprintln!("{}", CacheError::Write { e, path: cache.clone() });
    }

    if let Err(e) = filetime::set_file_mtime(&cache, filetime::FileTime::from_system_time(style_mtime)) {
        eprintln!("{}", CacheError::MTime { e, path: cache });
    }

    Ok(compiled)
}
