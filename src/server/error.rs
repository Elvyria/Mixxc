use std::fmt::Debug;

use crate::label;

use libpulse_binding::error::{Code, PAErr};
use thiserror::Error;

#[derive(Error)]
pub enum Error {
    #[error(transparent)]
    Pulse(#[from] PulseError),
}

impl Debug for Error {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "\x1b[7D{}: {} :{}", label::ERROR, label::PULSE, self)
    }
}

#[derive(Error, Debug)]
pub enum PulseError {
    #[error("Couldn't get access to the context")]
    Context,

    #[error("Couldn't establish connection with the PulseAudio server\n{0}")]
    Connection(Code),

    #[error("{0}")]
    Other(Code),

    // Nightly
    // #[error("An unknown error has occured")]
    // Unknown {
        // backtrace: std::backtrace::Backtrace,
    // },
}

impl From<PAErr> for PulseError {
    fn from(e: PAErr) -> Self {
        use num_traits::FromPrimitive;
        use libpulse_binding::error::Code::*;

        let code = Code::from_i32(e.0).unwrap_or(Code::Unknown);
        match code {
            ConnectionRefused | ConnectionTerminated | InvalidServer => {
                PulseError::Connection(code)
            },
            _ => PulseError::Other(code),
        }
    }
}
