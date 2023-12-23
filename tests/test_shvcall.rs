extern crate shv;

use std::process::Command;

#[test]
fn test_wrong_params() -> shv::Result<()> {
    let exit_status = Command::new("target/debug/shvcall")
        .arg("--urll").arg("tcp://admin:admin@localhost")
        .status()?;
    if exit_status.success() {
        Err("Wrong param error should be reported".into())
    } else {
        Ok(())
    }
}
