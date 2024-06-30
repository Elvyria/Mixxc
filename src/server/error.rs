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
    #[error("Couldn't establish connection with the PulseAudio server\n{0}")]
    Connection(Code),

    #[error("No connection to the pulse server")]
    NotConnected,

    #[error("Connection to the pulse audio server was terminated")]
    Disconnected,

    #[error("Quit the mainloop")]
    MainloopQuit,

    #[error("{0}")]
    Other(Code),
}

impl From<PAErr> for PulseError {
    fn from(e: PAErr) -> Self {
        use num_traits::FromPrimitive;
        use libpulse_binding::error::Code::*;

        if e.0 == -2 {
            return PulseError::MainloopQuit
        }

        let code = Code::from_i32(e.0).unwrap_or(Code::Unknown);
        match code {
            ConnectionTerminated => PulseError::Disconnected,
            ConnectionRefused | InvalidServer => {
                PulseError::Connection(code)
            },
            _ => PulseError::Other(code),
        }
    }
}
