# mini-redis
mini-http ???
## Running
 
Start the server:

```
cargo run --bin redis-server
cargo run --bin redis-server -- --help
```
  
Then, in a different terminal window, the various client [examples](examples)
can be executed. For example:

```
cargo run --example hello_world
```

Start the CLI client:

```
cargo run --bin redis-cli set foo bar

cargo run --bin redis-cli get foo
```

## OpenTelemetry

If you are running many instances of your application (which is usually the case
when you are developing a cloud service, for example), you need a way to get all
of your trace data out of your host and into a centralized place. There are many
options here, such as Prometheus, Jaeger, DataDog, Honeycomb, AWS X-Ray etc.

We leverage OpenTelemetry, because it's an open standard that allows for a
single data format to be used for all the options mentioned above (and more).
This eliminates the risk of vendor lock-in, since you can switch between
providers if needed.

### AWS X-Ray example

To enable sending traces to X-Ray, use the `otel` feature:
```
RUST_LOG=debug cargo run --bin redis-server --features otel
```

This will switch `tracing` to use `tracing-opentelemetry`. You will need to
have a copy of AWSOtelCollector running on the same host.

For demo purposes, you can follow the setup documented at
https://github.com/aws-observability/aws-otel-collector/blob/main/docs/developers/docker-demo.md#run-a-single-aws-otel-collector-instance-in-docker

## Supported commands

`mini-redis` currently supports the following commands.

* [PING](https://redis.io/commands/ping)
* [GET](https://redis.io/commands/get)
* [SET](https://redis.io/commands/set)
* [PUBLISH](https://redis.io/commands/publish)
* [SUBSCRIBE](https://redis.io/commands/subscribe)

The Redis wire protocol specification can be found
[here](https://redis.io/topics/protocol).

There is no support for persistence yet.

## Tokio patterns

The project demonstrates a number of useful patterns, including:

### TCP server

[`server.rs`](redis-lib/src/server.rs) starts a TCP server that accepts connections,
and spawns a new task per connection. It gracefully handles `accept` errors.

### Client library

[`client.rs`](src/clients/client.rs) shows how to model an asynchronous client. The
various capabilities are exposed as `async` methods.

### State shared across sockets

The server maintains a [`Db`] instance that is accessible from all connected
connections. The [`Db`] instance manages the key-value state as well as pub/sub
capabilities.

[`Db`]: redis-lib/src/db.rs

### Framing

[`connection.rs`](redis-lib/src/connection.rs) and [`frame.rs`](redis-lib/src/frame.rs) show how to
idiomatically implement a wire protocol. The protocol is modeled using an
intermediate representation, the `Frame` structure. `Connection` takes a
`TcpStream` and exposes an API that sends and receives `Frame` values.

### Graceful shutdown

The server implements graceful shutdown. [`tokio::signal`] is used to listen for
a SIGINT. Once the signal is received, shutdown begins. The server stops
accepting new connections. Existing connections are notified to shutdown
gracefully. In-flight work is completed, and the connection is closed.

[`tokio::signal`]: https://docs.rs/tokio/*/tokio/signal/

### Concurrent connection limiting

The server uses a [`Semaphore`] limits the maximum number of concurrent
connections. Once the limit is reached, the server stops accepting new
connections until an existing one terminates.

[`Semaphore`]: https://docs.rs/tokio/*/tokio/sync/struct.Semaphore.html

### Pub/Sub

The server implements non-trivial pub/sub capability. The client may subscribe
to multiple channels and update its subscription at any time. The server
implements this using one [broadcast channel][broadcast] per channel and a
[`StreamMap`] per connection. Clients are able to send subscription commands to
the server to update the active subscriptions.

[broadcast]: https://docs.rs/tokio/*/tokio/sync/broadcast/index.html
[`StreamMap`]: https://docs.rs/tokio-stream/*/tokio_stream/struct.StreamMap.html

### Using a `std::sync::Mutex` in an async application

The server uses a `std::sync::Mutex` and **not** a Tokio mutex to synchronize
access to shared state. See [`db.rs`](redis-lib/src/db.rs) for more details.

### Testing asynchronous code that relies on time

In [`tests/server.rs`](tests/server.rs), there are tests for key expiration.
These tests depend on time passing. In order to make the tests deterministic,
time is mocked out using Tokio's testing utilities.

## Contributing

Contributions to `mini-redis` are welcome. Keep in mind, the goal of the project
is **not** to reach feature parity with real Redis, but to demonstrate
asynchronous Rust patterns with Tokio.

Commands or other features should only be added if doing so is useful to
demonstrate a new pattern.

Contributions should come with extensive comments targeted to new Tokio users.

Contributions that only focus on clarifying and improving comments are very
welcome.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `mini-redis` by you, shall be licensed as MIT, without any
additional terms or conditions.
