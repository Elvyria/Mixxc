use std::str::FromStr;

use bitflags::bitflags;

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

#[derive(Debug)]
pub struct ParseAnchorError(pub String);

impl FromStr for Anchor {
    type Err = ParseAnchorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.as_bytes().first().map(u8::to_ascii_lowercase) {
            Some(b't') => Ok(Anchor::Top),
            Some(b'l') => Ok(Anchor::Left),
            Some(b'b') => Ok(Anchor::Bottom),
            Some(b'r') => Ok(Anchor::Right),
            _          => Err(ParseAnchorError(s.to_owned())),
        }
    }
}
