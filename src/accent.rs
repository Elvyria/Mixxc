use crate::error::Error;

use zbus::zvariant::{OwnedValue, Structure};

#[zbus::proxy(
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop",
    interface = "org.freedesktop.portal.Settings",
    async_name = "Portal"
)]
trait Settings {
    fn read(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;
}

pub enum Scheme {
    Default,
    Dark,
    Light,
}

impl Portal<'_> {
    pub async fn scheme(&self) -> Result<Scheme, Error> {
        let reply = self.read("org.freedesktop.appearance", "color-scheme").await.unwrap();
        match reply.downcast_ref::<u32>().unwrap() {
            1 => Ok(Scheme::Dark),
            2 => Ok(Scheme::Light),
            _ => Ok(Scheme::Default),
        }
    }

    pub async fn accent(&self) -> Result<(u8, u8, u8), Error> {
        let reply = self.read("org.freedesktop.appearance", "accent-color").await.unwrap();
        let inner = reply.downcast_ref::<Structure>().unwrap();

        let (r, g, b) = <(f64, f64, f64)>::try_from(inner).unwrap();

        let r = (255.0 * r) as u8;
        let g = (255.0 * g) as u8;
        let b = (255.0 * b) as u8;

        Ok((r, g, b))
    }
}
