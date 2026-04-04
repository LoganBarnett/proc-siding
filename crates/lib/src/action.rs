use std::io::{Read, Write};
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
  #[error("HTTP {method} to {url} failed: {detail}")]
  HttpRequest {
    method: String,
    url: String,
    detail: String,
  },

  #[error("Command {cmd:?} failed: {detail}")]
  Exec { cmd: String, detail: String },
}

/// Outbound operations triggered on pressure state transitions.
pub trait PressureAction: Send + Sync {
  fn on_pressure(&self) -> Result<(), ActionError>;
  fn on_clear(&self) -> Result<(), ActionError>;
}

// ── HttpPostAction ─────────────────────────────────────────────────────────────

pub struct HttpPostAction {
  pub pressure_url: String,
  pub clear_url: String,
}

impl PressureAction for HttpPostAction {
  fn on_pressure(&self) -> Result<(), ActionError> {
    http_request("POST", &self.pressure_url)
  }

  fn on_clear(&self) -> Result<(), ActionError> {
    http_request("POST", &self.clear_url)
  }
}

// ── HttpAction ─────────────────────────────────────────────────────────────────

pub struct HttpAction {
  pub pressure_url: String,
  pub pressure_method: String,
  pub clear_url: String,
  pub clear_method: String,
}

impl PressureAction for HttpAction {
  fn on_pressure(&self) -> Result<(), ActionError> {
    http_request(&self.pressure_method, &self.pressure_url)
  }

  fn on_clear(&self) -> Result<(), ActionError> {
    http_request(&self.clear_method, &self.clear_url)
  }
}

// ── HTTP transport ─────────────────────────────────────────────────────────────

/// Parsed target for an HTTP request.
enum HttpTarget {
  Tcp {
    host: String,
    port: u16,
    path: String,
  },
  #[cfg(unix)]
  Unix {
    socket_path: String,
    url_path: String,
  },
}

/// Performs a plain HTTP/1.0 request with an empty body.  Supports both TCP
/// URLs (`http://host:port/path`) and Unix socket URLs
/// (`unix:/path/to/sock:/url-path`).
fn http_request(method: &str, url: &str) -> Result<(), ActionError> {
  let target = parse_url(url).map_err(|detail| ActionError::HttpRequest {
    method: method.to_string(),
    url: url.to_string(),
    detail,
  })?;

  match target {
    HttpTarget::Tcp { host, port, path } => {
      http_over_tcp(method, url, &host, port, &path)
    }
    #[cfg(unix)]
    HttpTarget::Unix {
      socket_path,
      url_path,
    } => http_over_unix(method, url, &socket_path, &url_path),
  }
}

fn http_over_tcp(
  method: &str,
  url: &str,
  host: &str,
  port: u16,
  path: &str,
) -> Result<(), ActionError> {
  let mut stream =
    TcpStream::connect(format!("{host}:{port}")).map_err(|e| {
      ActionError::HttpRequest {
        method: method.to_string(),
        url: url.to_string(),
        detail: format!("connect failed: {e}"),
      }
    })?;

  let request = format!(
        "{method} {path} HTTP/1.0\r\nHost: {host}:{port}\r\nContent-Length: 0\r\n\r\n"
    );
  write_and_check(&mut stream, request.as_bytes(), method, url)
}

#[cfg(unix)]
fn http_over_unix(
  method: &str,
  url: &str,
  socket_path: &str,
  url_path: &str,
) -> Result<(), ActionError> {
  let mut stream =
    UnixStream::connect(socket_path).map_err(|e| ActionError::HttpRequest {
      method: method.to_string(),
      url: url.to_string(),
      detail: format!("connect to {socket_path} failed: {e}"),
    })?;

  let request = format!(
        "{method} {url_path} HTTP/1.0\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n"
    );
  write_and_check(&mut stream, request.as_bytes(), method, url)
}

