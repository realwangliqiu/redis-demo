use bytes::Bytes;
use clap::{Parser, Subcommand};
use redis_lib::{DEFAULT_PORT, clients::Client};
use std::num::ParseIntError;
use std::str;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "redis-cli", version, author, about = "Issue Redis commands")]
struct CliCommand {
    #[clap(subcommand)]
    sub_cmd: Command,

    #[clap(long, default_value = "127.0.0.1")]
    host: String,

    #[clap(long, default_value_t = DEFAULT_PORT)]
    port: u16,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// [Ping]: redis_lib::cmd::Ping
    Ping { echo: Option<Bytes> },
    /// [Get]: redis_lib::cmd::Get
    Get { key: String },
    /// [Set]: redis_lib::cmd::Set
    Set {
        key: String,
        value: Bytes,
        #[clap(value_parser = duration_from)]
        expires: Option<Duration>,
    },
    /// [Publish]: redis_lib::cmd::Publish
    Publish { channel: String, message: Bytes },
    /// [Subscribe]: redis_lib::cmd::Subscribe
    Subscribe { channels: Vec<String> },
}

fn duration_from(src: &str) -> Result<Duration, ParseIntError> {
    let ms = src.parse::<u64>()?;
    Ok(Duration::from_millis(ms))
}

/// `flavor = "current_thread"` is used here to make CLI lighter instead of multi-threads.
#[tokio::main(flavor = "current_thread")]
async fn main() -> redis_lib::Result<()> {
    // Enable logging
    tracing_subscriber::fmt::try_init()?;

    let cmd = CliCommand::parse();

    let addr = format!("{}:{}", cmd.host, cmd.port);
    let mut client = Client::connect(&addr).await?;

    // Process subcommand
    match cmd.sub_cmd {
        Command::Ping { echo } => {
            let bytes = client.ping(echo).await?;
            if let Ok(string) = str::from_utf8(&bytes) {
                println!("\"{}\"", string);
            } else {
                println!("{:?}", bytes);
            }
        }
        Command::Get { key } => {
            if let Some(bytes) = client.get(&key).await? {
                if let Ok(string) = str::from_utf8(&bytes) {
                    println!("\"{}\"", string);
                } else {
                    println!("{:?}", bytes);
                }
            } else {
                println!("(nil)");
            }
        }
        Command::Set {
            key,
            value,
            expires: None,
        } => {
            client.set(&key, value).await?;
            println!("OK");
        }
        Command::Set {
            key,
            value,
            expires: Some(expires),
        } => {
            client.set_expires(&key, value, expires).await?;
            println!("OK");
        }
        Command::Publish { channel, message } => {
            client.publish(&channel, message).await?;
            println!("Publish OK");
        }
        Command::Subscribe { channels } => {
            if channels.is_empty() {
                return Err("channel(s) must be provided".into());
            }
            let mut subscriber = client.subscribe(channels).await?;

            // await messages on channels
            while let Some(msg) = subscriber.next_message().await? {
                println!(
                    "got message from the channel: {}; message = {:?}",
                    msg.channel, msg.content
                );
            }
        }
    }

    Ok(())
}
