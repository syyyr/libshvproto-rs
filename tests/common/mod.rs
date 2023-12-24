use std::io::Write;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
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
    let rv = rpcvalue_from_output(output)?;
    Ok(RpcMessage::from_rpcvalue(rv)?)
}
pub fn rpcvalue_from_output(output: Output) -> shv::Result<RpcValue> {
    let out = bytes_from_output(output)?;
    let cpon = std::str::from_utf8(&out)?;
    Ok(RpcValue::from_cpon(cpon)?)
}
pub fn bytes_from_output(output: Output) -> shv::Result<Vec<u8>> {
    if !output.status.success() {
        let errmsg = std::str::from_utf8(&output.stderr)?;
        return Err(format!("Process exited with error code {:?}, stderr: {}", output.status.code(), errmsg).into());
    }
    Ok(output.stdout)
}
pub fn text_from_output(output: Output) -> shv::Result<String> {
    let bytes = bytes_from_output(output)?;
    Ok(String::from_utf8(bytes)?)
}
pub fn value_list_from_output(output: Output) -> shv::Result<Vec<RpcValue>> {
    let bytes = text_from_output(output)?;
    let mut values = Vec::new();
    for cpon in bytes.split(|b| b == '\n').filter(|line| !line.is_empty()) {
        values.push(RpcValue::from_cpon(cpon)?);
    }
    Ok(values)
}
pub fn result_from_output(output: Output) -> shv::Result<RpcValue> {
    let msg = rpcmsg_from_output(output)?;
    let result = msg.result()?;
    //println!("cpon: {}, expected: {}", result, expected_value.to_cpon());
    //assert_eq!(result, expected_value);
    Ok(result.clone())
}
pub fn shv_call(path: &str, method: &str, param: &str) -> shv::Result<RpcValue> {
    let output = Command::new("target/debug/shvcall")
        .arg("-v").arg(".:T")
        .arg("--url").arg("tcp://admin:admin@localhost")
        .arg("--path").arg(path)
        .arg("--method").arg(method)
        .arg("--param").arg(param)
        .output()?;

    result_from_output(output)
}
pub fn shv_call_many(calls: Vec<String>) -> shv::Result<Vec<RpcValue>> {
    let mut child = Command::new("target/debug/shvcall")
        .arg("--url").arg("tcp://admin:admin@localhost")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .arg("-r")
        .arg("-v").arg(".:I")
        .spawn()?;
    let mut stdin = child.stdin.take().expect("shvcall should be running");
    thread::spawn(move || {
        for line in calls {
            stdin.write_all(line.as_bytes()).expect("Failed to write to stdin");
            stdin.write_all("\n".as_bytes()).expect("Failed to write to stdin");
        }
    });
    let output = child.wait_with_output()?;
    value_list_from_output(output)
}