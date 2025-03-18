use crate::frame::PushFrame;
use crate::{Connection, Db, Frame, Parse};

use bytes::Bytes;
use tracing::{debug, instrument};

/// Get the value of key.
///
/// If the key does not exist the special value nil is returned. An error is
/// returned if the value stored at key is not a string, because GET only
/// handles string values.
#[derive(Debug)]
pub struct Get {
    key: String,
}

impl Get {
    pub fn new(key: impl ToString) -> Get {
        Get {
            key: key.to_string(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// # Format
    ///
    /// Expects an array frame containing two entries.
    ///
    /// ```text
    /// GET key
    /// ```
    pub(crate) fn parse_frames(parse: &mut Parse) -> crate::Result<Get> {
        // The `GET` string has already been consumed. The next value is the
        // name of the key to get. If the next value is not a string or the
        // input is fully consumed, then an error is returned.
        let key = parse.next_string()?;

        Ok(Get { key })
    }

    /// Apply the `Get` command to the specified `Db` instance.
    /// 
    /// [apply]: redis_lib::cmd::Command::apply
    #[instrument(skip(self, db, dst))]
    pub(crate) async fn apply(self, db: &Db, dst: &mut Connection) -> crate::Result<()> {
        // Get the value from the shared database state
        let response = if let Some(value) = db.get(&self.key) {
            // If a value is present, it is written to the client in "bulk"
            // format.
            Frame::Bulk(value)
        } else {
            // If there is no value, `Null` is written.
            Frame::Null
        };

        debug!(?response);

        // Write the response back to the client
        dst.write_frame(&response).await?;

        Ok(())
    }

    /// Converts the command into an equivalent `Frame`.
    ///
    /// This is called by the client when encoding a `Get` command to send to
    /// the server.
    pub(crate) fn into_frame(self) -> Frame {
        let mut frame = vec![];
        frame.push_bulk(Bytes::from("get".as_bytes()));
        frame.push_bulk(Bytes::from(self.key.into_bytes()));

        frame.into()
    }
}
