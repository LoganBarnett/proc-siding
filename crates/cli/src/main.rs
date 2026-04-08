//! proc-siding — pressure monitor for safe-idle-worker governance.
//!
//! Reads a TOML config file, constructs the action list, and runs the
//! hysteresis state machine in a blocking loop.  All business logic lives
//! in proc-siding-lib; this file only wires components together.
//!
//! # LLM Development Guidelines
//! - Keep wiring logic here; keep business logic in proc-siding-lib.
//! - Use semantic error types — no anyhow wrapping.

mod config;
mod logging;

use clap::Parser;
use config::{CliRaw, Config, ConfigError};
use logging::init_logging;
use proc_siding_lib::{
  action::{ExecAction, HttpAction, HttpPostAction, PressureAction},
  config::{ActionConfig, AppConfig},
  metrics::Metrics,
  monitor::Monitor,
};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
enum ApplicationError {
  #[error("Failed to load configuration: {0}")]
  Config(#[from] ConfigError),
}

fn main() -> Result<(), ApplicationError> {
  let cli = CliRaw::parse();
  let config = Config::from_cli_and_file(cli).map_err(|e| {
    eprintln!("Configuration error: {e}");
    ApplicationError::Config(e)
  })?;
  init_logging(config.log_level, config.log_format);
  run(config.app)
}

fn action_from_config(config: ActionConfig) -> Box<dyn PressureAction> {
  match config {
    ActionConfig::HttpPost {
      pressure_url,
      clear_url,
    } => Box::new(HttpPostAction {
      pressure_url,
      clear_url,
    }),
    ActionConfig::Http {
      pressure_url,
      pressure_method,
      clear_url,
      clear_method,
    } => Box::new(HttpAction {
      pressure_url,
      pressure_method,
      clear_url,
      clear_method,
    }),
    ActionConfig::Exec {
      pressure_cmd,
      clear_cmd,
    } => Box::new(ExecAction {
      pressure_cmd,
      clear_cmd,
    }),
  }
}

fn run(app: AppConfig) -> Result<(), ApplicationError> {
  let mut actions: Vec<Box<dyn PressureAction>> =
    vec![action_from_config(app.action)];
  actions.extend(app.extra_actions.into_iter().map(action_from_config));

  let metrics = app.metrics_listen.as_ref().map(|listen| {
    let m = Arc::new(Metrics::new());
    proc_siding_lib::metrics_server::spawn(listen, Arc::clone(&m))
      .expect("Failed to start metrics server");
    m
  });

  Monitor {
    detector_cmd: app.detector_cmd,
    actions,
    config: app.pressure,
    metrics,
  }
  .run();

  Ok(())
}
