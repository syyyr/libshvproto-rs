use shv::metamethod::{Access, Flag, MetaMethod};
use shv::{RpcMessage, RpcMessageMetaTags, RpcValue};
use shv::shvnode::{DIR_LS_METHODS, ProcessRequestResult, ShvNode};
use crate::Broker;

const METH_CLIENT_INFO: &str = "clientInfo";
const METH_MOUNTED_CLIENT_INFO: &str = "mountedClientInfo";
const METH_CLIENTS: &str = "clients";

const APP_BROKER_METHODS: [MetaMethod; 3] = [
    MetaMethod { name: METH_CLIENT_INFO, param: "Int", result: "ClientInfo", access: Access::Service, flags: Flag::None as u32, description: "" },
    MetaMethod { name: METH_MOUNTED_CLIENT_INFO, param: "String", result: "ClientInfo", access: Access::Service, flags: Flag::None as u32, description: "" },
    MetaMethod { name: METH_CLIENTS, param: "", result: "List[Int]", access: Access::Service, flags: Flag::None as u32, description: "" },
];

pub(crate) struct AppBrokerNode {}
impl ShvNode<crate::Broker> for AppBrokerNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(APP_BROKER_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rq: &RpcMessage, broker: &mut crate::Broker) -> ProcessRequestResult {
        match rq.method() {
            Some(METH_CLIENT_INFO) => {
                let client_id = rq.param().unwrap_or_default().as_i32();
                match broker.client_info(client_id) {
                    None => { Ok((RpcValue::null(), None)) }
                    Some(info) => { RpcValue::from(info).into() }
                }
            }
            Some(METH_MOUNTED_CLIENT_INFO) => {
                let mount_point = rq.param().unwrap_or_default().as_str();
                match broker.mounted_client_info(mount_point) {
                    None => { Ok((RpcValue::null(), None)) }
                    Some(info) => { RpcValue::from(info).into() }
                }
            }
            Some(METH_CLIENTS) => {
                let clients = broker.clients();
                RpcValue::from(clients).into()
            }
            _ => {
                ShvNode::<crate::Broker>::process_dir_ls(self, rq)
            }
        }
    }
}

const APP_BROKER_CURRENT_CLIENT_METHODS: [MetaMethod; 1] = [
    MetaMethod { name: METH_INFO, flags: Flag::None as u32, access: Access::Browse, param: "Int", result: "ClientInfo", description: "" },
];
const METH_INFO: &str = "info";

pub(crate) struct AppBrokerCurrentClientNode {}
impl ShvNode<Broker> for AppBrokerCurrentClientNode {
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
                ShvNode::<Broker>::process_dir_ls(self, rq)
            }
        }
    }
}
