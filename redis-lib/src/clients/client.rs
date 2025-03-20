//! Redis client implementation
//!
//! Provides an async connect and methods for issuing the supported commands.

use crate::cmd::{Get, Ping, Protocol, Publish, Set, Subscribe, Unsubscribe};
use crate::{Connection, Frame}; 
use async_stream::try_stream;
use bytes::Bytes;
use std::io::{Error, ErrorKind};
use std::time::Duration;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio_stream::Stream;
use tracing::{debug, instrument};

/// Backed by a single `TcpStream`.
pub struct Client {
    connection: Connection,
}

/// A client that has entered pub/sub mode.
///
/// Once clients subscribe to a channel, they may only perform pub/sub related
/// commands. The `Client` type is transitioned to a `Subscriber` type in order
/// to prevent non-pub/sub methods from being called.
pub struct Subscriber {
    client: Client,
    subscribed_channels: Vec<String>,
}

/// A message received on a subscribed channel.
#[derive(Debug, Clone)]
pub struct Message {
    pub channel: String,
    pub content: Bytes,
}

impl Client {
    /// Establish a connection with the Redis server located at `addr`.
    pub async fn connect<T: ToSocketAddrs>(addr: T) -> crate::Result<Client> { 
        let stream = TcpStream::connect(addr).await?;
        let connection = Connection::new(stream);

        Ok(Client { connection })
    }

    /// [Ping] to the server.
    ///
    /// [Ping]: crate::cmd::Ping 
    #[instrument(skip(self))]
    pub async fn ping(&mut self, msg: Option<Bytes>) -> crate::Result<Bytes> {
        let frame = Ping::new(msg).into_frame();
        debug!(request = ?frame);
        self.connection.write_frame(&frame).await?;

        match self.read_response().await? {
            Frame::Simple(value) => Ok(value.into()),
            Frame::Bulk(value) => Ok(value),
            frame => Err(frame.to_error()),
        }
    }

    /// Get the value of key.
    /// 
    /// # return
    /// 
    /// If the key does not exist the special value `None` is returned.
    #[instrument(skip(self))]
    pub async fn get(&mut self, key: &str) -> crate::Result<Option<Bytes>> {
        // Create a `Get` command for the `key` and convert it to a frame.
        let frame = Get::new(key).into_frame();

        debug!(request = ?frame);

        // Write the frame to the socket. This writes the full frame to the
        // socket, waiting if necessary.
        self.connection.write_frame(&frame).await?;

        // Wait for the response from the server
        //
        // Both `Simple` and `Bulk` frames are accepted. `Null` represents the
        // key not being present and `None` is returned.
        match self.read_response().await? {
            Frame::Simple(value) => Ok(Some(value.into())),
            Frame::Bulk(value) => Ok(Some(value)),
            Frame::Null => Ok(None),
            frame => Err(frame.to_error()),
        }
    }

    /// Set `key` to hold the given `value`.
    ///
    /// If key already holds a value, it is overwritten. Any previous time to
    /// live associated with the key is discarded on successful SET operation. 
    #[instrument(skip(self))]
    pub async fn set(&mut self, key: &str, value: Bytes) -> crate::Result<()> { 
        self.set_cmd(Set::new(key, value, None)).await
    }

    /// Set `key` to hold the given `value`. The value expires after `expiration`
    /// 
    /// If key already holds a value, it is overwritten. Any previous time to
    /// live associated with the key is discarded on a successful SET operation.
    ///
    /// # Note
    ///
    /// This function assumes the client and server stay relatively synchronized in time. 
    /// The real world tends to not be so favorable.
    #[instrument(skip(self))]
    pub async fn set_expires(
        &mut self,
        key: &str,
        value: Bytes,
        expiration: Duration,
    ) -> crate::Result<()> { 
        self.set_cmd(Set::new(key, value, Some(expiration))).await
    }

    /// The core `SET` logic. 
    async fn set_cmd(&mut self, cmd: Set) -> crate::Result<()> { 
        let frame = cmd.into_frame(); 
        debug!(request = ?frame);
 
        self.connection.write_frame(&frame).await?;

        // Wait for the response from the server.
        match self.read_response().await? {
            Frame::Simple(s) if s == "OK" => Ok(()),
            other => Err(other.to_error()),
        }
    }

    /// publish `message` to the given `channel`.
    /// 
    /// # Return
    /// 
    /// Returns the number of subscribers currently listening on the channel.
    #[instrument(skip(self))]
    pub async fn publish(&mut self, channel: &str, message: Bytes) -> crate::Result<u64> { 
        let frame = Publish::new(channel, message).into_frame();
        debug!(request = ?frame);
 
        self.connection.write_frame(&frame).await?;

        // Read the response from server.
        match self.read_response().await? {
            Frame::Integer(num) => Ok(num),
            other => Err(other.to_error()),
        }
    }

    /// Subscribes the client to the given channels.
    ///
    /// Once a client issues a subscribe command, it may no longer issue any
    /// non-pub/sub commands. The function consumes `self` and returns a `Subscriber`.
    #[instrument(skip(self))]
    pub async fn subscribe(mut self, channels: Vec<String>) -> crate::Result<Subscriber> { 
        self.do_subscribe(&channels).await?;
 
        Ok(Subscriber {
            client: self,
            subscribed_channels: channels,
        })
    }
 
