use crate::internals::net_helper::{create_tcp_listener, ConnectionParameters};
use crate::internals::runtime_helper::Runtime;
use async_runtime::io::AsyncReadExt;
use async_runtime::io::AsyncWrite;
use async_runtime::{net::TcpStream, spawn};
use futures::task::noop_waker_ref;
use std::pin::Pin;
use std::task::{Context, Poll};
use test_scenarios_rust::scenario::Scenario;
use tracing::info;

pub struct TcpServer;

#[allow(dead_code)]
// Implementation with only read, as WriteExt trait is not implemented yet
// https://github.com/qorix-group/inc_orchestrator_internal/issues/291
async fn handle_connection_no_response(mut stream: TcpStream) {
    let mut buf = [0u8; 1024];
    match stream.read(&mut buf).await {
        Ok(0) => {
            info!("Client closed connection");
        }
        Ok(n) => {
            // Try to decode as UTF-8 and log as string
            match std::str::from_utf8(&buf[..n]) {
                Ok(s) => info!(message = "Read bytes as string", headers = s),
                Err(_) => info!(message = "Read bytes (not valid UTF-8)", headers = &buf[..n]),
            }
        }
        Err(e) => {
            info!("Read error: {:?}", e);
        }
    }
}

// Implementation with read and write using AsyncWrite trait directly
async fn handle_connection_with_response(mut stream: TcpStream) {
    let mut buf = [0u8; 1024];
    match stream.read(&mut buf).await {
        Ok(0) => {
            info!("Client closed connection");
        }
        Ok(n) => {
            info!("Read {} bytes: {:?}", n, &buf[..n]);
            // Echo back the received message
            let mut pinned = Pin::new(&mut stream);
            let waker = noop_waker_ref();
            let mut cx = Context::from_waker(waker);

            let mut written = 0;
            while written < n {
                match AsyncWrite::poll_write(pinned.as_mut(), &mut cx, &buf[written..n]) {
                    Poll::Ready(Ok(0)) => {
                        info!("Client closed connection during write");
                        break;
                    }
                    Poll::Ready(Ok(m)) => {
                        written += m;
                    }
                    Poll::Ready(Err(e)) => {
                        info!("Write error: {:?}", e);
                        break;
                    }
                    Poll::Pending => {
                        info!("Write would block, try again later");
                        break;
                    }
                }
            }
        }
        Err(e) => {
            info!("Read error: {:?}", e);
        }
    }
}

impl Scenario for TcpServer {
    fn name(&self) -> &str {
        "basic_server"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();
        let connection_parameters = ConnectionParameters::from_json(input).expect("Failed to parse connection parameters");

        let _ = rt.block_on(async move {
            info!("Program entered engine");

            let listener = create_tcp_listener(connection_parameters).await;
            info!("TCP server listening on {}", listener.local_addr().expect("Failed to get local address"));

            // Loop is expected to be terminated by SIGTERM from Python test.
            loop {
                let (stream, _addr) = listener
                    .accept()
                    .await
                    .map_err(|e| e.to_string())
                    .expect("Failed to accept TCP connection");
                spawn(handle_connection_with_response(stream));
            }
        });

        Ok(())
    }
}
