use crate::broker::{Sender, SubscribePath};
use crate::rpcframe::RpcFrame;
use crate::rpcmessage::{CliId};
use crate::{RpcMessage};

pub enum BrokerCommand {
    RpcCall(RpcCallData),
    SetSubscribePath(SetSubscribePathData),
    PropagateSubscriptions(PropagateSubscriptionsData),
    //CallSubscribe(CallSubscribeData),
}
pub(crate) struct SetSubscribePathData {
    pub(crate) client_id: CliId,
    pub(crate) subscribe_path: SubscribePath,
}
pub(crate) struct PropagateSubscriptionsData {
    pub(crate) client_id: CliId,
}
//pub(crate) struct CallSubscribeData {
//    pub(crate) client_id: CliId,
//    pub(crate) subscriptions: Vec<Subscription>,
//}
pub(crate) struct RpcCallData {
    pub(crate) client_id: CliId,
    pub(crate) request: RpcMessage,
    pub(crate) response_sender: Sender<RpcFrame>,
}
