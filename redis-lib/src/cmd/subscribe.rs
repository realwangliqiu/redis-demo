use crate::cmd::{Parse, ParseError, Protocol, Unknown};
use crate::frame::PushFrame;
use crate::{Command, Connection, Db, Frame, Shutdown};
use bytes::Bytes;
use std::pin::Pin;
use tokio::select;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, StreamMap};

/// client subscribes to one or more channels.
///
/// Once the client enters the subscribed state, it is not supposed to issue any
/// other commands, except for additional `SUBSCRIBE`, `PSUBSCRIBE`, `UNSUBSCRIBE`,
/// `PUNSUBSCRIBE`, `PING` and `QUIT` commands.
#[derive(Debug)]
pub struct Subscribe {
    channels: Vec<String>,
}

/// client unsubscribes one or more channels.
///
/// When no channels are specified, the client unsubscribes from all the subscribed channels.
#[derive(Clone, Debug)]
pub struct Unsubscribe {
    channels: Vec<String>,
}

/// Stream of messages. The stream receives messages from the `broadcast::Receiver`.
type Messages = Pin<Box<dyn Stream<Item = Bytes> + Send>>;

impl Subscribe {
    pub(crate) fn new(channels: Vec<String>) -> Subscribe {
        Subscribe { channels }
    }

    /// # Format
    ///
    /// Expects an array frame containing two or more entries.
    ///
    /// ```text
    /// SUBSCRIBE [channel [channel ...]]
    /// ```
    pub(crate) fn parse_frames(parse: &mut Parse) -> crate::Result<Subscribe> {
        use ParseError::EndOfStream;

        let mut channels = vec![parse.next_string()?];

        loop {
            match parse.next_string() {
                Ok(s) => channels.push(s),
                Err(EndOfStream) => break,
                Err(err) => return Err(err.into()),
            }
        }

        Ok(Subscribe { channels })
    }

    /// Apply the `Subscribe` command to the specified `Db` instance.
    ///
    /// [apply]: crate::cmd::Command::apply
    pub(crate) async fn apply(
        mut self,
        db: &Db,
        dst: &mut Connection,
        shutdown: &mut Shutdown,
    ) -> crate::Result<()> {
        // An individual client may subscribe to multiple channels and may
        // dynamically add and remove channels from its subscription set. To
        // handle this, a `StreamMap` is used to track active subscriptions.
        let mut subscriptions = StreamMap::new();

        loop {
            // `self.channels` is used to track additional channels to subscribe
            // to. When new `SUBSCRIBE` commands are received during the
            // execution of `apply`, the new channels are pushed onto this vec.
            for channel in self.channels.drain(..) {
                subscribe_to_channel(channel, &mut subscriptions, db, dst).await?;
            }

            // Wait for one of the following to happen:
            //
            // - Receive a message from one of the subscribed channels.
            // - Receive a subscribe or unsubscribe command from the client.
            // - A server shutdown signal.
            select! {
                Some((channel, msg)) = subscriptions.next() => {
                    dst.write_frame(&make_message_frame(channel, msg)).await?;
                }
                res = dst.read_frame() => {
                    let frame = match res? {
                        Some(frame) => frame,
                        // This happens if the remote client has disconnected.
                        None => return Ok(())
                    };

                    handle_command(
                        frame,
                        &mut self.channels,
                        &mut subscriptions,
                        dst,
                    ).await?;
                }
                _ = shutdown.recv() => {
                    return Ok(());
                }
            };
        }
    }
}

async fn subscribe_to_channel(
    channel: String,
    subscriptions: &mut StreamMap<String, Messages>,
    db: &Db,
    dst: &mut Connection,
) -> crate::Result<()> {
    let mut rx = db.subscribe(channel.clone());

    let rx = Box::pin(async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(msg) => yield msg,
                // If we lagged in consuming messages, just resume.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => break,
            }
        }
    });

    // Track subscription in this client's subscription set.
    subscriptions.insert(channel.clone(), rx);

    // Respond with the successful subscription
    let frame = make_subscribe_frame(channel, subscriptions.len());
    dst.write_frame(&frame).await?;

    Ok(())
}

