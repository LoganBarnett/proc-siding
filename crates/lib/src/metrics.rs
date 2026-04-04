use std::sync::Arc;

use prometheus::{Gauge, IntCounter, Registry, TextEncoder};

/// Prometheus metrics for the pressure monitor.
pub struct Metrics {
  registry: Registry,
  pub pressure_transitions: IntCounter,
  pub clear_transitions: IntCounter,
  pub action_errors: IntCounter,
  pub gpu_pressure: Gauge,
}

impl Metrics {
  pub fn new() -> Self {
    let registry = Registry::new();

    let pressure_transitions = IntCounter::new(
      "proc_siding_pressure_transitions_total",
      "Times pressure was detected and actions fired",
    )
    .expect("Failed to create pressure_transitions counter");

    let clear_transitions = IntCounter::new(
      "proc_siding_clear_transitions_total",
      "Times pressure cleared and actions fired",
    )
    .expect("Failed to create clear_transitions counter");

    let action_errors = IntCounter::new(
      "proc_siding_action_errors_total",
      "Action failures during pressure or clear transitions",
    )
    .expect("Failed to create action_errors counter");

    let gpu_pressure = Gauge::new(
      "proc_siding_gpu_pressure_ratio",
      "Most recent sampled GPU pressure value",
    )
    .expect("Failed to create gpu_pressure gauge");

    registry
      .register(Box::new(pressure_transitions.clone()))
      .expect("Failed to register pressure_transitions");
    registry
      .register(Box::new(clear_transitions.clone()))
      .expect("Failed to register clear_transitions");
    registry
      .register(Box::new(action_errors.clone()))
      .expect("Failed to register action_errors");
    registry
      .register(Box::new(gpu_pressure.clone()))
      .expect("Failed to register gpu_pressure");

    Self {
      registry,
      pressure_transitions,
      clear_transitions,
      action_errors,
      gpu_pressure,
    }
  }

  /// Encode all registered metrics in Prometheus text exposition format.
  pub fn encode(&self) -> String {
    let encoder = TextEncoder::new();
    let families = self.registry.gather();
    let mut buf = String::new();
    encoder
      .encode_utf8(&families, &mut buf)
      .expect("Failed to encode metrics");
    buf
  }
}

/// Shared handle to metrics, cheaply cloneable across threads.
pub type SharedMetrics = Arc<Metrics>;
