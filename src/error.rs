use std::{path::PathBuf, io, fmt::Debug};

use crate::label;

use thiserror::Error;

#[macro_export]
macro_rules! warnln {
    ($($arg:tt)*) => {{
        println!("{}: {}", $crate::label::WARNING, format_args!($($arg)*))
    }};
}

#[derive(Error)]
pub enum Error {
    #[error(transparent)]
    Cli(#[from] CLIError),

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Style(#[from] StyleError),

    #[error(transparent)]
    Cache(#[from] CacheError),

    #[cfg(feature = "Accent")]
    #[error(transparent)]
    Accent(#[from] ZbusError),
}

impl Debug for Error {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "\x1b[7D{}: {}", label::ERROR, self)
    }
}

#[derive(Error, Debug)]
pub enum CLIError {
    #[error("'{0}' is not a valid anchor point")]
    Anchor(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Unable to access a config directory {path}\n{e}")]
    Read { e: io::Error, path: PathBuf },

    #[error("Unable to create a config directory {path}\n{e}")]
    Create { e: io::Error, path: PathBuf },

    #[error("Unable to access a config directory\n{0} is not a directory")]
    NotDirectory(PathBuf),
}

#[derive(Error, Debug)]
pub enum StyleError {
    #[error("Unable to create a style file ({path})\n{e}")]
    Create { e: io::Error, path: PathBuf },

    #[error("Unknown style file extension (expected {expected})")]
    Extension { expected: &'static str },

    #[error("Unable to read a style file ({path})\n{e}")]
    Read { e: io::Error, path: PathBuf },

    #[error("Error while trying to get metadata ({path})\n{e}")]
    Meta { e: io::Error, path: PathBuf },

    #[error("Unable to read mtime of a style ({path})\n{e}")]
    MTime { e: io::Error, path: PathBuf},

    #[error("Unable to write a style to a file ({path})\n{e}")]
    Write { e: io::Error, path: PathBuf },

    #[error(transparent)]
    NotFound(io::Error),

    #[cfg(not(feature = "Sass"))]
    #[error("Couldn't compile a style using the system `sass` binary ({path})\n{e:?}")]
    SystemCompiler { e: Option<io::Error>, path: PathBuf },

    #[cfg(feature = "Sass")]
    #[error(transparent)]
    Sass(#[from] Box<grass::Error>),
}

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Unable to create a cache file ({path})\n{e}")]
    Create { e: io::Error, path: PathBuf },

    #[error("Unable to read a cache file ({path})\n{e}")]
    Read { e: io::Error, path: PathBuf },

    #[error("Unable to write a cache file ({path})\n{e}")]
    Write { e: io::Error, path: PathBuf },

    #[error("Unable to update mtime for cache ({path})\n{e}")]
    MTime { e: io::Error, path: PathBuf },
}

#[cfg(feature = "Accent")]
#[derive(Error, Debug)]
pub enum ZbusError {
    #[error("Couldn't establish a connection with the session bus\n{e}")]
    Connect { e: zbus::Error },

    #[error("Couldn't create a proxy to access the bus interface\n{e}")]
    Proxy { e: zbus::Error },

    #[error("Unable to read `{key}` from `{namespace}, make sure that your `xdg-desktop-portal` supports it and configured correctly`\n{e}")]
    Read { e: zbus::Error, namespace: String, key: String },

    #[error("Unable to parse unexpected result from the portal\n{v}")]
    BadResult { v: String }
}
