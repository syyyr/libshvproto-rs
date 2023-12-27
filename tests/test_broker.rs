extern crate shv;

use std::process::{Command};
use std::{thread, time::Duration};
use shv::{metamethod, RpcValue, rpcvalue};
use shv::metamethod::{Flag, MetaMethod};
use shv::shvnode::{METH_DIR, METH_LS, METH_NAME, METH_PING};
use crate::common::{KillProcessGuard, shv_call, shv_call_many, ShvCallOutputFormat};

mod common;

#[test]
fn test_broker() -> shv::Result<()> {
    let mut broker_process_guard = KillProcessGuard::new(Command::new("target/debug/shvbroker")
        .arg("-v").arg(".:W")
        //.arg("-v").arg("Acl")
        .spawn()?);
    thread::sleep(Duration::from_millis(100));
    assert!(broker_process_guard.is_running());

    println!("====== broker =====");
    println!("---broker---: :ls(.app)");
    assert_eq!(shv_call("", "ls", r#"".app""#)?, RpcValue::from(true));
    assert_eq!(shv_call(".app", "ls", r#""broker""#)?, RpcValue::from(true));
    assert_eq!(shv_call(".app/broker", "ls", r#""client""#)?, RpcValue::from(true));
    {
        println!("---broker---: .app:dir()");
        let expected_methods = vec![
            MetaMethod { name: METH_DIR.into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
            MetaMethod { name: METH_LS.into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
            MetaMethod { name: METH_NAME.into(), flags: Flag::IsGetter as u32,  ..Default::default() },
            MetaMethod { name: METH_PING.into(), ..Default::default() },
        ];
        {
            let methods = shv_call(".app", "dir", "")?;
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
            let methods = shv_call(".app", "dir", "true")?;
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
            let method = shv_call(".app", "dir", r#""ping""#)?;
            assert!(method.is_imap());
            let name = method.as_imap().get(&metamethod::DirAttribute::Name.into()).ok_or("Name attribute doesn't exist")?.as_str();
            assert_eq!(name, "ping");
        }
    }
    println!("---broker---: .app:ping()");
    assert_eq!(shv_call(".app", "ping", "")?, RpcValue::null());

    println!("====== device =====");
    let mut device_process_guard = common::KillProcessGuard {
        child: Command::new("target/debug/examples/device")
            .arg("--url").arg("tcp://admin:admin@localhost")
            .arg("--mount").arg("test/device")
            .spawn()?
    };
    thread::sleep(Duration::from_millis(100));
    assert!(device_process_guard.is_running());

    println!("---broker---: test:ls()");
    assert_eq!(shv_call("test", "ls", "")?, vec![RpcValue::from("device")].into());
    println!("---broker---: test/device:ls()");
    assert_eq!(shv_call("test/device", "ls", "")?, vec![RpcValue::from(".app"), RpcValue::from("number")].into());
    println!("---broker---: test/device/.app:ping()");
    assert_eq!(shv_call("test/device/.app", "ping", "")?, RpcValue::null());
    println!("---broker---: test/device/number:ls()");
    assert_eq!(shv_call("test/device/number", "ls", "")?, rpcvalue::List::new().into());

    println!("---broker---: .app/broker:clients()");
    assert_eq!(shv_call(".app/broker", "clients", "")?.as_list().len(), 2); // [device-id, shvcall-id]

    println!("---broker---: .app/broker:mounts()");
    assert_eq!(shv_call(".app/broker", "mounts", "")?, vec![RpcValue::from("test/device")].into());
    {
        println!("====== subscriptions =====");
        let calls: Vec<String> = vec![
            r#".app/broker/currentClient:subscribe {"methods": "chng", "paths": "test/**"}"#.into(),
            r#"test/device/number:set 42"#.into(),
            r#".app/broker/currentClient:unsubscribe {"methods": "chng", "paths": "test/**"}"#.into(),
            r#"test/device/number:set 123"#.into(),
        ];
        let values = shv_call_many(calls, ShvCallOutputFormat::Simple)?;
        for v in values.iter() {
            println!("\t{}", v);
        }
        let expected: Vec<&str> = vec![
            "RES null", // response to subscribe
            "SIG test/device/number:chng 42", // SIG chng
            "RES null", // response to SET
            "RES true", // response to unsubscribe
            "RES null", // response to SET
        ];
        for (no, val) in values.iter().enumerate() {
            assert_eq!(expected[no], val);
        }
    }
    Ok(())
}
