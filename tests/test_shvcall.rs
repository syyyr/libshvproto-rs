extern crate shv;

use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use shv::{RpcValue};
use crate::common::{shv_call, text_from_output};

mod common;

#[test]
fn test_invalid_param() -> shv::Result<()> {
    let exit_status = Command::new("target/debug/shvcall")
        .arg("--urll").arg("tcp://admin:admin@localhost")
        .status()?;
    assert!(!exit_status.success());
    Ok(())
}
#[test]
fn test_cannot_connect() -> shv::Result<()> {
    let exit_status = Command::new("target/debug/shvcall")
        .arg("--url").arg("tcp://admin:admin@localhost")
        .status()?;
    assert!(!exit_status.success());
    Ok(())
}
#[test]
fn test_call_ping_stdin() -> shv::Result<()> {
    let mut broker_process_guard = common::KillProcessGuard::new(Command::new("target/debug/shvbroker")
        .arg("-v").arg(".:I")
        .spawn()?);
    thread::sleep(Duration::from_millis(100));
    assert!(broker_process_guard.is_running());

    assert_eq!(shv_call(".app", "ping", "")?, RpcValue::null());

    let mut child = Command::new("target/debug/shvcall")
        .arg("--url").arg("tcp://admin:admin@localhost")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .arg("-r")
        .arg("-v").arg(".:I")
        .spawn()?;
    let mut stdin = child.stdin.take().expect("shvcall should be running");
    thread::spawn(move || {
        stdin.write_all(".app:ping\n".as_bytes()).expect("Failed to write to stdin");
        stdin.write_all(".app:name\n".as_bytes()).expect("Failed to write to stdin");
    });
    let output = child.wait_with_output()?;
    let out = text_from_output(output)?;
    let expected: [&str; 2] = ["null", "\"shvbroker\""];
    for (no, line) in out.split(|b| b == '\n').enumerate() {
        if no < expected.len() {
            assert_eq!(expected[no], line);
        }
    }

    Ok(())
}
