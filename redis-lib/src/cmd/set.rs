use crate::cmd::{Parse, ParseError, Protocol};
use crate::frame::PushFrame;
use crate::{Connection, Db, Frame};
use bytes::Bytes;
use std::time::Duration;
use tracing::{debug, instrument};

/// Set `key` `value`.
///
/// If `key` already holds a value, it is overwritten, regardless of its type.
/// Any previous time to live associated with the key is discarded on successful
/// SET operation.
///
/// # Options
///
/// Currently, the following options are supported:
///
/// * EX `seconds` -- Set the specified expire time, in seconds.
/// * PX `milliseconds` -- Set the specified expire time, in milliseconds.
#[derive(Debug)]
pub struct Set {
    key: String,
    value: Bytes,
    /// When to expire the key
    expire: Option<Duration>,
}

impl Set {
    pub fn new(key: impl ToString, value: Bytes, expire: Option<Duration>) -> Set {
        Set {
            key: key.to_string(),
            value,
            expire,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> &Bytes {
        &self.value
    }

    pub fn expire(&self) -> Option<Duration> {
        self.expire
    }

    /// # Format
    ///
    /// Expects an array frame containing at least 3 entries.
    ///
    /// ```text
    /// SET key value [EX seconds|PX milliseconds]
    /// ```
    pub(crate) fn parse_frames(parse: &mut Parse) -> crate::Result<Set> {
        use ParseError::EndOfStream;

        let key = parse.next_string()?;
        let value = parse.next_bytes()?;

        // The expiration is optional. If nothing else follows, then it is `None`.
        let mut expire = None;
        // Attempt to parse `expire`.
        match parse.next_string() {
            Ok(s) if s.to_uppercase() == "EX" => {
                let secs = parse.next_int()?;
                expire = Some(Duration::from_secs(secs));
            }
            Ok(s) if s.to_uppercase() == "PX" => {
                let ms = parse.next_int()?;
                expire = Some(Duration::from_millis(ms));
            }
            // Currently, redis-demo does not support any of the other SET options.
            Ok(_) => return Err("currently `SET` only supports the expiration option".into()),
            // The `EndOfStream` error indicates there is no further data to parse.
            // In this case, it is normal.
            Err(EndOfStream) => {}
            // unexpected error
            Err(err) => return Err(err.into()),
        }

        Ok(Set { key, value, expire })
    }

    /// [apply]: crate::cmd::Command::apply
    #[instrument(skip(self, db, dst))]
    pub(crate) async fn apply(self, db: &Db, dst: &mut Connection) -> crate::Result<()> {
        db.set(self.key, self.value, self.expire);

        let resp_frame = Frame::Simple("OK".to_string());
        debug!(?resp_frame);
        // Write the `resp_frame` to the client
        dst.write_frame(&resp_frame).await?;

        Ok(())
    }
}

impl Protocol for Set {
    fn into_frame(self) -> Frame {
        let mut frame = vec![];
        frame.push_bulk(Bytes::from("set".as_bytes()));
        frame.push_bulk(Bytes::from(self.key.into_bytes()));
        frame.push_bulk(self.value);
        if let Some(ms) = self.expire {
            frame.push_bulk(Bytes::from("px".as_bytes()));
            frame.push_int(ms.as_millis() as u64);
        }

        frame.into()
    }
}
