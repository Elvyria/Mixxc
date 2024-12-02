use crate::error::{Error, ZbusError};

use zbus::zvariant::{OwnedValue, Structure};

#[zbus::proxy(
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop",
    interface = "org.freedesktop.portal.Settings",
    async_name = "Settings"
)]
pub trait Settings {
    fn read(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;
}

pub enum Scheme {
    Default,
    Dark,
    Light,
}

impl Settings<'_> {
    async fn appearance(&self, key: &str) -> Result<OwnedValue, Error> {
        self.read("org.freedesktop.appearance", key)
            .await
            .map_err(|e| ZbusError::Read { e,
                namespace: "org.freedesktop.appearance".to_string(),
                key: key.to_string()
            })
            .map_err(Into::into)
    }

    #[allow(dead_code)]
    pub async fn scheme(&self) -> Result<Scheme, Error> {
        let reply = self.appearance("color-scheme").await?;

        let v = reply.downcast_ref::<u32>()
            .map_err(|_| ZbusError::BadResult { v: format!("{reply:?}") })?;

        match v {
            1 => Ok(Scheme::Dark),
            2 => Ok(Scheme::Light),
            _ => Ok(Scheme::Default),
        }
    }

    pub async fn accent(&self) -> Result<(u8, u8, u8), Error> {
        let reply = self.appearance("accent-color").await?;

        let (r, g, b) = reply.downcast_ref::<Structure>()
            .and_then(<(f64, f64, f64)>::try_from)
            .map_err(|_| ZbusError::BadResult { v: format!("{reply:?}") })?;

        let r = (255.0 * r) as u8;
        let g = (255.0 * g) as u8;
        let b = (255.0 * b) as u8;

        Ok((r, g, b))
    }
}
