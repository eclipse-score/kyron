use crate::internals::net_helper::{create_tcp_listener, ConnectionParameters};
use crate::internals::runtime_helper::Runtime;
use async_runtime::io::{AsyncReadExt, AsyncWrite};
use async_runtime::net::{TcpListener, TcpStream};
use async_runtime::spawn;
use futures::task::noop_waker_ref;
use std::pin::Pin;
use std::task::{Context, Poll};
use test_scenarios_rust::scenario::{Scenario, ScenarioGroup, ScenarioGroupImpl};
use tracing::info;

// Implementation with read and write using AsyncWrite trait directly
async fn handle_connection_with_response(mut stream: TcpStream) {
    // Addresses.
    let peer_addr = stream.peer_addr().expect("Failed to get peer address");
    let local_addr = stream.local_addr().expect("Failed to get local address");
    info!(peer_addr = format!("{peer_addr:?}"), local_addr = format!("{local_addr:?}"));

    // Read.
    let mut buf = [0u8; 1024];
    {
        match stream.read(&mut buf).await {
            Ok(0) => {
                info!("Client closed connection");
                return;
            }
            Ok(n) => {
                info!("Read {n} bytes");
            }
            Err(e) => {
                info!("Read error: {e:?}");
                return;
            }
        }

        let message_read = String::from_utf8(buf.to_vec()).expect("Failed to convert string from bytes");
        let message_read_trim = message_read.trim_end_matches(char::from(0));
        info!(message_read = message_read_trim);
    }

    // Write.
    {
        let mut pinned = Pin::new(&mut stream);
        let waker = noop_waker_ref();
        let mut ctx = Context::from_waker(waker);

        let mut written = 0;
        while written < buf.len() {
            match AsyncWrite::poll_write(pinned.as_mut(), &mut ctx, &buf[written..buf.len()]) {
                Poll::Ready(Ok(0)) => {
                    info!("Client closed connection during write");
                    break;
                }
                Poll::Ready(Ok(m)) => {
                    written += m;
                    info!("Written {m} bytes");
                }
                Poll::Ready(Err(e)) => {
                    info!("Write error: {e:?}");
                    break;
                }
                Poll::Pending => {
                    info!("Write would block, try again later");
                    continue;
                }
            }
        }
    }
}

struct Smoke;

impl Scenario for Smoke {
    fn name(&self) -> &str {
        "smoke"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();
        let connection_parameters = ConnectionParameters::from_json(input).expect("Failed to parse connection parameters");

        let _ = rt.block_on(async move {
            let listener = create_tcp_listener(connection_parameters).await;
            info!("TCP server listening on {}", listener.local_addr().expect("Failed to get local address"));

            // Loop is expected to be terminated by SIGTERM from Python test.
            loop {
                let (stream, _addr) = listener.accept().await.expect("Failed to accept TCP connection");
                spawn(handle_connection_with_response(stream));
            }
        });

        Ok(())
    }
}

async fn print_listener_ttl(listener: TcpListener) {
    let ttl = listener.ttl().expect("Failed to get TTL value");
    info!(ttl);
}

struct SetGetTtl;

impl Scenario for SetGetTtl {
    fn name(&self) -> &str {
        "set_get_ttl"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();
        let connection_parameters = ConnectionParameters::from_json(input).expect("Failed to parse connection parameters");

        let _ = rt.block_on(async move {
            let listener = create_tcp_listener(connection_parameters).await;

            let _ = spawn(print_listener_ttl(listener)).await;
            Ok(0)
        });

        Ok(())
    }
}

pub fn tcp_listener_group() -> Box<dyn ScenarioGroup> {
    Box::new(ScenarioGroupImpl::new("tcp_listener", vec![Box::new(Smoke), Box::new(SetGetTtl)], vec![]))
}
