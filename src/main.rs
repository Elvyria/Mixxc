use std::path::PathBuf;

use error::{Error, ConfigError};
use anchor::Anchor;

static APP_NAME:   &str = "Mixxc";
static APP_ID:     &str = "elvy.mixxc";
static APP_BINARY: &str = "mixxc";

#[derive(argh::FromArgs)]
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

    /// show only active sinks
    #[argh(switch, short = 'A', long = "active")]
    active_only: bool,

    #[cfg(feature = "Accent")]
    /// inherit accent color from the system's settings
    #[argh(switch, short = 'C', long = "accent")]
    accent: bool,

    /// margin distance for each anchor point
    #[argh(option, short = 'm', long = "margin")]
    margins: Vec<i32>,

    /// enable master volume slider
    #[argh(switch, short = 'M', long = "master")]
    master: bool,

    /// volume slider orientation: (h)orizontal, (v)ertical
    #[argh(option, short = 'b')]
    bar: Option<String>,

    /// path to the userstyle
    #[argh(option, short = 'u')]
    userstyle: Option<PathBuf>,

    /// keep window open
    #[argh(switch, short = 'k', long = "keep")]
    keep: bool,

    /// enable client icons
    #[argh(switch, short = 'i', long = "icon")]
    icon: bool,

    /// max volume level in percent (default: 100; 1-255)
    #[argh(option, short = 'x', long = "max-volume")]
    max_volume: Option<u8>,

    /// use only one volume slider for each system process
    #[argh(switch, short = 'P', long = "per-process")]
    per_process: bool,

    /// print version
    #[argh(switch, short = 'v')]
    version: bool,
}

fn main() -> Result<(), Error> {
    let args: Args = argh::from_env();

    if args.version {
        print!("{}", env!("CARGO_PKG_NAME"));

        match option_env!("GIT_COMMIT") {
            Some(s) => println!(" {s}"),
            None    => println!(" {}", env!("CARGO_PKG_VERSION")),
        };

        return Ok(())
    }

    let mut anchors = Anchor::None;

    for a in args.anchors.iter().map(Anchor::try_from) {
        anchors |= a?;
    }

    warning(&args);

    let app = relm4::RelmApp::new(crate::APP_ID).with_args(vec![]);

    // Vertically oriented bars imply that we are stacking clients horizontally
    let horizontal = args.bar.unwrap_or_default().starts_with('v');

    app::WM_CONFIG.get_or_init(|| app::WMConfig {
        anchors,
        keep:    args.keep,
        margins: args.margins,
    });

    app.run_async::<app::App>(app::Config {
        width: args.width.unwrap_or(if horizontal { 65 } else { 350 }),
        height: args.height.unwrap_or(if horizontal { 350 } else { 30 }),
        spacing: args.spacing.unwrap_or(20) as i32,
        max_volume: args.max_volume.unwrap_or(100).max(1) as f64 / 100.0,
        show_icons: args.icon,
        horizontal,
        master: args.master,
        show_corked: !args.active_only,
        per_process: args.per_process,
        userstyle: args.userstyle,

        #[cfg(feature = "Accent")]
        accent: args.accent,

        server: server::pulse::Pulse::new().into(),
    });

    Ok(())
}

#[allow(unused_variables)]
fn warning(args: &Args) {
    #[cfg(not(feature = "Wayland"))]
    if xdg::is_wayland() {
        warnln!("You are trying to use {APP_NAME} on Wayland, but '{}' feature wasn't included at compile time!", label::WAYLAND);
    }

    #[cfg(not(feature = "X11"))]
    if xdg::is_x11() {
        warnln!("You are trying to use {APP_NAME} on X Window System, but '{}' feature wasn't included at compile time!", label::X11);
    }

    #[cfg(not(feature = "Sass"))]
    if let Some(p) = &args.userstyle {
        let extension = p.extension().and_then(std::ffi::OsStr::to_str);
        if let Some("sass"|"scss") = extension {
            warnln!("You have specified *.{} file as userstyle, but '{}' feature wasn't included at compile time!", extension.unwrap(), label::SASS)
        }
    }
}

pub async fn config_dir() -> Result<PathBuf, ConfigError> {
    use tokio::fs;

    let mut dir = xdg::config_dir();
    dir.push(crate::APP_BINARY);

    let metadata = fs::metadata(&dir).await;

    match metadata {
        Err(_) => {
            fs::create_dir(&dir)
                .await
                .map_err(|e| ConfigError::Create { e, path: std::mem::take(&mut dir) })?;
        },
        Ok(metadata) => if !metadata.is_dir() {
            return Err(ConfigError::NotDirectory(dir))
        }
    }

    Ok(dir)
}

mod xdg;
mod server;
mod app;
mod anchor;
mod label;
mod proto;
mod error;
mod style;
mod widgets;

#[cfg(feature = "Accent")]
mod accent;
