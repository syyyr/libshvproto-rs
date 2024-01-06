use crate::metamethod::{Access, Flag, MetaMethod};
use crate::{RpcMessage, RpcMessageMetaTags};
use crate::rpc::{Subscription};
use crate::rpcmessage::{CliId};
use crate::shvnode::{RequestCommand, ShvNode};

const METH_CLIENT_INFO: &str = "clientInfo";
const METH_MOUNTED_CLIENT_INFO: &str = "mountedClientInfo";
const METH_CLIENTS: &str = "clients";
const METH_MOUNTS: &str = "mounts";
const METH_DISCONNECT_CLIENT: &str = "disconnectClient";

const APP_BROKER_METHODS: [MetaMethod; 5] = [
    MetaMethod { name: METH_CLIENT_INFO, param: "Int", result: "ClientInfo", access: Access::Service, flags: Flag::None as u32, description: "" },
    MetaMethod { name: METH_MOUNTED_CLIENT_INFO, param: "String", result: "ClientInfo", access: Access::Service, flags: Flag::None as u32, description: "" },
    MetaMethod { name: METH_CLIENTS, param: "void", result: "List[Int]", access: Access::SuperService, flags: Flag::None as u32, description: "" },
    MetaMethod { name: METH_MOUNTS, param: "void", result: "List[String]", access: Access::SuperService, flags: Flag::None as u32, description: "" },
    MetaMethod { name: METH_DISCONNECT_CLIENT, param: "Int", result: "void", access: Access::SuperService, flags: Flag::None as u32, description: "" },
];
pub enum BrokerNodeCommand {
    ClientInfo(CliId),
    MountedClientInfo(String),
    Clients,
    Mounts,
    DisconnectClient(CliId),
    CurrentClientInfo,
    Subscribe(Subscription),
    Unsubscribe(Subscription),
    Subscriptions,
}
pub(crate) struct AppBrokerNode {}
impl ShvNode<BrokerNodeCommand> for AppBrokerNode {
    fn defined_methods(&self) -> Vec<&MetaMethod> {
        APP_BROKER_METHODS.iter().collect()
    }

    fn process_request(&mut self, rq: &RpcMessage) -> RequestCommand<BrokerNodeCommand> {
        match rq.method() {
            Some(METH_CLIENT_INFO) => {
                let client_id = rq.param().unwrap_or_default().as_i32();
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::ClientInfo(client_id))
            }
            Some(METH_MOUNTED_CLIENT_INFO) => {
                let mount_point = rq.param().unwrap_or_default().as_str();
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::MountedClientInfo(mount_point.to_owned()))
            }
            Some(METH_CLIENTS) => {
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::Clients)
            }
            Some(METH_MOUNTS) => {
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::Mounts)
            }
            Some(METH_DISCONNECT_CLIENT) => {
                let client_id = rq.param().unwrap_or_default().as_i32();
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::DisconnectClient(client_id))
            }
            _ => {
                ShvNode::<BrokerNodeCommand>::process_dir_ls(self, rq)
            }
        }
    }
}

const APP_BROKER_CURRENT_CLIENT_METHODS: [MetaMethod; 4] = [
    MetaMethod { name: METH_INFO, flags: Flag::None as u32, access: Access::Browse, param: "Int", result: "ClientInfo", description: "" },
    MetaMethod { name: METH_SUBSCRIBE, flags: Flag::None as u32, access: Access::Browse, param: "SubscribeParams", result: "void", description: "" },
    MetaMethod { name: METH_UNSUBSCRIBE, flags: Flag::None as u32, access: Access::Browse, param: "SubscribeParams", result: "void", description: "" },
    MetaMethod { name: METH_SUBSCRIPTIONS, flags: Flag::None as u32, access: Access::Browse, param: "void", result: "List", description: "" },
];
const METH_INFO: &str = "info";
pub const METH_SUBSCRIBE: &str = "subscribe";
pub const METH_UNSUBSCRIBE: &str = "unsubscribe";
pub const METH_SUBSCRIPTIONS: &str = "subscriptions";

pub(crate) struct AppBrokerCurrentClientNode {}
impl ShvNode<BrokerNodeCommand> for AppBrokerCurrentClientNode {
    fn defined_methods(&self) -> Vec<&MetaMethod> {
        APP_BROKER_CURRENT_CLIENT_METHODS.iter().collect()
    }

    fn process_request(&mut self, rq: &RpcMessage) -> RequestCommand<BrokerNodeCommand> {
        match rq.method() {
            Some(METH_INFO) => {
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::CurrentClientInfo)
            }
            Some(METH_SUBSCRIBE) => {
                let subscription = Subscription::from_rpcvalue(rq.param().unwrap_or_default());
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::Subscribe(subscription))
            }
            Some(METH_UNSUBSCRIBE) => {
                let subscription = Subscription::from_rpcvalue(rq.param().unwrap_or_default());
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::Unsubscribe(subscription))
            }
            Some(METH_SUBSCRIPTIONS) => {
                RequestCommand::<BrokerNodeCommand>::Custom(BrokerNodeCommand::Subscriptions)
            }
            _ => {
                ShvNode::<BrokerNodeCommand>::process_dir_ls(self, rq)
            }
        }
    }
}
