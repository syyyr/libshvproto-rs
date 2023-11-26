extern crate shv;

use std::process::{Child, Command};
use std::{thread, time::Duration};
use shv::{RpcMessage, RpcValue};

struct KillProcessGuard {
    child: Child,
}
impl Drop for KillProcessGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let exit_status= self.child.wait();
        println!("shvbroker exit status: {:?}", exit_status);
    }
}

#[test]
fn it_works() -> Result<(), Box<dyn std::error::Error>> {
    let _broker_process_guard = KillProcessGuard { child: Command::new("target/debug/shvbroker").spawn()? };
    thread::sleep(Duration::from_millis(500));

    fn call(path: &str, method: &str, param: &str, expected_value: &RpcValue) -> Result<(), Box<dyn std::error::Error>> {
        let output = Command::new("target/debug/shvcall")
            .arg("--url").arg("tcp://admin:admin@localhost")
            .arg("--path").arg(path)
            .arg("--method").arg(method)
            .arg("--param").arg(param)
            .output()?;

        let out = output.stdout;
        if !output.status.success() {
            let errmsg = std::str::from_utf8(&output.stderr)?;
            return Err(format!("Process exited with error code {:?}, stderr: {}", output.status.code(), errmsg).into());
        }
        let cpon = std::str::from_utf8(&out)?;
        let rv = RpcValue::from_cpon(cpon)?;
        let msg = RpcMessage::from_rpcvalue(rv)?;
        let result = msg.result().ok_or("no result")?;
        println!("cpon: {}, expected: {}", result, expected_value.to_cpon());
        assert_eq!(result, expected_value);
        Ok(())
    }

    call("", "ls", "", &(vec![RpcValue::from(".app")].into()))?;
    call("", "ls", r#"".app""#, &RpcValue::from(true))?;

    Ok(())
}
