use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{PathBuf, Path};

use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use crate::error::{StyleError, Error};

#[derive(Default, Copy, Clone)]
pub struct StyleSettings {
    #[cfg(feature = "Accent")]
    pub accent: bool,
}

pub async fn find(path: impl Into<PathBuf>, settings: StyleSettings) -> Result<Cow<'static, str>, Error> {
    let mut path = path.into();

    path.push("style");

    for ext in ["scss", "sass"] {
        path.set_extension(ext);

        match read(&path).await {
            Ok(style) => return Ok(style),
            Err(Error::Style(StyleError::NotFound(_))) => continue,
            Err(e) => return Err(e),
        }
    }

    path.set_extension("css");

    let s = match path.exists() {
        true  => read(path).await,
        false => write_default(path, StyleSettings::default()).await,
    };

    #[cfg(feature = "Accent")]
    if settings.accent {
        if let Ok(s) = s {
            return apply_accent(s).await;
        }
    }

    s
}

pub async fn default(settings: StyleSettings) -> Cow<'static, str> {
    static DEFAULT_STYLE: &str = include_str!("../style/default.css");

    #[cfg(feature = "Accent")]
    if settings.accent {
        let s = Cow::Borrowed(DEFAULT_STYLE);

        if let Ok(s) = apply_accent(s).await {
            return s;
        }
    }

    Cow::Borrowed(DEFAULT_STYLE)
}

async fn write_default(path: impl AsRef<Path>, settings: StyleSettings) -> Result<Cow<'static, str>, Error> {
    let path = path.as_ref();
    let style = default(settings).await;

    let mut fd = File::create(path)
        .await.map_err(|e| StyleError::Create { e, path: path.to_owned() })?;

    fd.write_all(style.as_bytes())
        .await.map_err(|e| StyleError::Write { e, path: path.to_owned() })?;

    Ok(style)
}

pub async fn read(path: impl AsRef<Path>) -> Result<Cow<'static, str>, Error> {
    let path = path.as_ref();

    match path.extension().and_then(OsStr::to_str) {
        Some("sass" | "scss") => compile_sass(path).await.map(Cow::Owned),
        Some("css") => {
            fs::read_to_string(path).await
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

async fn compile_sass(style_path: impl AsRef<std::path::Path>) -> Result<String, Error> {
    use crate::{xdg, error};

    let style_path = style_path.as_ref();

    let style_meta = match fs::metadata(style_path).await {
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

    if let Ok(cache_meta) = fs::metadata(&cache_path).await {
        if Some(style_mtime) == cache_meta.modified().ok() {
            return fs::read_to_string(&cache_path).await
                .map_err(|e| error::CacheError::Read { e, path: cache_path })
                .map_err(Into::into);
        }
    }

    #[cfg(feature = "Sass")]
    let compiled = {
        let style = fs::read_to_string(style_path).await
                .map_err(|e| StyleError::Read { e, path: style_path.to_owned() })?;

        grass::from_string(style, &grass::Options::default()).map_err(StyleError::Sass)?
    };

    #[cfg(not(feature = "Sass"))]
    let compiled = {
        use std::io::Write;

        let output = std::process::Command::new("sass")
            .args(["--no-source-map", "-s", "expanded", &style_path.to_string_lossy()])
            .output()
            .map_err(|e| StyleError::SystemCompiler { e: Some(e), path: style_path.to_owned() })?;

        let _ = std::io::stderr().write_all(&output.stderr);

        if !output.status.success() {
            let e = StyleError::SystemCompiler { e: None, path: style_path.to_owned() };
            return Err(e.into())
        }

        unsafe { String::from_utf8_unchecked(output.stdout) }
    };

    if let Err(e) = cache(cache_path, &compiled, style_mtime).await {
        eprintln!("{e}");
    }

    Ok(compiled)
}

async fn cache(path: impl AsRef<Path>, style: &str, time: std::time::SystemTime) -> Result<(), crate::error::CacheError> {
    use crate::error::CacheError;

    let path = path.as_ref();

    let mut f = File::create(path).await
        .map_err(|e| CacheError::Create { e, path: path.to_owned() })?;

    f.write_all(style.as_bytes()).await
        .map_err(|e| CacheError::Write { e, path: path.to_owned() })?;

    f.into_std().await.set_modified(time)
        .map_err(|e| CacheError::MTime { e, path: path.to_owned() })
}

#[cfg(feature = "Accent")]
async fn apply_accent(s: Cow<'static, str>) -> Result<Cow<'static, str>, Error> {
    use crate::accent;
    use crate::error::ZbusError;

    fn find_and_replace(s: &str, r: u8, g: u8, b: u8) -> Option<String> {
        let mut s = s.to_owned();

        let start = s.find("--accent:")?;
        let end = s[start..].find(';')?;

        s.replace_range(
            start..start + end,
            &format!("--accent:#{r:02X}{g:02X}{b:02X}"));

        Some(s)
    }

    let conn = zbus::Connection::session().await
        .map_err(|e| ZbusError::Connect { e })?;

    let settings = accent::Settings::new(&conn).await
        .map_err(|e| ZbusError::Proxy { e })?;

    let (r, g, b) = settings.accent().await?;

    match find_and_replace(&s, r, g, b).map(Cow::Owned) {
        Some(s) => Ok(s),
        None => Ok(s),
    }
}
