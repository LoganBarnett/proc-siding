use std::time::Duration;

use tracing::{debug, error, info, warn};

use crate::action::PressureAction;
use crate::config::PressureConfig;
use crate::metrics::SharedMetrics;

#[derive(Debug, thiserror::Error)]
pub enum SampleError {
  #[error("Detector command failed: {0}")]
  CommandFailed(String),

  #[error("Failed to parse detector output line {line:?}: {detail}")]
  ParseFailed { line: String, detail: String },
}

/// A single pressure sample parsed from the detector command's output.
#[derive(Debug)]
pub struct Sample {
  pub pressure: f64,
  pub contributors: Vec<String>,
}

/// Runs the detector command and parses its TSV output into a pressure
/// reading and contributor list.
pub fn run_detector(cmd: &str) -> Result<Sample, SampleError> {
  let output = std::process::Command::new("sh")
    .args(["-c", cmd])
    .output()
    .map_err(|e| SampleError::CommandFailed(format!("failed to spawn: {e}")))?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(SampleError::CommandFailed(format!(
      "exited {}: {}",
      output.status,
      stderr.trim()
    )));
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let mut pressure = 0.0;
  let mut contributors = Vec::new();

  for line in stdout.lines() {
    let line = line.trim();
    if line.is_empty() {
      continue;
    }
    let (value_str, entity) =
      line
        .split_once('\t')
        .ok_or_else(|| SampleError::ParseFailed {
          line: line.to_string(),
          detail: "expected tab-separated <value>\\t<entity>".to_string(),
        })?;
    let value: f64 =
      value_str
        .trim()
        .parse()
        .map_err(|_| SampleError::ParseFailed {
          line: line.to_string(),
          detail: format!("{value_str:?} is not a valid number"),
        })?;
    pressure += value;
    contributors.push(entity.trim().to_string());
  }

  Ok(Sample {
    pressure,
    contributors,
  })
}

pub struct Monitor {
  pub detector_cmd: String,
  pub actions: Vec<Box<dyn PressureAction>>,
  pub config: PressureConfig,
  pub metrics: Option<SharedMetrics>,
}

impl Monitor {
  /// Run the pressure monitor loop indefinitely.
  ///
  /// The hysteresis counters prevent flapping: `hysteresis` consecutive
  /// above-threshold samples trigger `on_pressure`, and `hysteresis`
  /// consecutive below-threshold samples trigger `on_clear`.
  pub fn run(&self) {
    let mut above: u32 = 0;
    let mut below: u32 = 0;
    let mut paused = false;
    let interval = Duration::from_millis(self.config.poll_interval_ms);

    info!(
      threshold = self.config.threshold,
      hysteresis = self.config.hysteresis,
      poll_interval_ms = self.config.poll_interval_ms,
      detector_cmd = %self.detector_cmd,
      "Starting pressure monitor"
    );

    loop {
      let Sample {
        pressure,
        contributors,
      } = match run_detector(&self.detector_cmd) {
        Ok(s) => s,
        Err(e) => {
          warn!(error = %e, "Detector sample failed; skipping tick");
          std::thread::sleep(interval);
          continue;
        }
      };

      if let Some(m) = &self.metrics {
        m.pressure_sample.set(pressure);
      }

      debug!(
        pressure,
        paused,
        above,
        below,
        contributors = contributors.join(", "),
        "Pressure tick"
      );

      if pressure > self.config.threshold {
        above += 1;
        below = 0;
        if above >= self.config.hysteresis && !paused {
          info!(
            pressure,
            contributors = contributors.join(", "),
            "Pressure sustained; triggering actions"
          );
          for action in &self.actions {
            if let Err(e) = action.on_pressure() {
              error!(error = %e, "on_pressure action failed");
              if let Some(m) = &self.metrics {
                m.action_errors.inc();
              }
            }
          }
          if let Some(m) = &self.metrics {
            m.pressure_transitions.inc();
          }
          paused = true;
        }
      } else {
        below += 1;
        above = 0;
        if below >= self.config.hysteresis && paused {
          info!(pressure, "Pressure cleared; triggering actions");
          for action in &self.actions {
            if let Err(e) = action.on_clear() {
              error!(error = %e, "on_clear action failed");
              if let Some(m) = &self.metrics {
                m.action_errors.inc();
              }
            }
          }
          if let Some(m) = &self.metrics {
            m.clear_transitions.inc();
          }
          paused = false;
        }
      }

      std::thread::sleep(interval);
    }
  }
}
