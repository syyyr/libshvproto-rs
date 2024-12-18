#[cfg(test)]
mod test {
    use std::process::{Command, Output, Stdio};
    use std::{thread};
    use std::io::Write;
    use shvproto::{RpcValue};

    fn run_cp2cp(data: &str) -> Result<Output, String> {
        let block = hex::decode(data).expect("HEX decoding failed");
        let mut cmd = Command::new("target/debug/cp2cp");
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("--chainpack-rpc-block");
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().expect("cp2cp should be running");
        thread::spawn(move || {
            stdin.write_all(&block).expect("Failed to write to stdin");
        });
        child.wait_with_output().map_err(|e| e.to_string())
    }
    #[test]
    fn chainpack_rpc_block_valid() -> Result<(), String> {
        // <T:RpcMessage,id:4,method:"ls">i{}
        let data = "0E018B414148444A86026C73FF8AFF";
        let output = run_cp2cp(data)?;
        let exit_code = output.status.code().unwrap();
        let output = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        let mut lines = output.split("\n");
        let block_length = lines.next().unwrap().parse::<usize>().unwrap();
        let frame_length = lines.next().unwrap().parse::<usize>().unwrap();
        let proto = lines.next().unwrap().parse::<i32>().unwrap();
        let cpon = lines.next().unwrap().to_owned();

        assert_eq!(exit_code, 0);
        assert_eq!(block_length, 15);
        assert_eq!(frame_length, 14);
        assert_eq!(proto, 1);
        let rv = RpcValue::from_cpon(&cpon).unwrap();
        assert!(rv.is_imap());
        // println!("{}", rv.to_cpon());
        assert_eq!(rv.meta().get(8).unwrap().as_int(), 4);
        assert_eq!(rv.meta().get(10).unwrap().as_str(), "ls");
        Ok(())
    }
    #[test]
    fn chainpack_rpc_block_error() -> Result<(), String> {
        // <T:RpcMessage,id:4,method:"ls">i{}
        let data = "0E0190414148444A86026C73FF8AFF";
        let output = run_cp2cp(data)?;
        let exit_code = output.status.code().unwrap();
        assert_eq!(exit_code, 1);
        Ok(())
    }
    #[test]
    fn chainpack_rpc_block_not_enough_data() -> Result<(), String> {
        // <T:RpcMessage,id:4,method:"ls">i{}
        let data = "0E018B414148444A86026C73FF8A";
        let output = run_cp2cp(data)?;
        let exit_code = output.status.code().unwrap();
        assert_eq!(exit_code, 2);
        Ok(())
    }
    #[test]
    fn chainpack_rpc_block_invalid_length() -> Result<(), String> {
        // <T:RpcMessage,id:4,method:"ls">i{}
        let data = "FFFF";
        let output = run_cp2cp(data)?;
        let exit_code = output.status.code().unwrap();
        assert_eq!(exit_code, 1);
        Ok(())
    }
    #[test]
    fn chainpack_rpc_block_invalid_protocol() -> Result<(), String> {
        // <T:RpcMessage,id:4,method:"ls">i{}
        let data = "0103";
        let output = run_cp2cp(data)?;
        let exit_code = output.status.code().unwrap();
        assert_eq!(exit_code, 1);
        Ok(())
    }
}