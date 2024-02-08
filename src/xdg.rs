use std::{env, path::PathBuf};

fn env_or_default(env: &str, fallback: &str) -> PathBuf {
    env::var_os(env)
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|mut p| { p.push(fallback); p })
        })
    .unwrap_or_else(|| panic!("couldn't find the {env} directory"))
}

pub fn config_dir() -> PathBuf {
    env_or_default("XDG_CONFIG_HOME", ".config")
}

#[cfg(feature = "Sass")]
pub fn cache_dir() -> PathBuf {
    env_or_default("XDG_CACHE_HOME", ".cache")
}

pub fn is_wayland() -> bool {
    env::var("WAYLAND_DISPLAY").is_ok()
        || env::var("XDG_SESSION_TYPE") == Ok("wayland".to_owned())
}

pub fn is_x11() -> bool {
    use env::var;

    var("DISPLAY").is_ok() && var("WAYLAND_DISPLAY").is_err()
        || var("XDG_SESSION_TYPE") == Ok("x11".to_owned())
}
