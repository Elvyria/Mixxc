use std::borrow::Cow;
use std::ffi::OsStr;
use std::io::Write;
use std::fs::{File, self};
use std::path::{PathBuf, Path};

use crate::error::{StyleError, Error};

pub fn find(path: impl Into<PathBuf>) -> Result<Cow<'static, str>, Error> {
    let mut path = path.into();

    path.push("style");

    #[cfg(feature = "Sass")]
    for ext in ["scss", "sass"] {
        path.set_extension(ext);

        match read(&path) {
            Ok(style) => return Ok(style),
            Err(Error::Style(StyleError::NotFound(_))) => continue,
            Err(e) => return Err(e),
        }
    }

    path.set_extension("css");

    match path.exists() {
        true  => read(path),
        false => write_default(path),
    }
}

pub fn default() -> Cow<'static, str> {
    static DEFAULT_STYLE: &str = include_str!("../style/default.css");
    Cow::Borrowed(DEFAULT_STYLE)
}

fn write_default(path: impl AsRef<Path>) -> Result<Cow<'static, str>, Error> {
    let path = path.as_ref();
    let style = default();
    let mut fd = File::create(path).map_err(|e| StyleError::Create { e, path: path.to_owned() })?;
    fd.write_all(style.as_bytes()).map_err(|e| StyleError::Write { e, path: path.to_owned() })?;

    Ok(style)
}

pub fn read(path: impl AsRef<Path>) -> Result<Cow<'static, str>, Error> {
    let path = path.as_ref();

    match path.extension().and_then(OsStr::to_str) {
        #[cfg(feature = "Sass")]
        Some("sass" | "scss") => compile_sass(path).map(Cow::Owned),
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
fn compile_sass(style_path: impl AsRef<std::path::Path>) -> Result<String, Error> {
    use crate::{xdg, error};

    let style_path = style_path.as_ref();

    let style_meta = match fs::metadata(style_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(StyleError::NotFound(e).into()),
        Err(e) => {
            return Err(StyleError::Meta { e, path: style_path.to_owned() }.into())
        }
    };

    let style_mtime = style_meta.modified().map_err(|e| StyleError::MTime { e, path: style_path.to_owned() })?;

    let mut cache_path = xdg::cache_dir();
    cache_path.push(crate::APP_BINARY);
    cache_path.set_extension("css");

    if let Ok(cache_meta) = fs::metadata(&cache_path) {
        if Some(style_mtime) == cache_meta.modified().ok() {
            return fs::read_to_string(&cache_path)
                .map_err(|e| error::CacheError::Read { e, path: cache_path })
                .map_err(Into::into);
        }
    }

    let compiled = grass::from_path(style_path, &grass::Options::default()).map_err(StyleError::Sass)?;

    if let Err(e) = cache(cache_path, &compiled, style_mtime) {
        eprintln!("{e}");
    }

    Ok(compiled)
}

#[cfg(feature = "Sass")]
fn cache(path: impl AsRef<Path>, style: &str, time: std::time::SystemTime) -> Result<(), crate::error::CacheError> {
    use crate::error::CacheError;

    let path = path.as_ref();

    let mut f = File::create(path)
        .map_err(|e| CacheError::Create { e, path: path.to_owned() })?;

    f.write_all(style.as_bytes())
        .map_err(|e| CacheError::Write { e, path: path.to_owned() })?;

    f.set_modified(time)
        .map_err(|e| CacheError::MTime { e, path: path.to_owned() })
}
