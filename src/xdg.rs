use std::{env, path::PathBuf};

macro_rules! xdg {
    ($env:expr, $dir:expr) => {
        env::var_os($env)
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|mut p| { p.push($dir); p })
            })
        .expect(concat!("couldn't find ", $env, " directory"))
    };
}

pub fn config_dir() -> PathBuf {
    xdg!("XDG_CONFIG_HOME", ".config")
}

#[cfg(feature = "Sass")]
pub fn cache_dir() -> PathBuf {
    xdg!("XDG_CACHE_HOME", ".cache")
}
