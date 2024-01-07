use crate::broker::{Sender, SubscribePath};
use crate::rpcframe::RpcFrame;
use crate::rpcmessage::{CliId};
use crate::{RpcMessage};

pub enum BrokerCommand {
    RpcCall{
        client_id: CliId,
        request: RpcMessage,
        response_sender: Sender<RpcFrame>,
    },
    SetSubscribePath{
        client_id: CliId,
        subscribe_path: SubscribePath,
    },
    PropagateSubscriptions{
        client_id: CliId,
    },
    //CallSubscribe(CallSubscribeData),
}
