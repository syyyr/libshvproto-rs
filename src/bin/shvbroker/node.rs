use lazy_static::lazy_static;
use shv::metamethod::MetaMethod;
use shv::{RpcMessage, RpcMessageMetaTags, RpcValue};
use shv::shvnode::{DIR_LS_METHODS, ProcessRequestResult, ShvNode};

const METH_CLIENT_INFO: &str = "clientInfo";

lazy_static! {
    pub static ref APP_BROKER_METHODS: [MetaMethod; 1] = [
        MetaMethod { name: METH_CLIENT_INFO.into(), param: "Int".into(), result: "Map".into(), ..Default::default() },
    ];
}
pub(crate) struct AppBrokerNode {}
impl ShvNode for AppBrokerNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(APP_BROKER_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        match rpcmsg.method() {
            Some(METH_CLIENT_INFO) => {
                Ok((RpcValue::from(()), None))
            }
            _ => {
                ShvNode::process_request_dir_ls(self, rpcmsg)
            }
        }
    }
}

pub(crate) struct CurrentClientNode {}
