//!
//! server
//!

#![warn(clippy::pedantic)]
// #![warn(clippy::cargo)]

use redis_lib::{server, DEFAULT_PORT};

use clap::Parser;
use tokio::net::TcpListener;
use tokio::signal;
 

#[cfg(feature = "otel")] 
use opentelemetry::global;
#[cfg(feature = "otel")] 
use opentelemetry::sdk::trace as sdktrace; 
#[cfg(feature = "otel")] 
// OpenTelemetry-specific types (such as `OpenTelemetryLayer`)
use tracing_subscriber::{
    fmt, layer::SubscriberExt, util::SubscriberInitExt, util::TryInitError, EnvFilter,
};

/// cargo install --locked tokio-console
///
/// build with: RUSTFLAGS=--cfg tokio_unstable
///
/// tokio-console --lang en_US.UTF-8 
fn console(){ 
    console_subscriber::init();
}
 
#[tokio::main]
pub async fn main() -> redis_lib::Result<()> {
    set_up_logging()?;
    // console();

    let cmd = CliCommand::parse();
    // let port = cmd.port.unwrap_or(DEFAULT_PORT);

    let listener = TcpListener::bind(&format!("127.0.0.1:{}", cmd.port)).await?;
     
    server::run(listener, signal::ctrl_c()).await;

    Ok(())
}

#[derive(Parser, Debug)]
#[command(name = "redis-server", version, author, about = "A Redis server")]
struct CliCommand {
    #[clap(long, default_value_t = DEFAULT_PORT)]
    port: u16,
}

#[cfg(not(feature = "otel"))]
fn set_up_logging() -> redis_lib::Result<()> { 
    tracing_subscriber::fmt::try_init()
}

#[cfg(feature = "otel")]
fn set_up_logging() -> Result<(), TryInitError> { 
    global::set_text_map_propagator(XrayPropagator::default());

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .with_trace_config(
            sdktrace::config()
                .with_sampler(sdktrace::Sampler::AlwaysOn)
                // Needed in order to convert the trace IDs into an Xray-compatible format
                .with_id_generator(sdktrace::XrayIdGenerator::default()),
        )
        .install_simple()
        .expect("Unable to initialize OtlpPipeline");

    // Create a tracing layer with the configured tracer
    let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    // Parse an `EnvFilter` configuration from the `RUST_LOG`
    // environment variable.
    let filter = EnvFilter::from_default_env();

    // Use the tracing subscriber `Registry`, or any other subscriber
    // that impls `LookupSpan`
    tracing_subscriber::registry()
        .with(opentelemetry)
        .with(filter)
        .with(fmt::Layer::default())
        .try_init()
}
