use clap::Parser;
use proc_siding_lib::config::AppConfig;
use proc_siding_lib::{LogFormat, LogLevel};
use serde::Deserialize;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("Failed to read config file at {path:?}: {source}")]
  FileRead {
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to parse config file at {path:?}: {source}")]
  Parse {
    path: PathBuf,
    #[source]
    source: toml::de::Error,
  },

  #[error("Configuration validation failed: {0}")]
  Validation(String),
}

#[derive(Debug, Parser)]
#[command(
  name = "proc-siding",
  about = "Pressure monitor for safe-idle-worker governance"
)]
pub struct CliRaw {
  /// Log level (trace, debug, info, warn, error).
  #[arg(long, env = "LOG_LEVEL")]
  pub log_level: Option<String>,

  /// Log format (text, json).
  #[arg(long, env = "LOG_FORMAT")]
  pub log_format: Option<String>,

  /// Path to the TOML configuration file.
  #[arg(short, long, env = "PROC_SIDING_CONFIG")]
  pub config: Option<PathBuf>,
}

/// Raw deserialization target for the config file.  Fields not part of
/// AppConfig (e.g. log_level) are extracted first; the rest is flattened into
/// AppConfig.
#[derive(Debug, Deserialize)]
struct ConfigFileRaw {
  #[serde(default)]
  log_level: Option<String>,
  #[serde(default)]
  log_format: Option<String>,
  #[serde(flatten)]
  app: AppConfig,
}

pub struct Config {
  pub log_level: LogLevel,
  pub log_format: LogFormat,
  pub app: AppConfig,
}

impl Config {
  pub fn from_cli_and_file(cli: CliRaw) -> Result<Self, ConfigError> {
    let config_path = cli
      .config
      .unwrap_or_else(|| PathBuf::from("/etc/proc-siding.toml"));

    let contents = std::fs::read_to_string(&config_path).map_err(|source| {
      ConfigError::FileRead {
        path: config_path.clone(),
        source,
      }
    })?;

    let raw: ConfigFileRaw =
      toml::from_str(&contents).map_err(|source| ConfigError::Parse {
        path: config_path,
        source,
      })?;

    let log_level = raw
      .log_level
      .as_deref()
      .unwrap_or("info")
      .parse::<LogLevel>()
      .map_err(|e| ConfigError::Validation(e.to_string()))?;

    let log_format = raw
      .log_format
      .as_deref()
      .unwrap_or("text")
      .parse::<LogFormat>()
      .map_err(|e| ConfigError::Validation(e.to_string()))?;

    Ok(Config {
      log_level,
      log_format,
      app: raw.app,
    })
  }
}
