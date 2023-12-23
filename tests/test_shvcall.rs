extern crate shv;

use std::process::{Command};
use std::thread;
use std::time::Duration;
use shv::{RpcValue};
use crate::common::shv_call;

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
fn test_call_ping() -> shv::Result<()> {
    let mut broker_process_guard = common::KillProcessGuard::new(Command::new("target/debug/shvbroker")
        .arg("-v").arg(".:W")
        //.arg("-v").arg("Acl")
        .spawn()?);
    thread::sleep(Duration::from_millis(100));
    assert!(broker_process_guard.is_running());

    assert_eq!(shv_call(".app", "ping", "")?, RpcValue::null());

    Ok(())
}