/// Handle a command received while inside `Subscribe::apply`.
/// Only `SUBSCRIBE` and `UNSUBSCRIBE` commands are permitted in this context.
///
/// Any new subscriptions are appended to `subscribe_to` instead of modifying `subscriptions`.
async fn handle_command(
    frame: Frame,
    subscribe_to: &mut Vec<String>,
    subscriptions: &mut StreamMap<String, Messages>,
    dst: &mut Connection,
) -> crate::Result<()> {
    match Command::from_frame(frame)? {
        Command::Subscribe(subscribe) => {
            subscribe_to.extend(subscribe.channels.into_iter());
        }
        Command::Unsubscribe(mut unsubscribe) => {
            // If no channels are specified, this requests unsubscribing from all channels.
            if unsubscribe.channels.is_empty() {
                unsubscribe.channels = subscriptions.keys().map(|s| s.to_string()).collect();
            }

            for channel in unsubscribe.channels {
                subscriptions.remove(&channel);

                let resp_frame = make_unsubscribe_frame(channel, subscriptions.len());
                dst.write_frame(&resp_frame).await?;
            }
        }
        command => {
            let cmd = Unknown::new(command.get_name());
            cmd.apply(dst).await?;
        }
    }
    Ok(())
}

fn make_subscribe_frame(channel: String, n_subs: usize) -> Frame {
    let mut response = vec![];
    response.push_bulk(Bytes::from_static(b"subscribe"));
    response.push_bulk(Bytes::from(channel));
    response.push_int(n_subs as u64);

    response.into()
}

fn make_unsubscribe_frame(channel: String, n_subs: usize) -> Frame {
    let mut response = vec![];
    response.push_bulk(Bytes::from_static(b"unsubscribe"));
    response.push_bulk(Bytes::from(channel));
    response.push_int(n_subs as u64);

    response.into()
}

/// Creates a message informing the client about a new message on a channel that
/// the client subscribes to.
fn make_message_frame(channel: String, msg: Bytes) -> Frame {
    let mut response = vec![];
    response.push_bulk(Bytes::from_static(b"message"));
    response.push_bulk(Bytes::from(channel));
    response.push_bulk(msg);

    response.into()
}

impl Unsubscribe {
    pub(crate) fn new(channels: &[String]) -> Unsubscribe {
        Unsubscribe {
            channels: channels.to_vec(),
        }
    }

    /// # Format
    ///
    /// Expects an array frame containing at least one entry.
    ///
    /// ```text
    /// UNSUBSCRIBE [channel [channel ...]]
    /// ```
    pub(crate) fn parse_frames(parse: &mut Parse) -> Result<Unsubscribe, ParseError> {
        use ParseError::EndOfStream;

        let mut channels = vec![];

        loop {
            match parse.next_string() {
                Ok(s) => channels.push(s),
                Err(EndOfStream) => break,
                Err(err) => return Err(err),
            }
        }

        Ok(Unsubscribe { channels })
    }
}

impl Protocol for Subscribe {
    fn into_frame(self) -> Frame {
        let mut frame = vec![];
        frame.push_bulk(Bytes::from("subscribe".as_bytes()));
        for channel in self.channels {
            frame.push_bulk(Bytes::from(channel.into_bytes()));
        }

        frame.into()
    }
}

impl Protocol for Unsubscribe {
    fn into_frame(self) -> Frame {
        let mut frame = vec![];
        frame.push_bulk(Bytes::from("unsubscribe".as_bytes()));

        for channel in self.channels {
            frame.push_bulk(Bytes::from(channel.into_bytes()));
        }

        frame.into()
    }
}
