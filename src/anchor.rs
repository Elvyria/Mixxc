use crate::error::CLIError;

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Anchor: u8 {
        const None    = 0b0000;
        const Top     = 0b0001;
        const Left    = 0b0010;
        const Bottom  = 0b0100;
        const Right   = 0b1000;
    }
}

impl TryFrom<&String> for Anchor {
    type Error = CLIError;

    fn try_from(s: &String) -> Result<Self, Self::Error> {
        match s.as_bytes().first().map(u8::to_ascii_lowercase) {
            Some(b't') => Ok(Anchor::Top),
            Some(b'l') => Ok(Anchor::Left),
            Some(b'b') => Ok(Anchor::Bottom),
            Some(b'r') => Ok(Anchor::Right),
            _          => Err(CLIError::Anchor(s.to_owned())),
        }
    }
}

#[cfg(feature = "X11")]
impl Anchor {
    pub fn position(&self, margins: &[i32], screen: (u32, u32), window: (u32, u32)) -> (i32, i32) {
        let (mut x, mut y) = (0i32, 0i32);

        for (i, anchor) in self.iter().enumerate() {
            let margin = margins.get(i).unwrap_or(&0);

            match anchor {
                Anchor::Top => y += margin,
                Anchor::Left => x += margin,
                Anchor::Bottom => y += screen.1 as i32 - window.1 as i32 - margin,
                Anchor::Right => x += screen.0 as i32 - window.0 as i32 - margin,
                _ => {},
            }
        }

        (x, y)
    }
}