/// Writes the request bytes, reads the response, and checks for a 2xx status.
fn write_and_check(
  stream: &mut (impl Read + Write),
  request: &[u8],
  method: &str,
  url: &str,
) -> Result<(), ActionError> {
  stream
    .write_all(request)
    .map_err(|e| ActionError::HttpRequest {
      method: method.to_string(),
      url: url.to_string(),
      detail: format!("write failed: {e}"),
    })?;

  let mut buf = [0u8; 256];
  let n = stream
    .read(&mut buf)
    .map_err(|e| ActionError::HttpRequest {
      method: method.to_string(),
      url: url.to_string(),
      detail: format!("read failed: {e}"),
    })?;
  let response = std::str::from_utf8(&buf[..n]).unwrap_or("");
  let status_line = response.lines().next().unwrap_or("");

  if status_line.starts_with("HTTP/") {
    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() >= 2 {
      let code: u16 = parts[1].parse().unwrap_or(0);
      if (200..300).contains(&code) {
        return Ok(());
      }
    }
  }

  Err(ActionError::HttpRequest {
    method: method.to_string(),
    url: url.to_string(),
    detail: format!("non-2xx response: {status_line}"),
  })
}

// ── URL parsing ────────────────────────────────────────────────────────────────

/// Parses a URL into an `HttpTarget`.  Accepts:
/// - `http://host:port/path` for TCP connections.
/// - `unix:/path/to/sock:/url-path` for Unix domain sockets.
fn parse_url(url: &str) -> Result<HttpTarget, String> {
  if let Some(rest) = url.strip_prefix("unix:") {
    parse_unix_url(rest)
  } else {
    parse_http_url(url)
  }
}

fn parse_http_url(url: &str) -> Result<HttpTarget, String> {
  let without_scheme = url.strip_prefix("http://").ok_or_else(|| {
    format!("URL must start with http:// or unix: — got {url}")
  })?;

  let (authority, path) = match without_scheme.find('/') {
    Some(idx) => (&without_scheme[..idx], without_scheme[idx..].to_string()),
    None => (without_scheme, "/".to_string()),
  };

  let (host, port) = if let Some(idx) = authority.rfind(':') {
    let port: u16 = authority[idx + 1..]
      .parse()
      .map_err(|_| format!("invalid port in URL: {url}"))?;
    (authority[..idx].to_string(), port)
  } else {
    (authority.to_string(), 80u16)
  };

  Ok(HttpTarget::Tcp { host, port, path })
}

/// Parses the portion after `unix:`, expecting `/sock/path:/url-path`.
#[cfg(unix)]
fn parse_unix_url(rest: &str) -> Result<HttpTarget, String> {
  let colon = rest.rfind(':').filter(|&i| i > 0).ok_or_else(|| {
    format!("Unix URL must be unix:/socket/path:/url-path — got unix:{rest}")
  })?;
  let socket_path = &rest[..colon];
  let url_path = &rest[colon + 1..];
  let url_path = if url_path.is_empty() { "/" } else { url_path };
  Ok(HttpTarget::Unix {
    socket_path: socket_path.to_string(),
    url_path: url_path.to_string(),
  })
}

#[cfg(not(unix))]
fn parse_unix_url(_rest: &str) -> Result<HttpTarget, String> {
  Err("Unix socket URLs are not supported on this platform".to_string())
}

// ── ExecAction ─────────────────────────────────────────────────────────────────

pub struct ExecAction {
  pub pressure_cmd: String,
  pub clear_cmd: String,
}

impl PressureAction for ExecAction {
  fn on_pressure(&self) -> Result<(), ActionError> {
    run_cmd(&self.pressure_cmd)
  }

  fn on_clear(&self) -> Result<(), ActionError> {
    run_cmd(&self.clear_cmd)
  }
}

fn run_cmd(cmd: &str) -> Result<(), ActionError> {
  let status = std::process::Command::new("sh")
    .args(["-c", cmd])
    .status()
    .map_err(|e| ActionError::Exec {
      cmd: cmd.to_string(),
      detail: format!("failed to spawn shell: {e}"),
    })?;
  if !status.success() {
    return Err(ActionError::Exec {
      cmd: cmd.to_string(),
      detail: format!("exited {status}"),
    });
  }
  Ok(())
}
