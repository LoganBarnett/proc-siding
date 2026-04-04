use std::collections::HashSet;
use thiserror::Error;

use crate::detector::Pid;

#[derive(Debug, Error)]
pub enum DiscoveryError {
  #[error("Failed to read cgroup procs for {unit} at {path}: {source}")]
  CgroupRead {
    unit: String,
    path: String,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to run pgrep: {0}")]
  Pgrep(String),

  #[error("Process {0} not found")]
  PidNotFound(Pid),
}

/// Discovers the set of PIDs belonging to the watched process tree.
pub trait ProcessDiscovery: Send + Sync {
  fn pids(&self) -> Result<HashSet<Pid>, DiscoveryError>;
}

// ── SystemdUnitDiscovery ───────────────────────────────────────────────────────

pub struct SystemdUnitDiscovery {
  pub unit: String,
}

impl ProcessDiscovery for SystemdUnitDiscovery {
  fn pids(&self) -> Result<HashSet<Pid>, DiscoveryError> {
    // systemd exposes all PIDs in a service's cgroup (including child
    // processes) in this file; no recursive walk needed.
    let path =
      format!("/sys/fs/cgroup/system.slice/{}/cgroup.procs", self.unit);
    let content = std::fs::read_to_string(&path).map_err(|source| {
      DiscoveryError::CgroupRead {
        unit: self.unit.clone(),
        path: path.clone(),
        source,
      }
    })?;
    Ok(
      content
        .lines()
        .filter_map(|line| line.trim().parse::<Pid>().ok())
        .collect(),
    )
  }
}

// ── PidDiscovery ───────────────────────────────────────────────────────────────

pub struct PidDiscovery {
  pub pid: Pid,
}

impl ProcessDiscovery for PidDiscovery {
  fn pids(&self) -> Result<HashSet<Pid>, DiscoveryError> {
    if !process_exists(self.pid) {
      return Err(DiscoveryError::PidNotFound(self.pid));
    }
    let mut result = HashSet::new();
    result.insert(self.pid);
    collect_children(self.pid, &mut result);
    Ok(result)
  }
}

// ── ProcessNameDiscovery ───────────────────────────────────────────────────────

pub struct ProcessNameDiscovery {
  pub pattern: String,
}

impl ProcessDiscovery for ProcessNameDiscovery {
  fn pids(&self) -> Result<HashSet<Pid>, DiscoveryError> {
    let output = std::process::Command::new("pgrep")
      .args(["-f", &self.pattern])
      .output()
      .map_err(|e| {
        DiscoveryError::Pgrep(format!("failed to spawn pgrep: {e}"))
      })?;

    // pgrep exits 1 when no processes match; treat as an empty set.
    if !output.status.success() && output.status.code() != Some(1) {
      let stderr = String::from_utf8_lossy(&output.stderr);
      return Err(DiscoveryError::Pgrep(format!(
        "pgrep exited {}: {stderr}",
        output.status
      )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result: HashSet<Pid> = stdout
      .lines()
      .filter_map(|line| line.trim().parse::<Pid>().ok())
      .collect();

    // Collect children for each matched root PID.
    let roots: Vec<Pid> = result.iter().copied().collect();
    for pid in roots {
      collect_children(pid, &mut result);
    }

    Ok(result)
  }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn process_exists(pid: Pid) -> bool {
  #[cfg(target_os = "linux")]
  {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
  }
  #[cfg(target_os = "macos")]
  {
    // kill -0 checks for process existence without sending a real signal.
    std::process::Command::new("kill")
      .args(["-0", &pid.to_string()])
      .status()
      .map(|s| s.success())
      .unwrap_or(false)
  }
  #[cfg(not(any(target_os = "linux", target_os = "macos")))]
  {
    let _ = pid;
    false
  }
}

/// Recursively collect all child PIDs of `pid` into `pids`.
fn collect_children(pid: Pid, pids: &mut HashSet<Pid>) {
  #[cfg(target_os = "linux")]
  {
    // Linux 3.5+ exposes direct children under this path.
    let path = format!("/proc/{pid}/task/{pid}/children");
    let Ok(content) = std::fs::read_to_string(&path) else {
      return;
    };
    for token in content.split_whitespace() {
      let Ok(child) = token.parse::<Pid>() else {
        continue;
      };
      if pids.insert(child) {
        collect_children(child, pids);
      }
    }
  }
  #[cfg(target_os = "macos")]
  {
    let Ok(output) = std::process::Command::new("pgrep")
      .args(["-P", &pid.to_string()])
      .output()
    else {
      return;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
      let Ok(child) = line.trim().parse::<Pid>() else {
        continue;
      };
      if pids.insert(child) {
        collect_children(child, pids);
      }
    }
  }
  // On other platforms we don't recurse; the PID set stays as provided.
  #[cfg(not(any(target_os = "linux", target_os = "macos")))]
  {
    let _ = (pid, pids);
  }
}
