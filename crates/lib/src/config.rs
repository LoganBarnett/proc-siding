use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PressureConfig {
  #[serde(default = "default_threshold")]
  pub threshold: f64,
  #[serde(default = "default_hysteresis")]
  pub hysteresis: u32,
  #[serde(default = "default_poll_interval_ms")]
  pub poll_interval_ms: u64,
}

fn default_threshold() -> f64 {
  25.0
}

fn default_hysteresis() -> u32 {
  3
}

fn default_poll_interval_ms() -> u64 {
  2000
}

impl Default for PressureConfig {
  fn default() -> Self {
    Self {
      threshold: default_threshold(),
      hysteresis: default_hysteresis(),
      poll_interval_ms: default_poll_interval_ms(),
    }
  }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionConfig {
  HttpPost {
    pressure_url: String,
    clear_url: String,
  },
  Http {
    pressure_url: String,
    #[serde(default = "default_http_method")]
    pressure_method: String,
    clear_url: String,
    #[serde(default = "default_http_method")]
    clear_method: String,
  },
  Exec {
    pressure_cmd: String,
    clear_cmd: String,
  },
}

fn default_http_method() -> String {
  "POST".to_string()
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
  #[serde(default)]
  pub pressure: PressureConfig,
  /// Command to run as the resource poller.  Must output TSV lines of
  /// `<value>\t<entity>` on stdout.
  pub detector_cmd: String,
  pub action: ActionConfig,
  #[serde(default)]
  pub extra_actions: Vec<ActionConfig>,
  #[serde(default)]
  pub metrics_listen: Option<String>,
}
