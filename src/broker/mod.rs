use async_std::{channel, task};
use async_std::net::TcpListener;
use crate::broker::config::{AccessControl, BrokerConfig};
use std::collections::{BTreeMap};
use async_std::channel::unbounded;
use glob::{Pattern};
use log::{debug, info, warn};
use crate::{RpcMessage};
use crate::rpc::{SubscriptionPattern};
use crate::rpcframe::RpcFrame;
use crate::rpcmessage::{CliId, RqId};
use crate::shvnode::{find_longest_prefix, ShvNode};
use async_std::stream::StreamExt;
use futures::select;
use futures::FutureExt;
use crate::broker::state::State;

pub mod config;
pub mod peer;
pub mod node;
mod call;
#[cfg(test)]
mod test;
mod state;

type Sender<T> = async_std::channel::Sender<T>;
type Receiver<T> = async_std::channel::Receiver<T>;

pub(crate) enum LoginResult {
    Ok,
    ClientSocketClosed,
}

#[derive(Debug)]
pub(crate) enum PeerToBrokerMessage {
    GetPassword {
        client_id: CliId,
        user: String,
    },
    NewPeer {
        client_id: CliId,
        sender: Sender<BrokerToPeerMessage>,
        peer_kind: PeerKind,
    },
    RegisterDevice {
        client_id: CliId,
        device_id: Option<String>,
        mount_point: Option<String>,
    },
    #[allow(dead_code)]
    SetSubscribePath {
        // used for testing, to bypass device discovery RPC Calls
        client_id: CliId,
        subscribe_path: SubscribePath,
    },
    FrameReceived {
        client_id: CliId,
        frame: RpcFrame,
    },
    PeerGone {
        peer_id: CliId,
    },
}

#[derive(Debug)]
pub(crate) enum BrokerToPeerMessage {
    PasswordSha1(Option<Vec<u8>>),
    SendFrame(RpcFrame),
    SendMessage(RpcMessage),
    DisconnectByBroker,
}

#[derive(Debug)]
pub enum PeerKind {
    Client,
    ParentBroker,
}

#[derive(Debug)]
struct Peer {
    sender: Sender<BrokerToPeerMessage>,
    user: String,
    mount_point: Option<String>,
    subscriptions: Vec<SubscriptionPattern>,
    peer_kind: PeerKind,
    subscribe_path: Option<SubscribePath>,
}

