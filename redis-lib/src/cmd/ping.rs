use crate::cmd::Protocol;
use crate::frame::PushFrame;
use crate::{Connection, Frame, Parse, ParseError};
use bytes::Bytes;
use tracing::{debug, instrument};

/// Returns PONG if no argument is provided, otherwise return a copy of the argument as a bulk.
///
/// This command is often used to test if a connection is still alive, or to measure latency.
#[derive(Debug, Default)]
pub struct Ping {
    echo: Option<Bytes>,
}

impl Ping {
    pub fn new(echo: Option<Bytes>) -> Ping {
        Ping { echo }
    }

    /// # Format
    ///
    /// Expects an array frame containing `PING` and an optional message.
    ///
    /// ```text
    /// PING [message]
    /// ```
    pub(crate) fn parse_frames(parse: &mut Parse) -> crate::Result<Ping> {
        match parse.next_bytes() {
            Ok(bytes) => Ok(Ping::new(Some(bytes))),
            Err(ParseError::EndOfStream) => Ok(Ping::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// [apply]: crate::cmd::Command::apply
    #[instrument(skip(self, dst))]
    pub(crate) async fn apply(self, dst: &mut Connection) -> crate::Result<()> {
        let resp_frame = match self.echo {
            None => Frame::Simple("PONG".to_string()),
            Some(bytes) => Frame::Bulk(bytes),
        };

        debug!(?resp_frame);
        // Write the response to the client
        dst.write_frame(&resp_frame).await?;

        Ok(())
    }
}

impl Protocol for Ping {
    fn into_frame(self) -> Frame {
        let mut frame = vec![];
        frame.push_bulk(Bytes::from("ping".as_bytes()));
        if let Some(msg) = self.echo {
            frame.push_bulk(msg);
        }

        frame.into()
    }
}
