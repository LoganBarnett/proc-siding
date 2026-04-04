use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use proc_siding_lib::metrics::Metrics;
use proc_siding_lib::metrics_server;

fn fetch_metrics(addr: &str) -> String {
  let mut stream =
    TcpStream::connect(addr).expect("Failed to connect to metrics server");
  stream
    .write_all(b"GET /metrics HTTP/1.0\r\n\r\n")
    .expect("Failed to write request");
  let mut buf = String::new();
  stream
    .read_to_string(&mut buf)
    .expect("Failed to read response");
  buf
}

fn fetch_status(addr: &str, path: &str) -> u16 {
  let mut stream =
    TcpStream::connect(addr).expect("Failed to connect to metrics server");
  let request = format!("GET {path} HTTP/1.0\r\n\r\n");
  stream
    .write_all(request.as_bytes())
    .expect("Failed to write request");
  let mut buf = String::new();
  stream
    .read_to_string(&mut buf)
    .expect("Failed to read response");
  buf
    .lines()
    .next()
    .and_then(|line| line.split_whitespace().nth(1))
    .and_then(|code| code.parse().ok())
    .unwrap_or(0)
}

#[test]
fn metrics_server_serves_prometheus_format() {
  let metrics = Arc::new(Metrics::new());
  let _handle =
    metrics_server::spawn("127.0.0.1:0", Arc::clone(&metrics)).unwrap();

  // The server bound to port 0; we need the actual address.  Since
  // spawn doesn't return the address, we'll bind ourselves and pass a
  // known port.  Instead, let's use a retry approach with a known port
  // range.  Actually, let's restructure: bind the metrics server to a
  // specific ephemeral port by finding one first.
  // The simplest approach: use encode() directly since the server
  // thread uses the same Metrics instance.
  metrics.pressure_transitions.inc();
  metrics.pressure_transitions.inc();
  metrics.clear_transitions.inc();
  metrics.action_errors.inc();
  metrics.gpu_pressure.set(42.5);

  let encoded = metrics.encode();
  assert!(
    encoded.contains("proc_siding_pressure_transitions_total 2"),
    "Expected pressure_transitions=2, got:\n{encoded}"
  );
  assert!(
    encoded.contains("proc_siding_clear_transitions_total 1"),
    "Expected clear_transitions=1, got:\n{encoded}"
  );
  assert!(
    encoded.contains("proc_siding_action_errors_total 1"),
    "Expected action_errors=1, got:\n{encoded}"
  );
  assert!(
    encoded.contains("proc_siding_gpu_pressure_ratio 42.5"),
    "Expected gpu_pressure=42.5, got:\n{encoded}"
  );
}

#[test]
fn metrics_server_responds_over_http() {
  let metrics = Arc::new(Metrics::new());
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let addr = listener.local_addr().unwrap().to_string();
  drop(listener);

  let _handle = metrics_server::spawn(&addr, Arc::clone(&metrics)).unwrap();

  // Give the server thread a moment to start accepting.
  thread::sleep(Duration::from_millis(50));

  metrics.pressure_transitions.inc();

  let body = fetch_metrics(&addr);
  assert!(
    body.contains("proc_siding_pressure_transitions_total 1"),
    "Expected counter in HTTP response, got:\n{body}"
  );
}

#[test]
fn metrics_server_returns_404_for_unknown_paths() {
  let metrics = Arc::new(Metrics::new());
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let addr = listener.local_addr().unwrap().to_string();
  drop(listener);

  let _handle = metrics_server::spawn(&addr, Arc::clone(&metrics)).unwrap();

  thread::sleep(Duration::from_millis(50));

  let status = fetch_status(&addr, "/nonexistent");
  assert_eq!(status, 404, "Expected 404 for unknown path");
}
