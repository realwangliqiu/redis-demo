#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]

use redis_lib::{Result, clients::Client};

#[tokio::main]
pub async fn main() -> Result<()> {
    let mut client = Client::connect("127.0.0.1:6379").await?;
    let mut client_2 = Client::connect("127.0.0.1:6379").await?;

    client.set("hello", "world".into()).await?;
    let result = client.get("hello").await?;
    println!("got value from the server; success={:?}", result.is_some());

    // subscribe to channel foo
    let mut subscriber = client.subscribe(vec!["foo".into()]).await?;
    // publish message `bar` on channel foo
    client_2.publish("foo", "bar".into()).await?;
    // await messages on channel foo
    if let Some(msg) = subscriber.next_message().await? {
        println!("channel: {} ==> message = {:?}", msg.channel, msg.content);
    }

    Ok(())
}
