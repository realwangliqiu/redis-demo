use crate::{Connection, Frame};
use tracing::{debug, instrument};

/// This is not a real `Redis` command.
#[derive(Debug)]
pub struct Unknown {
    command_name: String,
}

impl Unknown { 
    pub(crate) fn new(key: impl ToString) -> Unknown {
        Unknown {
            command_name: key.to_string(),
        }
    }
 
    pub(crate) fn get_name(&self) -> &str {
        &self.command_name
    }

    /// Responds to the client, indicating the command is not recognized.
    /// 
    /// [apply]: crate::cmd::Command::apply
    #[instrument(skip(self, dst))]
    pub(crate) async fn apply(self, dst: &mut Connection) -> crate::Result<()> {
        let resp_frame = Frame::Error(format!("ERR unknown command '{}'", self.command_name));

        debug!(?resp_frame); 
        dst.write_frame(&resp_frame).await?;
        
        Ok(())
    }
}
