use std::{path::PathBuf, io};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Style(#[from] StyleError),

    #[cfg(feature = "Sass")]
    #[error(transparent)]
    Cache(#[from] CacheError),

    #[cfg(feature = "Sass")]
    #[error(transparent)]
    Sass(#[from] Box<grass::Error>),
}

#[derive(Error, Debug)]
pub enum ConfigError {
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

    #[cfg(feature = "Sass")]
    #[error("Error while trying to get metadata ({path})\n{e}")]
    Meta { e: io::Error, path: PathBuf },

    #[cfg(feature = "Sass")]
    #[error("Unable to read mtime of a style ({path})\n{e}")]
    MTime { e: io::Error, path: PathBuf},

    #[error("Unable to write a style to a file ({path})\n{e}")]
    Write { e: io::Error, path: PathBuf },

    #[cfg(feature = "Sass")]
    #[error(transparent)]
    NotFound { e: io::Error },
}

#[cfg(feature = "Sass")]
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Unable to read a cache file ({path})\n{e}")]
    Read { e: io::Error, path: PathBuf },

    #[error("Unable to write a cache file ({path})\n{e}")]
    Write { e: io::Error, path: PathBuf },

    #[error("Unable to update mtime for cache ({path})\n{e}")]
    MTime { e: io::Error, path: PathBuf },
}
