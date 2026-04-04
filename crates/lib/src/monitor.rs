use std::collections::HashSet;
use std::time::Duration;

use tracing::{debug, error, info, warn};

use crate::action::PressureAction;
use crate::config::PressureConfig;
use crate::detector::{DetectorError, Pid, PressureDetector};
use crate::discovery::{DiscoveryError, ProcessDiscovery};

#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
  #[error("Discovery failed: {0}")]
  Discovery(#[from] DiscoveryError),

  #[error("Detector failed: {0}")]
  Detector(#[from] DetectorError),
}

pub struct Monitor {
  pub detector: Box<dyn PressureDetector>,
  pub discovery: Box<dyn ProcessDiscovery>,
  pub actions: Vec<Box<dyn PressureAction>>,
  pub config: PressureConfig,
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
      "Starting pressure monitor"
    );

    loop {
      let owned_pids: HashSet<Pid> = match self.discovery.pids() {
        Ok(pids) => pids,
        Err(e) => {
          warn!(error = %e, "PID discovery failed; using empty exclusion set");
          HashSet::new()
        }
      };

      let (pressure, contributors) = match self.detector.sample(&owned_pids) {
        Ok(p) => p,
        Err(e) => {
          warn!(error = %e, "Detector sample failed; skipping tick");
          std::thread::sleep(interval);
          continue;
        }
      };

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
            "GPU pressure sustained; pausing worker"
          );
          for action in &self.actions {
            if let Err(e) = action.on_pressure() {
              error!(error = %e, "on_pressure action failed");
            }
          }
          paused = true;
        }
      } else {
        below += 1;
        above = 0;
        if below >= self.config.hysteresis && paused {
          info!(pressure, "GPU pressure cleared; resuming worker");
          for action in &self.actions {
            if let Err(e) = action.on_clear() {
              error!(error = %e, "on_clear action failed");
            }
          }
          paused = false;
        }
      }

      std::thread::sleep(interval);
    }
  }
}
