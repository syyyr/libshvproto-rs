use std::process::{Child, Command, Output};
use shv::{RpcMessage, RpcValue};

pub struct KillProcessGuard {
    pub child: Child,
}
impl KillProcessGuard {
    pub fn new(child: Child) -> Self {
        KillProcessGuard {
            child,
        }
    }

    pub fn is_running(&mut self) -> bool {
        let status = self.child.try_wait().unwrap();
        status.is_none()
    }
}
impl Drop for KillProcessGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _exit_status= self.child.wait();
        //println!("shvbroker exit status: {:?}", exit_status);
    }
}

pub fn rpcmsg_from_output(output: Output) -> shv::Result<RpcMessage> {
    if !output.status.success() {
        let errmsg = std::str::from_utf8(&output.stderr)?;
        return Err(format!("Process exited with error code {:?}, stderr: {}", output.status.code(), errmsg).into());
    }
    let out = output.stdout;
    let cpon = std::str::from_utf8(&out)?;
    let rv = RpcValue::from_cpon(cpon)?;
    Ok(RpcMessage::from_rpcvalue(rv)?)
}
pub fn result_from_output(output: Output) -> shv::Result<RpcValue> {
    if !output.status.success() {
        let errmsg = std::str::from_utf8(&output.stderr)?;
        return Err(format!("Process exited with error code {:?}, stderr: {}", output.status.code(), errmsg).into());
    }
    let msg = rpcmsg_from_output(output)?;
    let result = msg.result()?;
    //println!("cpon: {}, expected: {}", result, expected_value.to_cpon());
    //assert_eq!(result, expected_value);
    Ok(result.clone())
}
pub fn shv_call(path: &str, method: &str, param: &str) -> shv::Result<RpcValue> {
    let output = Command::new("target/debug/shvcall")
        .arg("--url").arg("tcp://admin:admin@localhost")
        .arg("--path").arg(path)
        .arg("--method").arg(method)
        .arg("--param").arg(param)
        .output()?;

    result_from_output(output)
}