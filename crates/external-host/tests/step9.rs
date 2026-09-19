use std::env;
use std::time::Duration;

use rawweave_external_host::{
    ArgumentSource, CliArgument, CliArgumentType, CliHost, CliHostConfig, CliRequest, CliValue,
    DataPlane, HostConfig, HostError, ResourceLimits, Supervisor,
};
use rawweave_external_protocol::{DataKind, RequestPayload};

fn shell_config(script: &str) -> HostConfig {
    HostConfig::new("/bin/sh")
        .with_args(["-c", script])
        .with_limits(ResourceLimits {
            request_timeout: Duration::from_millis(100),
            ..ResourceLimits::default()
        })
}

#[test]
fn temp_buffers_validate_hash_size_and_clean_up_on_drop() {
    let plane = DataPlane::new(1024 * 1024).unwrap();
    let buffer = plane.create(DataKind::Bytes, b"hello").unwrap();
    let path = buffer.path().to_owned();
    assert_eq!(plane.read(buffer.descriptor()).unwrap(), b"hello");
    drop(buffer);
    assert!(!path.exists());
}

#[test]
fn malicious_buffer_paths_are_rejected() {
    let plane = DataPlane::new(1024).unwrap();
    let mut buffer = plane.create(DataKind::Bytes, b"x").unwrap();
    buffer.descriptor_mut().relative_path = "../../etc/passwd".into();
    assert!(plane.read(buffer.descriptor()).is_err());
}

#[test]
fn environment_is_cleared_except_for_allowlisted_names() {
    // The child is deliberately a shell because the host still passes the
    // configured command as argv; the host never constructs a shell command.
    unsafe {
        env::set_var("RAWWEAVE_EXTERNAL_SECRET", "must-not-leak");
    }
    let config = HostConfig::new("/bin/sh")
        .with_args(["-c", "test \"${RAWWEAVE_EXTERNAL_SECRET-unset}\" = unset"])
        .with_environment_allowlist([]);
    let supervisor = Supervisor::new(config).unwrap();
    let result = supervisor.request(RequestPayload::Discover);
    unsafe {
        env::remove_var("RAWWEAVE_EXTERNAL_SECRET");
    }
    assert!(result.is_ok(), "secret leaked or child failed: {result:?}");
}

#[test]
fn supervisor_returns_timeout_and_recovers_by_restarting() {
    let supervisor = Supervisor::new(shell_config("sleep 1")).unwrap();
    let first = supervisor.request(RequestPayload::Discover);
    assert!(matches!(first, Err(HostError::Timeout { .. })));
    let starts_after_timeout = supervisor.start_count();
    let second = supervisor.request(RequestPayload::Discover);
    assert!(matches!(second, Err(HostError::Timeout { .. })));
    assert!(supervisor.start_count() > starts_after_timeout);
}

#[test]
fn supervisor_reports_crashed_process_without_panicking() {
    let supervisor = Supervisor::new(shell_config("exit 23")).unwrap();
    assert!(matches!(
        supervisor.request(RequestPayload::Discover),
        Err(HostError::Crashed { .. })
    ));
}

#[test]
fn cli_host_executes_typed_arguments_and_validates_output() {
    let config = CliHostConfig::new("/bin/sh")
        .with_args(["-c", "printf '%s' \"$1\" > \"$2\"", "--"])
        .with_argument(CliArgument::new(
            "message",
            CliArgumentType::String,
            ArgumentSource::Value,
        ))
        .with_argument(CliArgument::new(
            "output",
            CliArgumentType::OutputFile,
            ArgumentSource::Output,
        ))
        .with_output("output", 128);
    let host = CliHost::new(config).unwrap();
    let request = CliRequest::default()
        .with_value("message", CliValue::String("safe; not shell code".into()))
        .with_output("output");
    let result = host.execute(request).unwrap();
    assert_eq!(result.outputs["output"].bytes, b"safe; not shell code");
}

#[test]
fn cli_host_reports_non_zero_status_and_diagnostics() {
    let config = CliHostConfig::new("/bin/sh").with_args(["-c", "printf boom >&2; exit 9"]);
    let host = CliHost::new(config).unwrap();
    let error = host.execute(CliRequest::default()).unwrap_err();
    assert!(matches!(error, HostError::CliExit { status: 9, .. }));
    assert!(error.to_string().contains("boom"));
}
