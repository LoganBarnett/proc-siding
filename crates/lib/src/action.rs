use std::io::{Read, Write};
use std::net::TcpStream;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("HTTP POST to {url} failed: {detail}")]
    HttpPost { url: String, detail: String },

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
        http_post(&self.pressure_url)
    }

    fn on_clear(&self) -> Result<(), ActionError> {
        http_post(&self.clear_url)
    }
}

/// Performs a plain HTTP/1.0 POST with an empty body.  All calls go to
/// localhost, so no TLS is needed.
fn http_post(url: &str) -> Result<(), ActionError> {
    let (host, port, path) =
        parse_http_url(url).map_err(|detail| ActionError::HttpPost {
            url: url.to_string(),
            detail,
        })?;

    let mut stream =
        TcpStream::connect(format!("{host}:{port}")).map_err(|e| ActionError::HttpPost {
            url: url.to_string(),
            detail: format!("connect failed: {e}"),
        })?;

    let request = format!(
        "POST {path} HTTP/1.0\r\nHost: {host}:{port}\r\nContent-Length: 0\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| ActionError::HttpPost {
            url: url.to_string(),
            detail: format!("write failed: {e}"),
        })?;

    // Read enough of the response to check the status line.
    let mut buf = [0u8; 256];
    let n = stream.read(&mut buf).map_err(|e| ActionError::HttpPost {
        url: url.to_string(),
        detail: format!("read failed: {e}"),
    })?;
    let response = std::str::from_utf8(&buf[..n]).unwrap_or("");
    let status_line = response.lines().next().unwrap_or("");

    // Accept any 2xx status.
    if status_line.starts_with("HTTP/") {
        let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
        if parts.len() >= 2 {
            let code: u16 = parts[1].parse().unwrap_or(0);
            if (200..300).contains(&code) {
                return Ok(());
            }
        }
    }

    Err(ActionError::HttpPost {
        url: url.to_string(),
        detail: format!("non-2xx response: {status_line}"),
    })
}

/// Parse a plain `http://host:port/path` URL into its components.
fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
    let without_scheme = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("URL must start with http://: {url}"))?;

    let (authority, path) = match without_scheme.find('/') {
        Some(idx) => (
            &without_scheme[..idx],
            without_scheme[idx..].to_string(),
        ),
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

    Ok((host, port, path))
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
