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
