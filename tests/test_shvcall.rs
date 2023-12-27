extern crate shv;

use std::process::{Command};
use std::thread;
use std::time::Duration;
use shv::{RpcValue};
use crate::common::{shv_call, shv_call_many, ShvCallOutputFormat};

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
        .arg("--url").arg("tcp://admin:admin@nnn")
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

    println!("---shvcall---: .app/ping()");
    assert_eq!(shv_call(".app", "ping", "")?, RpcValue::null());

    println!("---shvcall---: .app/name()");
    let calls: Vec<String> = vec![
        ".app:ping".into(),
        ".app:name".into(),
    ];
    let values = shv_call_many(calls, ShvCallOutputFormat::Value)?;
    let expected = vec!["null", r#""shvbroker""#];
    for (no, val) in values.iter().enumerate() {
        assert_eq!(&expected[no], val);
    }

    Ok(())
}
