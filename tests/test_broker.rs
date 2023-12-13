extern crate shv;

use std::process::{Child, Command};
use std::{thread, time::Duration};
use shv::{metamethod, RpcMessage, RpcValue, rpcvalue};
use shv::metamethod::{Flag, MetaMethod};
use shv::shvnode::{METH_DIR, METH_LS, METH_NAME, METH_PING};

struct KillProcessGuard {
    child: Child,
}
impl KillProcessGuard {
    fn new(child: Child) -> Self {
        KillProcessGuard {
            child,
        }
    }

    fn is_running(&mut self) -> bool {
        let status = self.child.try_wait().unwrap();
        status.is_none()
    }
}
impl Drop for KillProcessGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let exit_status= self.child.wait();
        println!("shvbroker exit status: {:?}", exit_status);
    }
}

#[test]
fn test_broker() -> Result<(), Box<dyn std::error::Error>> {
    let mut broker_process_guard = KillProcessGuard::new(Command::new("target/debug/shvbroker")
        .arg("-v").arg(".:W")
        //.arg("-v").arg("Acl")
        .spawn()?);
    thread::sleep(Duration::from_millis(100));
    assert!(broker_process_guard.is_running());

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
        let result = msg.result()?;
        //println!("cpon: {}, expected: {}", result, expected_value.to_cpon());
        //assert_eq!(result, expected_value);
        Ok(result.clone())
    }

    println!("====== broker =====");
    println!("---broker---: :ls(.app)");
    assert_eq!(call("", "ls", r#"".app""#)?, RpcValue::from(true));
    assert_eq!(call(".app", "ls", r#""broker""#)?, RpcValue::from(true));
    assert_eq!(call(".app/broker", "ls", r#""client""#)?, RpcValue::from(true));
    {
        println!("---broker---: .app:dir()");
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
        println!("---broker---: .app:dir(true)");
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
        println!("---broker---: .app:dir(\"ping\")");
        {
            let method = call(".app", "dir", r#""ping""#)?;
            assert!(method.is_imap());
            let name = method.as_imap().get(&metamethod::DirAttribute::Name.into()).ok_or("Name attribute doesn't exist")?.as_str();
            assert_eq!(name, "ping");
        }
    }
    println!("---broker---: .app:ping()");
    assert_eq!(call(".app", "ping", "")?, RpcValue::null());


    println!("====== device =====");
    let mut device_process_guard = KillProcessGuard {
        child: Command::new("target/debug/examples/device")
            .arg("--url").arg("tcp://admin:admin@localhost")
            .arg("--mount").arg("test/device")
            .spawn()?
    };
    thread::sleep(Duration::from_millis(100));
    assert!(device_process_guard.is_running());

    println!("---broker---: test:ls()");
    assert_eq!(call("test", "ls", "")?, (vec![RpcValue::from("device")].into()));
    println!("---broker---: test/device:ls()");
    assert_eq!(call("test/device", "ls", "")?, (vec![RpcValue::from(".app"), RpcValue::from("number")].into()));
    println!("---broker---: test/device/.app:ping()");
    assert_eq!(call("test/device/.app", "ping", "")?, RpcValue::null());
    println!("---broker---: test/device/number:ls()");
    assert_eq!(call("test/device/number", "ls", "")?, (rpcvalue::List::new().into()));

    Ok(())
}
