use bitflags::bitflags;

use crate::error::CLIError;

bitflags! {
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