    async fn do_subscribe(&mut self, channels: &[String]) -> crate::Result<()> {
        let frame = Subscribe::new(channels.to_vec()).into_frame();
        debug!(request = ?frame);
 
        self.connection.write_frame(&frame).await?;

        // For each channel being subscribed to, the server responds with a confirmation message.  
        for channel in channels { 
            let resp_frame = self.read_response().await?;

            // Verify the server responds.
            match resp_frame {
                Frame::Array(ref frames) => match frames.as_slice() {
                    // The server responds with an array frame in the form of:
                    // [ "subscribe", channel, num-subscribed ]
                    // 
                    [subscribe, channel_name, ..]
                        if *subscribe == "subscribe" && *channel_name == channel => {}
                    _ => return Err(resp_frame.to_error()),
                },
                other => return Err(other.to_error()),
            };
        }

        Ok(())
    }

    /// Reads a response frame from the socket.
    ///
    /// If an `Error` frame is received, it is converted to `Err`.
    async fn read_response(&mut self) -> crate::Result<Frame> {
        let response = self.connection.read_frame().await?;
        debug!(?response);

        match response {
            Some(Frame::Error(msg)) => Err(msg.into()),
            Some(frame) => Ok(frame),
            None => {
                let err = Error::new(ErrorKind::ConnectionReset, "connection reset by server");

                Err(err.into())
            }
        }
    }
}

impl Subscriber {
    /// Returns the set of channels currently subscribed to.
    pub fn get_subscribed(&self) -> &[String] {
        &self.subscribed_channels
    }

    /// Receive the next message published on a subscribed channel, waiting if necessary.
    ///
    /// `None` indicates the subscription has been terminated.
    pub async fn next_message(&mut self) -> crate::Result<Option<Message>> {
        match self.client.connection.read_frame().await? {
            Some(frame) => {
                debug!(?frame);

                match frame {
                    Frame::Array(ref frames) => match frames.as_slice() {
                        [message, channel, content] if *message == "message" => Ok(Some(Message {
                            channel: channel.to_string(),
                            content: Bytes::from(content.to_string()),
                        })),
                        _ => Err(frame.to_error()),
                    },
                    other => Err(other.to_error()),
                }
            }
            None => Ok(None),
        }
    }

    /// Convert the subscriber into a `Stream` yielding new messages published
    /// on subscribed channels.
    ///
    /// `Subscriber` does not implement stream itself as doing so with safe code
    /// is non trivial. The usage of async/await would require a manual Stream
    /// implementation to use `unsafe` code. Instead, a conversion function is
    /// provided and the returned stream is implemented with the help of the
    /// `async-stream` crate.
    pub fn into_stream(mut self) -> impl Stream<Item = crate::Result<Message>> {
        // Uses the `try_stream` macro from the `async-stream` crate. Generators
        // are not stable in Rust. The crate uses a macro to simulate generators
        // on top of async/await. There are limitations, so read the
        // documentation there.
        try_stream! {
            while let Some(message) = self.next_message().await? {
                yield message;
            }
        }
    }

    /// Subscribe to a list of new channels
    #[instrument(skip(self))]
    pub async fn subscribe(&mut self, channels: &[String]) -> crate::Result<()> {
        // Issue the subscribe command
        self.client.do_subscribe(channels).await?;

        // Update the set of subscribed channels.
        self.subscribed_channels
            .extend(channels.iter().map(Clone::clone));

        Ok(())
    }

    /// Unsubscribe to a list of new channels
    #[instrument(skip(self))]
    pub async fn unsubscribe(&mut self, channels: &[String]) -> crate::Result<()> {
        let frame = Unsubscribe::new(channels).into_frame();

        debug!(request = ?frame);

        // Write the frame to the socket
        self.client.connection.write_frame(&frame).await?;

        // if the input channel list is empty, server acknowledges as unsubscribing
        // from all subscribed channels, so we assert that the unsubscribe list received
        // matches the client subscribed one
        let num = if channels.is_empty() {
            self.subscribed_channels.len()
        } else {
            channels.len()
        };

        // Read the response
        for _ in 0..num {
            let response = self.client.read_response().await?;

            match response {
                Frame::Array(ref frame) => match frame.as_slice() {
                    [unsubscribe, channel, ..] if *unsubscribe == "unsubscribe" => {
                        let len = self.subscribed_channels.len();

                        if len == 0 {
                            // There must be at least one channel
                            return Err(response.to_error());
                        }

                        // unsubscribed channel should exist in the subscribed list at this point
                        self.subscribed_channels.retain(|c| *channel != &c[..]);

                        // Only a single channel should be removed from the
                        // list of subscribed channels.
                        if self.subscribed_channels.len() != len - 1 {
                            return Err(response.to_error());
                        }
                    }
                    _ => return Err(response.to_error()),
                },
                frame => return Err(frame.to_error()),
            };
        }

        Ok(())
    }
}
