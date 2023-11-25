extern crate shv;

use std::process::Command;
use shv::{RpcMessage, RpcValue};

#[test]
fn it_works() -> Result<(), Box<dyn std::error::Error>> {
    //let mut shvbroker = Command::new("target/debug/shvbroker")
    //    .spawn()?;
        // .expect("failed to execute shvcall");

    let shvcall = Command::new("target/debug/shvcall")
        .arg("--url").arg("tcp://admin:admin@localhost")
        .arg("--path").arg("")
        .arg("--method").arg("ls")
        //.arg("--param").arg(".app")
        .output()?;
        // .expect("failed to execute shvcall");

    let out = shvcall.stdout;
    let cpon = std::str::from_utf8(&out)?;
    let rv = RpcValue::from_cpon(cpon)?;
    let msg = RpcMessage::from_rpcvalue(rv)?;
    //println!("cpon: {}, rv: {}", cpon, rv.to_cpon());
    assert_eq!(msg.result().ok_or("no result")?.as_list(), &vec![RpcValue::from(".app")]);

    //shvbroker.kill()?;

    Ok(())
}
