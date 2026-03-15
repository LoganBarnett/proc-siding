//! proc-siding — GPU pressure monitor for garage-queue workers.
//!
//! Reads a TOML config file, constructs the detector/discovery/action triple,
//! and runs the hysteresis state machine in a blocking loop.  All business
//! logic lives in proc-siding-lib; this file only wires components together.
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
    action::{ExecAction, HttpPostAction, PressureAction},
    config::{ActionConfig, AppConfig, DetectorConfig, ProcessDiscoveryConfig},
    detector::{AmdGpuDetector, MetalGpuDetector, NvidiaGpuDetector, PressureDetector},
    discovery::{
        PidDiscovery, ProcessDiscovery, ProcessNameDiscovery, SystemdUnitDiscovery,
    },
    monitor::Monitor,
};
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

fn run(app: AppConfig) -> Result<(), ApplicationError> {
    let detector: Box<dyn PressureDetector> = match app.detector {
        DetectorConfig::Amd => Box::new(AmdGpuDetector),
        DetectorConfig::Nvidia => Box::new(NvidiaGpuDetector),
        DetectorConfig::Metal => Box::new(MetalGpuDetector::default()),
    };

    let discovery: Box<dyn ProcessDiscovery> = match app.process_discovery {
        ProcessDiscoveryConfig::SystemdUnit { unit } => {
            Box::new(SystemdUnitDiscovery { unit })
        }
        ProcessDiscoveryConfig::Pid { pid } => Box::new(PidDiscovery { pid }),
        ProcessDiscoveryConfig::ProcessName { pattern } => {
            Box::new(ProcessNameDiscovery { pattern })
        }
    };

    let action: Box<dyn PressureAction> = match app.action {
        ActionConfig::HttpPost {
            pressure_url,
            clear_url,
        } => Box::new(HttpPostAction {
            pressure_url,
            clear_url,
        }),
        ActionConfig::Exec {
            pressure_cmd,
            clear_cmd,
        } => Box::new(ExecAction {
            pressure_cmd,
            clear_cmd,
        }),
    };

    Monitor {
        detector,
        discovery,
        action,
        config: app.pressure,
    }
    .run();

    Ok(())
}
