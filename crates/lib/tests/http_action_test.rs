use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

use proc_siding_lib::action::{HttpAction, HttpPostAction, PressureAction};

/// Captured request line from a mock server.
struct CapturedRequest {
  method: String,
  path: String,
}

/// Spins up a TCP listener that accepts one connection, reads the request
/// line, sends a 200 response, and forwards the parsed request through
/// the channel.
fn mock_tcp_server() -> (String, mpsc::Receiver<CapturedRequest>) {
  let listener =
    TcpListener::bind("127.0.0.1:0").expect("Failed to bind mock TCP server");
  let addr = listener.local_addr().unwrap();
  let url = format!("http://127.0.0.1:{}/{}", addr.port(), "test-path");
  let (tx, rx) = mpsc::channel();

  thread::spawn(move || {
    let (mut stream, _) = listener.accept().expect("Failed to accept");
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();

    let parts: Vec<&str> = request_line.trim().splitn(3, ' ').collect();
    tx.send(CapturedRequest {
      method: parts[0].to_string(),
      path: parts[1].to_string(),
    })
    .unwrap();

    stream
      .write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n")
      .unwrap();
  });

  (url, rx)
}

#[test]
fn http_post_action_sends_post() {
  let (url, rx) = mock_tcp_server();
  let action = HttpPostAction {
    pressure_url: url,
    clear_url: "http://127.0.0.1:1/unused".to_string(),
  };

  action.on_pressure().expect("on_pressure should succeed");

  let captured = rx.recv().expect("Should receive captured request");
  assert_eq!(captured.method, "POST");
  assert_eq!(captured.path, "/test-path");
}

#[test]
fn http_action_sends_configured_method() {
  let (url, rx) = mock_tcp_server();
  let action = HttpAction {
    pressure_url: url,
    pressure_method: "PUT".to_string(),
    clear_url: "http://127.0.0.1:1/unused".to_string(),
    clear_method: "DELETE".to_string(),
  };

  action.on_pressure().expect("on_pressure should succeed");

  let captured = rx.recv().expect("Should receive captured request");
  assert_eq!(captured.method, "PUT");
  assert_eq!(captured.path, "/test-path");
}

#[test]
fn http_action_clear_sends_clear_method() {
  let (url, rx) = mock_tcp_server();
  let action = HttpAction {
    pressure_url: "http://127.0.0.1:1/unused".to_string(),
    pressure_method: "PUT".to_string(),
    clear_url: url,
    clear_method: "DELETE".to_string(),
  };

  action.on_clear().expect("on_clear should succeed");

  let captured = rx.recv().expect("Should receive captured request");
  assert_eq!(captured.method, "DELETE");
  assert_eq!(captured.path, "/test-path");
}

#[test]
fn http_action_connection_refused() {
  let action = HttpAction {
    pressure_url: "http://127.0.0.1:1/should-fail".to_string(),
    pressure_method: "PUT".to_string(),
    clear_url: "http://127.0.0.1:1/should-fail".to_string(),
    clear_method: "DELETE".to_string(),
  };

  let err = action.on_pressure().unwrap_err();
  let msg = format!("{err}");
  assert!(
    msg.contains("connect failed"),
    "Expected connect failure, got: {msg}"
  );
}

#[test]
fn http_action_malformed_url() {
  let action = HttpAction {
    pressure_url: "ftp://not-http/path".to_string(),
    pressure_method: "GET".to_string(),
    clear_url: "ftp://not-http/path".to_string(),
    clear_method: "GET".to_string(),
  };

  let err = action.on_pressure().unwrap_err();
  let msg = format!("{err}");
  assert!(
    msg.contains("http://") || msg.contains("unix:"),
    "Expected URL scheme error, got: {msg}"
  );
}

#[cfg(unix)]
mod unix_socket_tests {
  use super::*;
  use std::os::unix::net::UnixListener;

  fn mock_unix_server(
    socket_path: &std::path::Path,
  ) -> mpsc::Receiver<CapturedRequest> {
    let listener = UnixListener::bind(socket_path)
      .expect("Failed to bind mock Unix socket server");
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
      let (mut stream, _) = listener.accept().expect("Failed to accept");
      let mut reader = BufReader::new(stream.try_clone().unwrap());
      let mut request_line = String::new();
      reader.read_line(&mut request_line).unwrap();

      let parts: Vec<&str> = request_line.trim().splitn(3, ' ').collect();
      tx.send(CapturedRequest {
        method: parts[0].to_string(),
        path: parts[1].to_string(),
      })
      .unwrap();

      stream
        .write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    });

    rx
  }

  #[test]
  fn http_action_over_unix_socket() {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    let sock_path = dir.path().join("test.sock");
    let rx = mock_unix_server(&sock_path);

    let url = format!("unix:{}:/mute", sock_path.display());
    let action = HttpAction {
      pressure_url: url,
      pressure_method: "PUT".to_string(),
      clear_url: "unix:/nonexistent.sock:/unused".to_string(),
      clear_method: "DELETE".to_string(),
    };

    action
      .on_pressure()
      .expect("on_pressure over Unix socket should succeed");

    let captured = rx.recv().expect("Should receive captured request");
    assert_eq!(captured.method, "PUT");
    assert_eq!(captured.path, "/mute");
  }

  #[test]
  fn http_action_unix_socket_connection_refused() {
    let action = HttpAction {
      pressure_url: "unix:/tmp/nonexistent-proc-siding-test.sock:/path"
        .to_string(),
      pressure_method: "PUT".to_string(),
      clear_url: "unix:/tmp/nonexistent-proc-siding-test.sock:/path"
        .to_string(),
      clear_method: "DELETE".to_string(),
    };

    let err = action.on_pressure().unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("connect"), "Expected connect failure, got: {msg}");
  }
}
