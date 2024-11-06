use std::{env, path::PathBuf, sync::OnceLock};

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

pub fn cache_dir() -> PathBuf {
    env_or_default("XDG_CACHE_HOME", ".cache")
}

enum Platform {
    Wayland,
    X11,
    Unknown,
}

fn platform() -> &'static Platform {
    static PLATFORM: OnceLock<Platform> = OnceLock::new();

    PLATFORM.get_or_init(|| {
        match env::var("XDG_SESSION_TYPE").map(|s| s.to_lowercase()).as_deref() {
            Ok("wayland") => return Platform::Wayland,
            Ok("x11")     => return Platform::X11,
            _             => {},
        }

        if env::var("WAYLAND_DISPLAY").is_ok() {
            return Platform::Wayland
        }

        if env::var("DISPLAY").is_ok() {
            return Platform::X11
        }

        Platform::Unknown
    })
}

pub fn is_wayland() -> bool {
    matches!(platform(), Platform::Wayland)
}

pub fn is_x11() -> bool {
    matches!(platform(), Platform::X11)
}
