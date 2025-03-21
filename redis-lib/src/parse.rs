//!
//! Utility for parsing a command
//!  

use crate::Frame;

use bytes::Bytes;
use std::{fmt, str, vec};

/// construct from `Frame::Array`
#[derive(Debug)]
pub(crate) struct Parse {
    frames: vec::IntoIter<Frame>,
}

#[derive(Debug)]
pub(crate) enum ParseError {
    /// Attempting to extract a value failed due to the frame being fully consumed.
    EndOfStream,

    /// `Other` result in the connection being terminated.
    Other(crate::Error),
}

impl Parse {
    /// Returns `Err` if `frame` is not an array frame.
    pub(crate) fn new(frame: Frame) -> Result<Parse, ParseError> {
        let array = match frame {
            Frame::Array(array) => array,
            other => return Err(format!("protocol error; expected array, got {:?}", other).into()),
        };

        Ok(Parse {
            frames: array.into_iter(),
        })
    }

    fn next(&mut self) -> Result<Frame, ParseError> {
        self.frames.next().ok_or(ParseError::EndOfStream)
    }

    /// covert the next `Frame` as a string.
    pub(crate) fn next_string(&mut self) -> Result<String, ParseError> {
        match self.next()? {
            // Both `Simple` and `Bulk` representation may be strings.
            Frame::Simple(s) => Ok(s),
            Frame::Bulk(bytes) => str::from_utf8(&bytes)
                .map(|s| s.to_string())
                .map_err(|_| "protocol error; invalid string".into()),
            other => Err(format!(
                "protocol error; expected simple frame or bulk frame, got {:?}",
                other
            )
            .into()),
        }
    }

    /// covert the next `Frame` as raw bytes.
    pub(crate) fn next_bytes(&mut self) -> Result<Bytes, ParseError> {
        match self.next()? {
            // Both `Simple` and `Bulk` representation may be raw bytes.
            Frame::Simple(s) => Ok(Bytes::from(s.into_bytes())),
            Frame::Bulk(bytes) => Ok(bytes),
            frame => Err(format!(
                "protocol error; expected simple frame or bulk frame, got {:?}",
                frame
            )
            .into()),
        }
    }

    /// covert the next `Frame` as an integer.
    ///
    /// This includes `Simple`, `Bulk`, and `Integer` frame types.  
    pub(crate) fn next_int(&mut self) -> Result<u64, ParseError> {
        use atoi::atoi;

        const MSG: &str = "protocol error; invalid number";

        match self.next()? {
            Frame::Integer(num) => Ok(num),
            Frame::Simple(data) => atoi::<u64>(data.as_bytes()).ok_or_else(|| MSG.into()),
            Frame::Bulk(data) => atoi::<u64>(&data).ok_or_else(|| MSG.into()),
            other => Err(format!("protocol error; expected int frame but got {:?}", other).into()),
        }
    }

    /// Check if there is any remaining unconsumed `Frame` in the `Parse`.
    pub(crate) fn check_done(&mut self) -> Result<(), ParseError> {
        if self.frames.next().is_none() {
            Ok(())
        } else {
            Err("protocol error; expected end of frame, but there was more".into())
        }
    }
}

impl From<String> for ParseError {
    fn from(src: String) -> ParseError {
        ParseError::Other(src.into())
    }
}

impl From<&str> for ParseError {
    fn from(src: &str) -> ParseError {
        src.to_string().into()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::EndOfStream => "protocol error; unexpected end of stream".fmt(f),
            ParseError::Other(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for ParseError {}