impl Peer {
    fn new(peer_kind: PeerKind, sender: Sender<BrokerToPeerMessage>) -> Self {
        Self {
            sender,
            user: "".to_string(),
            mount_point: None,
            subscriptions: vec![],
            peer_kind,
            subscribe_path: None,
        }
    }
    fn is_signal_subscribed(&self, path: &str, method: &str) -> bool {
        for subs in self.subscriptions.iter() {
            if subs.match_shv_method(path, method) {
                return true;
            }
        }
        false
    }
    pub fn is_broker(&self) -> crate::Result<bool> {
        match &self.subscribe_path {
            None => { Err(format!("Device mounted on: {:?} - not checked for broker capability yet.", self.mount_point).into()) }
            Some(path) => {
                match path {
                    SubscribePath::NotBroker => { Ok(false) }
                    SubscribePath::CanSubscribe(_) => { Ok(true) }
                }
            }
        }
    }
    pub fn broker_subscribe_path(&self) -> crate::Result<String> {
        match &self.subscribe_path {
            None => { Err(format!("Device mounted on: {:?} - not checked for broker capability yet.", self.mount_point).into()) }
            Some(path) => {
                match path {
                    SubscribePath::NotBroker => { Err("Not broker".into()) }
                    SubscribePath::CanSubscribe(path) => { Ok(path.to_string()) }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SubscribePath {
    NotBroker,
    CanSubscribe(String),
}

struct Device {
    client_id: CliId,
}

impl Device {
}

type Node<K> = Box<dyn ShvNode<K> + Send + Sync>;

enum Mount<K> {
    Peer(Device),
    Node(Node<K>),
}

struct ParsedAccessRule {
    path_method: SubscriptionPattern,
    grant: String,
}

impl ParsedAccessRule {
    pub fn new(path: &str, method: &str, grant: &str) -> crate::Result<Self> {
        let method = if method.is_empty() { "?*" } else { method };
        let path = if path.is_empty() { "**" } else { path };
        match Pattern::new(method) {
            Ok(method) => {
                match Pattern::new(path) {
                    Ok(path) => {
                        Ok(Self {
                            path_method: SubscriptionPattern { paths: path, methods: method },
                            grant: grant.to_string(),
                        })
                    }
                    Err(err) => { Err(format!("{}", &err).into()) }
                }
            }
            Err(err) => { Err(format!("{}", &err).into()) }
        }
    }
}

fn find_mount_mut<'a, 'b, K>(mounts: &'a mut BTreeMap<String, Mount<K>>, shv_path: &'b str) -> Option<(&'a mut Mount<K>, &'b str)> {
    if let Some((mount_dir, node_dir)) = find_longest_prefix(mounts, shv_path) {
        Some((mounts.get_mut(mount_dir).unwrap(), node_dir))
    } else {
        None
    }
}
struct PendingRpcCall {
    client_id: CliId,
    request_id: RqId,
    response_sender: Sender<RpcFrame>,
}

pub(crate) async fn broker_loop(peers_messages: Receiver<PeerToBrokerMessage>, access: AccessControl) {
    let (broker_command_sender, broker_command_receiver) = unbounded();
    let mut broker = State::new(access, broker_command_sender);

    broker.mounts.insert(".app".into(), Mount::Node(Box::new(crate::shvnode::AppNode { app_name: "shvbroker", ..Default::default() })));
    broker.mounts.insert(".app/broker".into(), Mount::Node(Box::new(node::AppBrokerNode {})));
    broker.mounts.insert(".app/broker/currentClient".into(), Mount::Node(Box::new(node::AppBrokerCurrentClientNode {})));
    loop {
        select! {
            command = broker_command_receiver.recv().fuse() => match command {
                Ok(command) => {
                    if let Err(err) = broker.process_broker_command(command).await {
                        warn!("Process broker command error: {}", err);
                    }
                }
                Err(err) => {
                    warn!("Reseive broker command error: {}", err);
                }
            },
            event = peers_messages.recv().fuse() => match event {
                Err(e) => {
                    info!("Peers channel closed, accept loop exited: {}", &e);
                    break;
                }
                Ok(message) => {
                    match message {
                        PeerToBrokerMessage::FrameReceived { client_id, frame} => {
                            if let Err(err) = broker.process_rpc_frame(client_id, frame).await {
                                warn!("Process RPC frame error: {err}");
                            }
                        }
                        PeerToBrokerMessage::NewPeer { client_id, sender, peer_kind } => {
                            if broker.peers.insert(client_id, Peer::new(peer_kind, sender)).is_some() {
                                // this might happen when connection to parent broker is restored
                                // after parent broker reset
                                // note that parent broker connection has always ID == 1
                                debug!("Client ID: {client_id} exists already!");
                            }
                        },
                        PeerToBrokerMessage::RegisterDevice { client_id, device_id, mount_point } => {
                            let _ = broker.mount_device(client_id, device_id, mount_point).await;
                        },
                        PeerToBrokerMessage::SetSubscribePath {client_id, subscribe_path} => {
                            let _ = broker.set_subscribe_path(client_id, subscribe_path);
                        }
                        PeerToBrokerMessage::PeerGone { peer_id } => {
                            broker.peers.remove(&peer_id);
                            info!("Client id: {} disconnected.", peer_id);
                            if let Some(path) = broker.client_id_to_mount_point(peer_id) {
                                info!("Unmounting path: '{}'", path);
                                broker.mounts.remove(&path);
                            }
                            let client_path = format!(".app/broker/client/{}", peer_id);
                            broker.mounts.remove(&client_path);
                            broker.pending_rpc_calls.retain(|c| c.client_id != peer_id);
                        }
                        PeerToBrokerMessage::GetPassword { client_id, user } => {
                            let shapwd = broker.sha_password(&user);
                            let peer = broker.peers.get_mut(&client_id).unwrap();
                            peer.user = user.clone();
                            peer.sender.send(BrokerToPeerMessage::PasswordSha1(shapwd)).await.unwrap();
                        }
                    }
                }
            }
        }
    }
}

pub async fn accept_loop(config: BrokerConfig, access: AccessControl) -> crate::Result<()> {
    if let Some(address) = config.listen.tcp.clone() {
        let (broker_sender, broker_receiver) = channel::unbounded();
        let parent_broker_peer_config = config.parent_broker.clone();
        let broker_task = task::spawn(crate::broker::broker_loop(broker_receiver, access));
        if parent_broker_peer_config.enabled {
            crate::spawn_and_log_error(peer::parent_broker_peer_loop_with_reconnect(1, parent_broker_peer_config, broker_sender.clone()));
        }
        info!("Listening on TCP: {}", address);
        let listener = TcpListener::bind(address).await?;
        info!("bind OK");
        let mut client_id = 2; // parent broker has client_id == 1
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream?;
            debug!("Accepting from: {}", stream.peer_addr()?);
            crate::spawn_and_log_error(peer::peer_loop(client_id, broker_sender.clone(), stream));
            client_id += 1;
        }
        drop(broker_sender);
        broker_task.await;
    } else {
        return Err("No port to listen on specified".into());
    }
    Ok(())
}

