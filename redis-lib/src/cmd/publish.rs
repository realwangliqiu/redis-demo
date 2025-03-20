use crate::{Connection, Db, Frame, Parse};
use crate::cmd::Protocol;
use crate::frame::PushFrame;
use bytes::Bytes;
  
/// Send a message into a specific channel. 
/// Consumers may subscribe to channels in order to receive the messages. 
#[derive(Debug)]
pub struct Publish {
    channel: String, 
    message: Bytes,
}

impl Publish { 
    pub(crate) fn new(channel: impl ToString, message: Bytes) -> Publish {
        Publish {
            channel: channel.to_string(),
            message,
        }
    }

    
    /// # Format
    ///
    /// Expects an array frame containing three entries.
    ///
    /// ```text
    /// PUBLISH channel message
    /// ```
    pub(crate) fn parse_frames(parse: &mut Parse) -> crate::Result<Publish> {  
        let channel = parse.next_string()?; 
        let message = parse.next_bytes()?;

        Ok(Publish { channel, message })
    }
 
    /// [apply]: crate::cmd::Command::apply
    pub(crate) async fn apply(self, db: &Db, dst: &mut Connection) -> crate::Result<()> {
        // Calling `db.publish` dispatches the message into the appropriate channel. 
        let n_subscribers = db.publish(&self.channel, self.message);
        let frame = Frame::Integer(n_subscribers as u64);

        dst.write_frame(&frame).await?;

        Ok(())
    }
}

impl Protocol for Publish {
    fn into_frame(self) -> Frame {
        let mut frame = vec![];
        frame.push_bulk(Bytes::from("publish".as_bytes()));
        frame.push_bulk(Bytes::from(self.channel.into_bytes()));
        frame.push_bulk(self.message);

        frame.into()
    }
}
