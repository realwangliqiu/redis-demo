# redis-demo
 
## Running
 
Start the server:

```
cargo run --bin redis-server

cargo run --bin redis-server -- --help
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
 
    
