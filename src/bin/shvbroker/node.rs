use lazy_static::lazy_static;
use shv::metamethod::{Access, MetaMethod};
use shv::{RpcMessage, RpcMessageMetaTags, RpcValue};
use shv::shvnode::{DIR_LS_METHODS, ProcessRequestResult, ShvNode};
use crate::Broker;

const METH_CLIENT_INFO: &str = "clientInfo";
const METH_MOUNTED_CLIENT_INFO: &str = "mountedClientInfo";
const METH_CLIENTS: &str = "clients";
const METH_INFO: &str = "info";

lazy_static! {
    pub static ref APP_BROKER_METHODS: [MetaMethod; 3] = [
        MetaMethod { name: METH_CLIENT_INFO.into(), param: "Int".into(), result: "ClientInfo".into(), access: Access::Service, ..Default::default() },
        MetaMethod { name: METH_MOUNTED_CLIENT_INFO.into(), param: "String".into(), result: "ClientInfo".into(), access: Access::Service, ..Default::default() },
        MetaMethod { name: METH_CLIENTS.into(), param: "".into(), result: "List[Int]".into(), access: Access::Service, ..Default::default() },
    ];
    pub static ref APP_BROKER_CURRENT_CLIENT_METHODS: [MetaMethod; 1] = [
        MetaMethod { name: METH_INFO.into(), param: "Int".into(), result: "ClientInfo".into(), ..Default::default() },
    ];
}
pub(crate) struct AppBrokerNode {}
impl ShvNode<crate::Broker> for AppBrokerNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(APP_BROKER_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rq: &RpcMessage, broker: &mut crate::Broker) -> ProcessRequestResult {
        match rq.method() {
            Some(METH_CLIENT_INFO) => {
                let client_id = broker.request_context.caller_client_id;
                Ok((RpcValue::from(broker.client_info(client_id).unwrap_or_default()), None))
            }
            //Some(METH_MOUNTED_CLIENT_INFO) => {
            //    let mount_path = rq.param().unwrap_or_default().as_str();
            //    Ok((RpcValue::from(broker.mounted_client_info(mount_path).unwrap_or_default()), None))
            //}
            _ => {
                ShvNode::<crate::Broker>::process_dir_ls(self, rq)
            }
        }
    }
}

pub(crate) struct AppBrokerCurrentClientNode {}
impl ShvNode<crate::Broker> for AppBrokerCurrentClientNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(APP_BROKER_CURRENT_CLIENT_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rq: &RpcMessage, broker: &mut Broker) -> ProcessRequestResult {
        match rq.method() {
            Some(METH_INFO) => {
                let client_id = broker.request_context.caller_client_id;
                Ok((RpcValue::from(broker.client_info(client_id).unwrap_or_default()), None))
            }
            _ => {
                ShvNode::<crate::Broker>::process_dir_ls(self, rq)
            }
        }
    }
}
