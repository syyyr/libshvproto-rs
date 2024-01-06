use crate::broker::{Broker, Mount, SubscribeMethodPath};
use crate::rpcframe::RpcFrame;
use crate::rpcmessage::{CliId, RqId};
use crate::broker::node::METH_SUBSCRIBE;
use crate::rpc::Subscription;
use crate::{RpcMessage};
use crate::shvnode::METH_DIR;

pub(crate) struct PendingBrokerRequest {
    pub(crate) request_id: RqId,
    pub(crate) command: BrokerRequestCommand,
}
// enum is used as dull man async trait replacement
// async trait can lead to nicer code, but I'm not able to make it work
pub(crate) enum BrokerRequestCommand {
    FindSubscribeMethodPath(FindSubscribeMethodPathsState),
}
impl BrokerRequestCommand {
    pub(crate) fn request(&self) -> crate::Result<RpcMessage> {
        match self {
            BrokerRequestCommand::FindSubscribeMethodPath(ctx) => {
                ctx.request()
            }
        }
    }
}
#[derive(Clone)]
pub(crate) struct FindSubscribeMethodPathsState {
    client_id: CliId,
    mount_point: String,
    checked_paths: Vec<String>,
    to_subscribe: Vec<Subscription>,
}
impl FindSubscribeMethodPathsState {
    pub fn new(client_id: CliId, mount_point: &str, to_subscribe: Vec<Subscription>) -> Self {
        Self {
            client_id,
            mount_point: mount_point.to_string(),
            checked_paths: vec![
                "/.app/broker/currentClient".to_string(),
                "/.app/broker/currentClient".to_string(),
                "/.broker/app".to_string(),
            ],
            to_subscribe,
        }
    }
    fn request(&self) -> crate::Result<RpcMessage> {
        if self.checked_paths.is_empty() {
            Err("No path to check".into())
        } else {
            Ok(RpcMessage::new_request(&self.checked_paths[0], METH_DIR, Some(METH_SUBSCRIBE.into())))
        }
    }
    pub(crate) async fn process_response(mut self, broker: &mut Broker, response: &RpcFrame) -> crate::Result<()> {
        assert!(!self.checked_paths.is_empty());
        let rpcmsg = response.to_rpcmesage()?;
        let path_exists = match rpcmsg.result() {
            Ok(result) => { !result.is_null() }
            Err(_) => { false }
        };
        if path_exists {
            let subscribe_path = &self.checked_paths[0];
            Self::set_device_subscribe_path(broker, &self.mount_point, SubscribeMethodPath::CanSubscribe(subscribe_path.to_string()))?;
            broker.subscribe_paths(self.client_id, self.to_subscribe).await?;
        } else {
            // path on top does not exist
            if self.checked_paths.len() <= 1 {
                // no path exists
                Self::set_device_subscribe_path(broker, &self.mount_point, SubscribeMethodPath::NotBroker)?;
            } else {
                //  try next path
                let client_id = self.client_id;
                self.checked_paths.remove(0);
                let cmd = BrokerRequestCommand::FindSubscribeMethodPath(self);
                broker.start_broker_request(client_id, cmd).await?;
            }
        };
        Ok(())
    }
    fn set_device_subscribe_path(broker: &mut Broker, mount_point: &str, subscribe_path: SubscribeMethodPath) -> crate::Result<()> {
        let mount = broker.mounts.get_mut(mount_point).ok_or_else(|| "Invalid mount point")?;
        if let Mount::Peer(device) = mount {
            device.subscribe_path = Some(subscribe_path);
            Ok(())
        } else {
            Err(format!("Not device mount point: {}", mount_point).into())
        }
    }
}
