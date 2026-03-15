use std::{path::PathBuf, process::Command};

fn binary_path() -> PathBuf {
  let mut path = std::env::current_exe().expect("current exe path");
  path.pop(); // deps/
  path.pop(); // debug/ or release/
  path.push("proc-siding");

  // Fallback: if tests run from a different layout, try sibling directory.
  if !path.exists() {
    path.pop();
    path.push("debug");
    path.push("proc-siding");
  }

  path
}

fn run(args: &[&str]) -> std::process::Output {
  Command::new(binary_path())
    .args(args)
    .output()
    .expect("failed to spawn proc-siding binary")
}

#[test]
fn help_flag_succeeds() {
  let out = run(&["--help"]);
  assert!(out.status.success(), "status: {:?}", out.status.code());
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert!(stdout.contains("--config"), "expected --config in help: {stdout}");
}

#[test]
fn missing_config_flag_exits_nonzero() {
  // The binary requires --config; omitting it must be an error.
  let out = run(&[]);
  assert!(!out.status.success(), "expected nonzero exit without --config");
}

#[test]
fn nonexistent_config_file_exits_nonzero() {
  let out = run(&["--config", "/nonexistent/path/proc-siding.toml"]);
  assert!(!out.status.success(), "expected nonzero exit for missing config");
}
