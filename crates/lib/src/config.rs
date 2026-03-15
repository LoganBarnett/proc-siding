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
pub enum DetectorConfig {
    Amd,
    Nvidia,
    Metal,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProcessDiscoveryConfig {
    SystemdUnit { unit: String },
    Pid { pid: u32 },
    ProcessName { pattern: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionConfig {
    HttpPost {
        pressure_url: String,
        clear_url: String,
    },
    Exec {
        pressure_cmd: String,
        clear_cmd: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub pressure: PressureConfig,
    pub detector: DetectorConfig,
    pub process_discovery: ProcessDiscoveryConfig,
    pub action: ActionConfig,
}
