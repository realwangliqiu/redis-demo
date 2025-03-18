//! The major components are:
//! * `server`:   
//! * `client`:    
//! * `cmd`: implementations of the supported Redis commands.
//! * `frame`: represents a single Redis protocol frame.  

#![warn(clippy::pedantic)]
#![warn(clippy::cargo)] 

pub mod clients;
pub use clients::{BlockingClient, BufferedClient, Client};

pub mod cmd;
pub use cmd::Command;

mod connection;
pub use connection::Connection;

pub mod frame;
pub use frame::Frame;

mod db;
use db::Db;
use db::DbDropGuard;

mod parse;
use parse::{Parse, ParseError};

pub mod server;

mod shutdown;
use shutdown::Shutdown;

/// Default port that a redis server listens on.
pub const DEFAULT_PORT: u16 = 6379;

/// simple Error. It should be specifically defined by enum.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub type Result<T> = std::result::Result<T, Error>;
