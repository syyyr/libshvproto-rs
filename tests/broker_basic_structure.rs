extern crate shv;

use std::process::{Child, Command};
use std::{thread, time::Duration};
use shv::{metamethod, RpcMessage, RpcValue};
use shv::metamethod::{Flag, MetaMethod};
use shv::shvnode::{METH_DIR, METH_LS, METH_NAME, METH_PING};

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
    let _device_process_guard = KillProcessGuard {
        child: Command::new("target/debug/examples/device")
        .arg("--url").arg("tcp://admin:admin@localhost")
        .arg("--mount").arg("test/device")
        .spawn()?
    };

    fn call(path: &str, method: &str, param: &str) -> Result<RpcValue, Box<dyn std::error::Error>> {
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
        //println!("cpon: {}, expected: {}", result, expected_value.to_cpon());
        //assert_eq!(result, expected_value);
        Ok(result.clone())
    }

    //assert_eq!(call("", "ls", "")?, (vec![RpcValue::from(".app")].into()));
    assert_eq!(call("", "ls", r#"".app""#)?, RpcValue::from(true));
    {
        let expected_methods = vec![
            MetaMethod { name: METH_DIR.into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
            MetaMethod { name: METH_LS.into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
            MetaMethod { name: METH_NAME.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
            MetaMethod { name: METH_PING.into(), ..Default::default() },
        ];
        {
            let methods = call(".app", "dir", "")?;
            let methods = methods.as_list();
            'outer: for xmm in expected_methods.iter() {
                for mm in methods.iter() {
                    assert!(mm.is_imap());
                    let name = mm.as_imap().get(&metamethod::DirAttribute::Name.into()).ok_or("Name attribute doesn't exist")?.as_str();
                    if name == xmm.name {
                        continue 'outer;
                    }
                }
                panic!("Method name '{}' is not found", xmm.name);
            }
        }
        {
            let methods = call(".app", "dir", "true")?;
            let methods = methods.as_list();
            'outer: for xmm in expected_methods.iter() {
                for mm in methods.iter() {
                    assert!(mm.is_map());
                    let name = mm.as_map().get("name").ok_or("Name attribute doesn't exist")?.as_str();
                    if name == xmm.name {
                        continue 'outer;
                    }
                }
                panic!("Method name '{}' is not found", xmm.name);
            }
        }
        {
            let method = call(".app", "dir", r#""ping""#)?;
            assert!(method.is_imap());
            let name = method.as_imap().get(&metamethod::DirAttribute::Name.into()).ok_or("Name attribute doesn't exist")?.as_str();
            assert_eq!(name, "ping");
        }
    }
    assert_eq!(call(".app", "ping", "")?, RpcValue::null());

    assert_eq!(call("test", "ls", "")?, (vec![RpcValue::from("device")].into()));
    assert_eq!(call("test/device/.app", "ping", "")?, RpcValue::null());

    Ok(())
}
