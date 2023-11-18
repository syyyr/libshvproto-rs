use lazy_static::lazy_static;
use shv::metamethod::{Flag, MetaMethod};
use shv::{RpcMessage, RpcMessageMetaTags, RpcValue};
use shv::shvnode::{ProcessRequestResult, ShvNode};

lazy_static! {
    static ref DIR_LS: [MetaMethod; 2] = [
        MetaMethod { name: "dir".into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
        MetaMethod { name: "ls".into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
    ];
}

struct DirNode {
    // methods: Vec<MetaMethod>
}
impl ShvNode for DirNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS.iter().collect()
    }

    fn process_request(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        Err(format!("NIY {}", rpcmsg).into())
    }
}

lazy_static! {
    static ref APP_METHODS: [MetaMethod; 2] = [
        MetaMethod { name: "ping".into(), ..Default::default() },
        MetaMethod { name: "name".into(), flags: Flag::IsGetter.into(),  ..Default::default() },
    ];
}
pub(crate) struct AppNode {
}
impl ShvNode for AppNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS.iter().chain(APP_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        match rpcmsg.method() {
            Some("ping") => {
                Ok(Some(RpcValue::from(())))
            }
            Some("name") => {
                Ok(Some(RpcValue::from("shvbroker")))
            }
            _ => {
                ShvNode::process_request_dir(self, rpcmsg)
            }
        }
    }
}