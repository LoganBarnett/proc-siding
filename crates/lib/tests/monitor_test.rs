use proc_siding_lib::monitor::{run_detector, SampleError};

#[test]
fn parses_single_tsv_line() {
  let sample = run_detector("printf '42.5\\tfirefox\\n'")
    .expect("Should parse single TSV line");

  assert!((sample.pressure - 42.5).abs() < f64::EPSILON);
  assert_eq!(sample.contributors, vec!["firefox"]);
}

#[test]
fn sums_multiple_contributors() {
  let cmd = "printf '10.0\\tfirefox\\n25.5\\tblender\\n'";
  let sample = run_detector(cmd).expect("Should parse multiple TSV lines");

  assert!((sample.pressure - 35.5).abs() < f64::EPSILON);
  assert_eq!(sample.contributors, vec!["firefox", "blender"]);
}

#[test]
fn ignores_blank_lines() {
  let cmd = "printf '\\n10.0\\tfirefox\\n\\n20.0\\tblender\\n\\n'";
  let sample = run_detector(cmd).expect("Should skip blank lines");

  assert!((sample.pressure - 30.0).abs() < f64::EPSILON);
  assert_eq!(sample.contributors, vec!["firefox", "blender"]);
}

#[test]
fn empty_output_yields_zero_pressure() {
  let sample = run_detector("true").expect("Should handle empty output");

  assert!((sample.pressure - 0.0).abs() < f64::EPSILON);
  assert!(sample.contributors.is_empty());
}

#[test]
fn trims_whitespace_from_values_and_entities() {
  let cmd = "printf '  42.5  \\t  firefox  \\n'";
  let sample = run_detector(cmd).expect("Should trim whitespace");

  assert!((sample.pressure - 42.5).abs() < f64::EPSILON);
  assert_eq!(sample.contributors, vec!["firefox"]);
}

#[test]
fn rejects_missing_tab_separator() {
  let result = run_detector("echo 'no-tab-here'");

  assert!(result.is_err());
  let err = result.unwrap_err();
  assert!(
    matches!(err, SampleError::ParseFailed { .. }),
    "Expected ParseFailed, got: {err}"
  );
}

#[test]
fn rejects_non_numeric_value() {
  let result = run_detector("printf 'abc\\tfirefox\\n'");

  assert!(result.is_err());
  let err = result.unwrap_err();
  assert!(
    matches!(err, SampleError::ParseFailed { .. }),
    "Expected ParseFailed, got: {err}"
  );
}

#[test]
fn reports_command_failure() {
  let result = run_detector("exit 1");

  assert!(result.is_err());
  let err = result.unwrap_err();
  assert!(
    matches!(err, SampleError::CommandFailed(..)),
    "Expected CommandFailed, got: {err}"
  );
}

#[test]
fn captures_stderr_on_failure() {
  let result = run_detector("echo 'something broke' >&2; exit 1");

  let err = result.unwrap_err();
  let msg = format!("{err}");
  assert!(
    msg.contains("something broke"),
    "Expected stderr in error message, got: {msg}"
  );
}

#[test]
fn handles_decimal_precision() {
  let cmd =
    "printf '33.33\\tprocess-a\\n33.33\\tprocess-b\\n33.34\\tprocess-c\\n'";
  let sample = run_detector(cmd).expect("Should handle decimals");

  assert!((sample.pressure - 100.0).abs() < 0.01);
  assert_eq!(sample.contributors.len(), 3);
}
