use std::collections::HashSet;
use thiserror::Error;

pub type Pid = u32;

#[derive(Debug, Error)]
pub enum DetectorError {
    #[error("Failed to read sysfs at {path}: {source}")]
    SysfsRead {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse GPU utilization from {path}: {value:?}")]
    ParseError { path: String, value: String },

    #[error("nvidia-smi query failed: {0}")]
    NvidiaSmi(String),

    #[error("metalps subprocess failed: {0}")]
    MetalSubprocess(String),

    #[error("Failed to parse metalps JSON output: {0}")]
    MetalJsonParse(String),
}

/// Measures external GPU pressure as a 0.0–100.0 value.
/// The `excluded` set contains PIDs owned by the watched process tree; these
/// are subtracted from the measurement so that the owned workload does not
/// incorrectly look like external pressure.
pub trait PressureDetector: Send + Sync {
    fn sample(&self, excluded: &HashSet<Pid>) -> Result<f64, DetectorError>;
}

// ── Linux helpers ─────────────────────────────────────────────────────────────

/// Returns true if the process has any open file descriptor whose symlink
/// target contains `pattern` (e.g. "/dev/dri/" or "/dev/nvidia").
#[cfg(target_os = "linux")]
fn pid_has_device_fd(pid: Pid, pattern: &str) -> bool {
    let fd_dir = format!("/proc/{pid}/fd");
    let Ok(entries) = std::fs::read_dir(&fd_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        if let Ok(target) = std::fs::read_link(entry.path()) {
            if target.to_string_lossy().contains(pattern) {
                return true;
            }
        }
    }
    false
}

/// Returns true if any PID not in `excluded` currently has the device open.
/// Requires root or CAP_SYS_PTRACE to read other processes' fd directories.
#[cfg(target_os = "linux")]
fn any_external_pid_has_device(excluded: &HashSet<Pid>, device_pattern: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let Ok(pid) = name_str.parse::<Pid>() else {
            continue;
        };
        if excluded.contains(&pid) {
            continue;
        }
        if pid_has_device_fd(pid, device_pattern) {
            return true;
        }
    }
    false
}

// ── AMD GPU detector ──────────────────────────────────────────────────────────

pub struct AmdGpuDetector;

#[cfg(target_os = "linux")]
impl AmdGpuDetector {
    /// Read the first card's gpu_busy_percent from sysfs.
    fn read_busy_percent() -> Result<f64, DetectorError> {
        let drm_dir = std::path::Path::new("/sys/class/drm");
        let entries = std::fs::read_dir(drm_dir).map_err(|source| {
            DetectorError::SysfsRead {
                path: "/sys/class/drm".to_string(),
                source,
            }
        })?;

        for entry in entries.flatten() {
            let name = entry.file_name();
            // Only consider card* entries; renderD* entries are for render nodes.
            if !name.to_string_lossy().starts_with("card") {
                continue;
            }
            let busy_path = entry.path().join("device/gpu_busy_percent");
            if !busy_path.exists() {
                continue;
            }
            let content =
                std::fs::read_to_string(&busy_path).map_err(|source| {
                    DetectorError::SysfsRead {
                        path: busy_path.display().to_string(),
                        source,
                    }
                })?;
            let trimmed = content.trim();
            return trimmed.parse::<f64>().map_err(|_| DetectorError::ParseError {
                path: busy_path.display().to_string(),
                value: trimmed.to_string(),
            });
        }

        // No AMD GPU found; treat as idle.
        Ok(0.0)
    }
}

#[cfg(target_os = "linux")]
impl PressureDetector for AmdGpuDetector {
    fn sample(&self, excluded: &HashSet<Pid>) -> Result<f64, DetectorError> {
        // If no external process has a DRM fd open, there is no external
        // pressure — skip the sysfs read entirely.
        if !any_external_pid_has_device(excluded, "/dev/dri/") {
            return Ok(0.0);
        }
        Self::read_busy_percent()
    }
}

#[cfg(not(target_os = "linux"))]
impl PressureDetector for AmdGpuDetector {
    fn sample(&self, _excluded: &HashSet<Pid>) -> Result<f64, DetectorError> {
        Err(DetectorError::SysfsRead {
            path: String::new(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "AMD GPU detector is only supported on Linux",
            ),
        })
    }
}

// ── NVIDIA GPU detector ───────────────────────────────────────────────────────

pub struct NvidiaGpuDetector;

#[cfg(target_os = "linux")]
impl PressureDetector for NvidiaGpuDetector {
    fn sample(&self, excluded: &HashSet<Pid>) -> Result<f64, DetectorError> {
        if !any_external_pid_has_device(excluded, "/dev/nvidia") {
            return Ok(0.0);
        }
        // Query overall utilization from nvidia-smi.  Per-process utilization
        // would require parsing a different query; total is a good enough proxy
        // because a gaming workload will saturate the GPU.
        let output = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=utilization.gpu",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .map_err(|e| {
                DetectorError::NvidiaSmi(format!("failed to spawn nvidia-smi: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DetectorError::NvidiaSmi(format!(
                "nvidia-smi exited {}: {stderr}",
                output.status
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().next().unwrap_or("").trim();
        line.parse::<f64>().map_err(|_| DetectorError::ParseError {
            path: "nvidia-smi output".to_string(),
            value: line.to_string(),
        })
    }
}

#[cfg(not(target_os = "linux"))]
impl PressureDetector for NvidiaGpuDetector {
    fn sample(&self, _excluded: &HashSet<Pid>) -> Result<f64, DetectorError> {
        Err(DetectorError::NvidiaSmi(
            "NVIDIA GPU detector is only supported on Linux".to_string(),
        ))
    }
}

// ── Metal GPU detector (macOS) ────────────────────────────────────────────────

/// Minimal serde types for parsing `metalps --json` output.  These mirror the
/// serialised form of `metalps_lib::types::GpuProcessInfo` / `GpuOutput`
/// without creating a Cargo dependency on that crate.
#[derive(serde::Deserialize)]
struct MetalpsProcess {
    pid: i32,
    gpu_percent: f64,
}

#[derive(serde::Deserialize)]
struct MetalpsOutput {
    processes: Vec<MetalpsProcess>,
}

pub struct MetalGpuDetector {
    /// Sample interval forwarded to `metalps --interval-ms`.
    pub sample_interval_ms: u64,
}

impl Default for MetalGpuDetector {
    fn default() -> Self {
        Self {
            sample_interval_ms: 500,
        }
    }
}

impl PressureDetector for MetalGpuDetector {
    fn sample(&self, excluded: &HashSet<Pid>) -> Result<f64, DetectorError> {
        let output = std::process::Command::new("metalps")
            .args([
                "--json",
                "--interval-ms",
                &self.sample_interval_ms.to_string(),
            ])
            .output()
            .map_err(|e| {
                DetectorError::MetalSubprocess(format!("failed to spawn metalps: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DetectorError::MetalSubprocess(format!(
                "metalps exited {}: {stderr}",
                output.status
            )));
        }

        let parsed: MetalpsOutput =
            serde_json::from_slice(&output.stdout).map_err(|e| {
                DetectorError::MetalJsonParse(e.to_string())
            })?;

        let total: f64 = parsed
            .processes
            .iter()
            .filter(|p| !excluded.contains(&(p.pid as u32)))
            .map(|p| p.gpu_percent)
            .sum();

        Ok(total.min(100.0))
    }
}
