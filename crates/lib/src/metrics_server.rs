use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

use tracing::{debug, error, info, warn};

use crate::metrics::SharedMetrics;

/// Spawns a background thread serving Prometheus metrics over HTTP on the
/// given listen address.  Returns the join handle.
pub fn spawn(
  listen: &str,
  metrics: SharedMetrics,
) -> Result<thread::JoinHandle<()>, MetricsServerBindError> {
  let listener =
    TcpListener::bind(listen).map_err(|source| MetricsServerBindError {
      listen: listen.to_string(),
      source,
    })?;
  let addr =
    listener
      .local_addr()
      .map_err(|source| MetricsServerBindError {
        listen: listen.to_string(),
        source,
      })?;
  info!(%addr, "Metrics server listening");

  let handle = thread::spawn(move || {
    for stream in listener.incoming() {
      let mut stream = match stream {
        Ok(s) => s,
        Err(e) => {
          warn!(error = %e, "Failed to accept metrics connection");
          continue;
        }
      };

      let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
          warn!(error = %e, "Failed to clone metrics stream");
          continue;
        }
      });

      // Read the request line to determine the path.
      let mut request_line = String::new();
      if let Err(e) = reader.read_line(&mut request_line) {
        warn!(error = %e, "Failed to read metrics request line");
        continue;
      }

      let path = request_line.split_whitespace().nth(1).unwrap_or("/");

      debug!(path, "Metrics request");

      let (status, body) = if path == "/metrics" {
        ("200 OK", metrics.encode())
      } else {
        ("404 Not Found", "Not found\n".to_string())
      };

      let response = format!(
        "HTTP/1.0 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
      );

      if let Err(e) = stream.write_all(response.as_bytes()) {
        error!(error = %e, "Failed to write metrics response");
      }
    }
  });

  Ok(handle)
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to bind metrics server on {listen}: {source}")]
pub struct MetricsServerBindError {
  listen: String,
  #[source]
  source: std::io::Error,
}
