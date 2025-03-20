//!
//! spawning a task per connection.
//!

use crate::{Command, Connection, Db, DbDropGuard, Shutdown};
use std::future::Future;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio::time::{self, Duration};
use tracing::{debug, error, info, instrument};

const MAX_CONNECTIONS: usize = 500;

/// Server listener state. Created in the `run` call.
#[derive(Debug)]
struct Listener {
    /// Shared database handle.
    ///
    /// Contains the key / value store as well as the broadcast channels for pub/sub.
    db_holder: DbDropGuard,

    /// supplied by the `run` caller.
    tcp_listener: TcpListener,

    limit_connections: Arc<Semaphore>,

    /// Broadcasts a shutdown signal to all active connections.
    ///
    /// When shutdown is initiated, a `()` value is sent via the broadcast::Sender.
    shutdown_sender: broadcast::Sender<()>,

    /// Used as part of the graceful shutdown process to wait for client
    /// connections to complete processing.
    ///
    /// Once all handler tasks
    /// complete, all clones of the `Sender` are also dropped. This results in
    /// `shutdown_complete_rx.recv()` completing with `None`. At this point, it
    /// is safe to exit the server process.
    shutdown_complete_tx: mpsc::Sender<()>,
}

/// spawn a task to handle each inbound tcp connection. The server runs until the
/// `shutdown` future completes,
///
/// `tokio::signal::ctrl_c()` can be used as the `shutdown` argument.
pub async fn run(listener: TcpListener, shutdown: impl Future) {
    // When the provided `shutdown` future completes, we must send a shutdown
    // message to all active connections.
    let (shutdown_sender, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    let mut server = Listener {
        tcp_listener: listener,
        db_holder: DbDropGuard::new(),
        limit_connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        shutdown_sender,
        shutdown_complete_tx,
    };

    // Concurrently run the server and listen for the `shutdown` signal.
    tokio::select! {
        res = server.run() => {
            if let Err(err) = res {
                error!(cause = %err, "failed to accept");
            }
        }
        _ = shutdown => {
            info!("The shutdown signal has been received");
        }
    }

    let Listener {
        shutdown_complete_tx,
        shutdown_sender,
        ..
    } = server;
    drop(shutdown_sender);
    // Drop final `Sender` so the `Receiver` below can complete
    drop(shutdown_complete_tx);

    // Wait for all active connections to finish processing.
    let _ = shutdown_complete_rx.recv().await;
}

impl Listener {
    /// performs the TCP listening and initialization of per-connection state.
    ///
    /// For each inbound connection, spawn a task to process that connection.
    async fn run(&mut self) -> crate::Result<()> {
        info!("accepting inbound connections");

        loop {
            // Wait for a permit to become available
            let permit = self
                .limit_connections
                .clone()
                .acquire_owned()
                .await
                .unwrap();

            let socket = self.accept().await?;

            let db = self.db_holder.db();
            let shutdown = Shutdown::new(self.shutdown_sender.subscribe()); 
            // Spawn a new task to process the connections.
            tokio::spawn(async move {
                if let Err(err) = process(Connection::new(socket), db, shutdown).await {
                    error!(cause = ?err, "connection error");
                }

                // returns the permit back to the semaphore.
                drop(permit);
            });
        }
    }

    /// Accept an inbound connection.
    ///
    /// Errors are handled by backing off and retrying. An exponential backoff
    /// strategy is used. If accepting fails on the 6th try after
    /// waiting for 64 seconds, then this function returns with an error.
    async fn accept(&mut self) -> crate::Result<TcpStream> {
        let mut backoff = 1;

        loop {
            match self.tcp_listener.accept().await {
                Ok((socket, _)) => return Ok(socket),
                Err(err) => {
                    if backoff > 64 {
                        return Err(err.into());
                    }
                }
            }

            // Pause execution
            time::sleep(Duration::from_secs(backoff)).await;

            backoff *= 2;
        }
    }
}

/// read request frames from the socket and processed. write responses back to the socket.
///
/// When the shutdown signal is received, the connection is processed until
/// it reaches a safe state, at which point it is terminated.
#[instrument]
async fn process(mut connection: Connection, db: Db, mut shutdown: Shutdown) -> crate::Result<()> {
    // As long as the shutdown signal has not been received, try to read a new request frame.
    while !shutdown.is_shutdown() {
        // While reading a request frame, also listen for the shutdown signal.
        let maybe_frame = tokio::select! {
            res = connection.read_frame() => res?,
            _ = shutdown.recv() => {
                return Ok(());
            }
        };

        // If `None` is returned from `read_frame()` then the peer closed
        // the socket. There is no further work to do and the task can be terminated.
        let frame = match maybe_frame {
            Some(frame) => frame,
            None => return Ok(()),
        };

        let cmd = Command::from_frame(frame)?;
        debug!(?cmd);

        cmd.apply(&db, &mut connection, &mut shutdown).await?;
    }

    Ok(())
}
