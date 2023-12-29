use std::{
    collections::hash_map::{Entry, HashMap},
};
use std::collections::BTreeMap;
use futures::{select, FutureExt, StreamExt};
use async_std::{channel, io::BufReader, net::{TcpListener, TcpStream, ToSocketAddrs}, prelude::*, task};
use glob::Pattern;
use rand::distributions::{Alphanumeric, DistString};
use log::*;
use shv::rpcframe::{RpcFrame};
use shv::{List, RpcMessage, RpcValue, rpcvalue, util};
use shv::rpcmessage::{CliId, RpcError, RpcErrorCode};
use shv::RpcMessageMetaTags;
use simple_logger::SimpleLogger;
use shv::rpc::Subscription;
use shv::shvnode::{find_longest_prefix, ShvNode, RequestCommand, process_local_dir_ls, SIG_CHNG};
use shv::util::{parse_log_verbosity, sha1_hash};
use crate::config::{Config, default_config, Password};
use crate::node::BrokerCommand;
use clap::{Parser};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Sender<T> = async_std::channel::Sender<T>;
type Receiver<T> = async_std::channel::Receiver<T>;

mod config;
mod node;
#[derive(Parser, Debug)]
struct Opt {
    /// Verbose mode (module, .)
    #[arg(short = 'v', long = "verbose")]
    verbose: Option<String>,
}

pub(crate) fn main() -> Result<()> {
    let opt = Opt::parse();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Info);
    if let Some(module_names) = opt.verbose {
        for (module, level) in parse_log_verbosity(&module_names, module_path!()) {
            logger = logger.with_module_level(module, level);
        }
    }
    logger.init().unwrap();

    //trace!("trace message");
    //debug!("debug message");
    //info!("info message");
    //warn!("warn message");
    //error!("error message");
    log!(target: "RpcMsg", Level::Debug, "RPC message");
    log!(target: "Acl", Level::Debug, "ACL message");

    let port = 3755;
    let host = "127.0.0.1";
    let address = format!("{}:{}", host, port);
    info!("Listening on: {}", &address);
    task::block_on(accept_loop(address))
}

async fn accept_loop(addr: impl ToSocketAddrs) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    let mut client_id = 0;
    let (broker_sender, broker_receiver) = channel::unbounded();
    let broker = task::spawn(broker_loop(broker_receiver));
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        debug!("Accepting from: {}", stream.peer_addr()?);
        client_id += 1;
        spawn_and_log_error(client_loop(client_id, broker_sender.clone(), stream));
    }
    drop(broker_sender);
    broker.await;
    Ok(())
}
enum LoginResult {
    Ok,
    ClientSocketClosed,
    LoginError,
}
async fn send_error(mut response: RpcMessage, mut writer: &TcpStream, errmsg: &str) -> Result<()> {
    response.set_error(RpcError{ code: RpcErrorCode::MethodCallException, message: errmsg.into()});
    shv::connection::send_message(&mut writer, &response).await
}
async fn send_result(mut response: RpcMessage, mut writer: &TcpStream, result: RpcValue) -> Result<()> {
    response.set_result(result);
    shv::connection::send_message(&mut writer, &response).await
}
async fn client_loop(client_id: i32, broker_writer: Sender<ClientEvent>, stream: TcpStream) -> Result<()> {
    let (socket_reader, mut frame_writer) = (&stream, &stream);
    let (peer_writer, peer_receiver) = channel::unbounded::<PeerEvent>();

    broker_writer.send(ClientEvent::NewClient { client_id, sender: peer_writer }).await.unwrap();

    //let stream_wr = stream.clone();
    let mut brd = BufReader::new(socket_reader);
    let mut frame_reader = shv::connection::FrameReader::new(&mut brd);
    let mut device_options = RpcValue::null();
    let login_result = loop {
        let login_fut = async {
            let frame = match frame_reader.receive_frame().await? {
                None => return crate::Result::Ok(LoginResult::ClientSocketClosed),
                Some(frame) => { frame }
            };
            let rpcmsg = frame.to_rpcmesage()?;
            let resp = rpcmsg.prepare_response()?;
            if rpcmsg.method().unwrap_or("") == "hello" {
                let nonce = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
                let mut result = shv::Map::new();
                result.insert("nonce".into(), RpcValue::from(&nonce));
                send_result(resp, frame_writer, result.into()).await?;

                let frame = match frame_reader.receive_frame().await? {
                    None => return crate::Result::Ok(LoginResult::ClientSocketClosed),
                    Some(frame) => { frame }
                };
                let rpcmsg = frame.to_rpcmesage()?;
                let resp = rpcmsg.prepare_response()?;
                if rpcmsg.method().unwrap_or("") == "login" {
                    let params = rpcmsg.param().ok_or("No login params")?.as_map();
                    let login = params.get("login").ok_or("Invalid login params")?.as_map();
                    let user = login.get("user").ok_or("User login param is missing")?.as_str();
                    let password = login.get("password").ok_or("Password login param is missing")?.as_str();
                    let login_type = login.get("type").map(|v| v.as_str()).unwrap_or("");

                    broker_writer.send(ClientEvent::GetPassword { client_id, user: user.to_string() }).await.unwrap();
                    match peer_receiver.recv().await? {
                        PeerEvent::PasswordSha1(broker_shapass) => {
                            let chkpwd = || {
                                match broker_shapass {
                                    None => {false}
                                    Some(broker_shapass) => {
                                        if login_type == "PLAIN" {
                                            let client_shapass = sha1_hash(password.as_bytes());
                                            client_shapass == broker_shapass
                                        } else {
                                            //info!("nonce: {}", nonce);
                                            //info!("broker password: {}", std::str::from_utf8(&broker_shapass).unwrap());
                                            //info!("client password: {}", password);
                                            let mut data = nonce.as_bytes().to_vec();
                                            data.extend_from_slice(&broker_shapass[..]);
                                            let broker_shapass = sha1_hash(&data);
                                            password.as_bytes() == broker_shapass
                                        }
                                    }
                                }
                            };
                            if chkpwd() {
                                let mut result = shv::Map::new();
                                result.insert("clientId".into(), RpcValue::from(client_id));
                                send_result(resp, frame_writer, result.into()).await?;
                                if let Some(options) = params.get("options") {
                                    if let Some(device) = options.as_map().get("device") {
                                        device_options = device.clone();
                                    }
                                }
                                crate::Result::Ok(LoginResult::Ok)
                            } else {
                                send_error(resp, frame_writer, &format!("Invalid login credentials.")).await?;
                                Ok(LoginResult::LoginError)
                            }
                        }
                        _ => {
                            panic!("Internal error, PeerEvent::PasswordSha1 expected");
                        }
                    }
                } else {
                    send_error(resp, frame_writer, &format!("login message expected.")).await?;
                    Ok(LoginResult::LoginError)
                }
            } else {
                send_error(resp, frame_writer, &format!("hello message expected.")).await?;
                Ok(LoginResult::LoginError)
            }
        };
        match login_fut.await {
            Ok(login_result) => {
                break login_result;
            }
            Err(errmsg) => {
                warn!("{}", errmsg);
            }
        }
    };
    if let LoginResult::Ok = login_result {
        let register_device = ClientEvent::RegisterDevice {
            client_id,
            device_id: device_options.as_map().get("deviceId").map(|v| v.as_str().to_string()),
            mount_point: device_options.as_map().get("mountPoint").map(|v| v.as_str().to_string()),
        };
        broker_writer.send(register_device).await.unwrap();
        loop {
            select! {
                frame = frame_reader.receive_frame().fuse() => match frame {
                    Ok(frame) => {
                        match frame {
                            None => {
                                debug!("Client socket closed");
                                break;
                            }
                            Some(frame) => {
                                // log!(target: "RpcMsg", Level::Debug, "----> Recv frame, client id: {}", client_id);
                                broker_writer.send(ClientEvent::Frame { client_id, frame }).await.unwrap();
                            }
                        }
                    }
                    Err(e) => {
                        error!("Read socket error: {}", &e);
                        break;
                    }
                },
                event = peer_receiver.recv().fuse() => match event {
                    Err(e) => {
                        debug!("Peer channel closed: {}", &e);
                        break;
                    }
                    Ok(event) => {
                        match event {
                            PeerEvent::PasswordSha1(_) => {
                                panic!("PasswordSha1 cannot be received here")
                            }
                            PeerEvent::DisconnectByBroker => {
                                info!("Disconnected by broker, client ID: {client_id}");
                                break;
                            }
                            PeerEvent::Frame(frame) => {
                                // log!(target: "RpcMsg", Level::Debug, "<---- Send frame, client id: {}", client_id);
                                shv::connection::send_frame(&mut frame_writer, frame).await?;
                            }
                            PeerEvent::Message(rpcmsg) => {
                                // log!(target: "RpcMsg", Level::Debug, "<---- Send message, client id: {}", client_id);
                                shv::connection::send_message(&mut frame_writer, &rpcmsg).await?;
                            },
                        }
                    }
                }
            }
        }
    }
    broker_writer.send(ClientEvent::ClientGone { client_id }).await.unwrap();
    info!("Client loop exit, client id: {}", client_id);
    Ok(())
}

#[derive(Debug)]
enum ClientEvent {
    GetPassword {
        client_id: CliId,
        user: String,
    },
    NewClient {
        client_id: CliId,
        sender: Sender<PeerEvent>,
    },
    RegisterDevice {
        client_id: CliId,
        device_id: Option<String>,
        mount_point: Option<String>,
    },
    Frame {
        client_id: CliId,
        frame: RpcFrame,
    },
    ClientGone {
        client_id: CliId,
    },
}

#[derive(Debug)]
enum PeerEvent {
    PasswordSha1(Option<Vec<u8>>),
    Frame(RpcFrame),
    Message(RpcMessage),
    DisconnectByBroker,
}
#[derive(Debug)]
struct Peer {
    sender: Sender<PeerEvent>,
    user: String,
    mount_point: Option<String>,
    subscriptions: Vec<Subscription>,
}
impl Peer {
    fn is_signal_subscribed(&self, path: &str, method: &str) -> bool {
        for subs in self.subscriptions.iter() {
            if subs.match_shv_method(path, method) {
                return true;
            }
        }
        false
    }
}
struct Device {
    client_id: CliId,
}
type Node<K> = Box<dyn ShvNode<K> + Send + Sync>;
enum Mount<K> {
    Peer(Device),
    Node(Node<K>),
}
struct ParsedAccessRule {
    path_method: Subscription,
    grant: String,
}
impl ParsedAccessRule {
    pub fn new(path: &str, method: &str, grant: &str) -> shv::Result<Self> {
       let method = if method.is_empty() { "?*" } else { method };
       let path = if path.is_empty() { "**" } else { path };
       match Pattern::new(method) {
           Ok(method) => {
               match Pattern::new(path) {
                   Ok(path) => {
                       Ok(Self {
                           path_method: Subscription { paths: path, methods: method },
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
struct Broker {
    peers: HashMap<CliId, Peer>,
    mounts: BTreeMap<String, Mount<crate::node::BrokerCommand>>,
    config: Config,
    role_access: HashMap<String, Vec<ParsedAccessRule>>,
}
fn find_mount<'a, 'b, K>(mounts: &'a mut BTreeMap<String, Mount<K>>, shv_path: &'b str) -> Option<(&'a mut Mount<K>, &'b str)> {
    if let Some((mount_dir, node_dir)) = find_longest_prefix(mounts, shv_path) {
        Some((mounts.get_mut(mount_dir).unwrap(), node_dir))
    } else {
        None
    }
}

impl Broker {
    fn new(config: Config) -> Self {
        let mut role_access: HashMap<String, Vec<ParsedAccessRule>> = Default::default();
        for (name, role) in &config.roles {
            let mut list: Vec<ParsedAccessRule> = Default::default();
            for rule in &role.access {
                match ParsedAccessRule::new(&rule.patterns, &rule.methods, &rule.grant) {
                    Ok(rule) => {
                        list.push(rule);
                    }
                    Err(err) => {
                        error!("Parse access rule error: {}", err);
                    }
                }
            }
            if !list.is_empty() {
                role_access.insert(name.clone(), list);
            }
        }
        Self {
            peers: Default::default(),
            mounts: Default::default(),
            config,
            role_access,
        }
    }
    pub fn sha_password(&self, user: &str) -> Option<Vec<u8>> {
        match self.config.users.get(user) {
            None => None,
            Some(user) => {
                match &user.password {
                    Password::Plain(password) => {
                        Some(sha1_hash(password.as_bytes()))
                    }
                    Password::Sha1(password) => {
                        Some(password.as_bytes().into())
                    }
                }
            }
        }
    }
    pub fn mount_device(&mut self, client_id: i32, device_id: Option<String>, mount_point: Option<String>) {
        if let Some(mount_point) = loop {
            if let Some(ref mount_point) = mount_point {
                if mount_point.starts_with("test/") {
                    info!("Client id: {} mounted on path: '{}'", client_id, &mount_point);
                    break Some(mount_point.clone());
                }
            }
            if let Some(device_id) = device_id {
                let mount_point = "test/".to_owned() + &device_id;
                info!("Client id: {}, device id: {} mounted on path: '{}'", client_id, device_id, &mount_point);
                break Some(mount_point);
            }
            break None;
        } {
            self.mounts.insert(mount_point.clone(), Mount::Peer(Device { client_id }));
            if let Some(peer) = self.peers.get_mut(&client_id) {
                peer.mount_point = Some(mount_point.clone());
            }
        };
        let client_path = format!(".app/broker/client/{}", client_id);
        self.mounts.insert(client_path, Mount::Peer(Device { client_id }));
    }
    pub fn client_id_to_mount_point(&self, client_id: CliId) -> Option<String> {
        match self.peers.get(&client_id) {
            None => None,
            Some(peer) => peer.mount_point.clone()
        }
    }
    fn flatten_roles_helper<'a>(&'a self, flatten_roles: Vec<&'a str>) -> Vec<&'a str> {
        fn add_unique_role<'a, 'b>(roles: &'b mut Vec<&'a str>, role: &'a str) {
            for r in roles.iter() {
                if r == &role {
                    return;
                }
            }
            roles.push(role);
        }
        let mut new_roles = flatten_roles.clone();
        for role in flatten_roles {
            if let Some(cfg_role) = self.config.roles.get(role) {
                for r2 in &cfg_role.roles {
                    add_unique_role(&mut new_roles, r2);
                }
            }
        }
        new_roles
    }
    fn flatten_roles(&self, user: &str) -> Option<Vec<&str>> {
        if let Some(user) = self.config.users.get(user) {
            let flatten_roles: Vec<&str> = user.roles.iter().map(|s| s.as_str()).collect();
            Some(self.flatten_roles_helper(flatten_roles))
        } else {
            None
        }
    }
    fn grant_for_request(&self, client_id: CliId, frame: &RpcFrame) -> Option<String> {
        log!(target: "Acl", Level::Debug, "======================= grant_for_request {}", &frame);
        let shv_path = frame.shv_path().unwrap_or_default();
        let method = frame.method().unwrap_or("");
        if method.is_empty() {
            None
        } else {
            if let Some(peer) = self.peers.get(&client_id) {
                if let Some(flatten_roles) = self.flatten_roles(&peer.user) {
                    log!(target: "Acl", Level::Debug, "user: {}, flatten roles: {:?}", &peer.user, flatten_roles);
                    for role_name in flatten_roles {
                        if let Some(rules) = self.role_access.get(role_name) {
                            log!(target: "Acl", Level::Debug, "----------- access for role: {}", role_name);
                            for rule in rules {
                                log!(target: "Acl", Level::Debug, "\trule: {}", rule.path_method);
                                if rule.path_method.match_shv_method(shv_path, method) {
                                    log!(target: "Acl", Level::Debug, "\t\t HIT");
                                    return Some(rule.grant.clone());
                                }
                            }
                        }
                    }
                }
            }
            None
        }
    }
    fn peer_to_info(client_id: CliId, peer: &Peer) -> rpcvalue::Map {
        let subs: List = peer.subscriptions.iter().map(|subs| subs.to_rpcvalue()).collect();
        rpcvalue::Map::from([
            ("clientId".to_string(), client_id.into()),
            ("userName".to_string(), RpcValue::from(&peer.user)),
            ("mountPoint".to_string(), RpcValue::from(peer.mount_point.clone().unwrap_or_default())),
            ("subscriptions".to_string(), subs.into()),
        ]
        )
    }
    pub fn client_info(&mut self, client_id: CliId) -> Option<rpcvalue::Map> {
        match self.peers.get(&client_id) {
            None => { None }
            Some(peer) => {
                Some(Broker::peer_to_info(client_id, peer))
            }
        }
    }
    async fn disconnect_client(&mut self, client_id: CliId) {
        match self.peers.get(&client_id) {
            None => {  }
            Some(peer) => {
                let _ = peer.sender.send(PeerEvent::DisconnectByBroker).await;
            }
        }
    }
    pub fn mounted_client_info(&mut self, mount_point: &str) -> Option<rpcvalue::Map> {
        for (client_id, peer) in &self.peers {
            if let Some(mount_point1) = &peer.mount_point {
                if mount_point1 == mount_point {
                    return Some(Broker::peer_to_info(*client_id, peer))
                }
            }
        }
        None
    }
    pub fn clients(&self) -> rpcvalue::List {
        let ids: rpcvalue::List = self.peers.iter().map(|(id, _)| RpcValue::from(*id) ).collect();
        ids
    }
    pub fn mounts(&self) -> rpcvalue::List {
       self.peers.iter().filter(|(_id, peer)| peer.mount_point.is_some()).map(|(_id, peer)| {
            RpcValue::from(peer.mount_point.clone().unwrap())
        } ).collect()
    }
    pub fn subscribe(&mut self, client_id: CliId, subscription: Subscription) {
        log!(target: "Subscr", Level::Debug, "New subscription for client id: {} - {}", client_id, &subscription);
        if let Some(peer) = self.peers.get_mut(&client_id) {
            peer.subscriptions.push(subscription);
        }
    }
    pub fn unsubscribe(&mut self, client_id: CliId, subscription: &Subscription) -> bool {
        if let Some(peer) = self.peers.get_mut(&client_id) {
            let cnt = peer.subscriptions.len();
            peer.subscriptions.retain(|subscr| subscr != subscription);
            cnt != peer.subscriptions.len()
        } else {
            false
        }
    }
}
async fn broker_loop(events: Receiver<ClientEvent>) {
    let mut broker = Broker::new(default_config());

    //let yaml = serde_yaml::to_string(&broker.config).unwrap();
    //println!("yaml = \n{}", yaml);

    broker.mounts.insert(".app".into(), Mount::Node(Box::new(shv::shvnode::AppNode { app_name: "shvbroker", ..Default::default() })));
    broker.mounts.insert(".app/broker".into(), Mount::Node(Box::new(node::AppBrokerNode {  })));
    broker.mounts.insert(".app/broker/currentClient".into(), Mount::Node(Box::new(node::AppBrokerCurrentClientNode {  })));
    loop {
        match events.recv().await {
            Err(e) => {
                info!("Client channel closed: {}", &e);
                break;
            }
            Ok(event) => {
                match event {
                    ClientEvent::Frame { client_id, mut frame} => {
                        if frame.is_request() {
                            type Command = RequestCommand<BrokerCommand>;
                            let shv_path = frame.shv_path().unwrap_or_default().to_string();
                            let response_meta= RpcFrame::prepare_response_meta(&frame.meta);
                            let command = match broker.grant_for_request(client_id, &frame) {
                                None => {
                                    Command::Error(RpcError::new(RpcErrorCode::PermissionDenied, &format!("Cannot resolve call method grant for current user.")))
                                }
                                Some(grant) => {
                                    let local_result = process_local_dir_ls(&broker.mounts, &frame);
                                    if local_result.is_some() {
                                        local_result.unwrap()
                                    } else {
                                        if let Some((mount, node_path)) = find_mount(&mut broker.mounts, &shv_path) {
                                            match mount {
                                                Mount::Peer(device) => {
                                                    let device_client_id = device.client_id;
                                                    frame.push_caller_id(client_id);
                                                    frame.set_shvpath(node_path);
                                                    frame.set_access(&grant);
                                                    let _ = broker.peers.get(&device_client_id).expect("client ID must exist").sender.send(PeerEvent::Frame(frame)).await;
                                                    Command::Noop
                                                }
                                                Mount::Node(node) => {
                                                    if let Ok(rpcmsg) = frame.to_rpcmesage() {
                                                        let mut rpcmsg2 = rpcmsg;
                                                        rpcmsg2.set_shvpath(node_path);
                                                        rpcmsg2.set_access(&grant);
                                                        if node.is_request_granted(&rpcmsg2) {
                                                            node.process_request(&rpcmsg2)
                                                        } else {
                                                            let err = RpcError::new(RpcErrorCode::PermissionDenied, &format!("Request to call {}:{} is not granted.", rpcmsg2.shv_path().unwrap_or_default(), rpcmsg2.method().unwrap_or_default()));
                                                            Command::Error(err)
                                                        }
                                                    } else {
                                                        Command::Error(RpcError::new(RpcErrorCode::InvalidRequest, &format!("Cannot convert RPC frame to Rpc message")))
                                                    }
                                                }
                                            }
                                        } else {
                                            let method = frame.method().unwrap_or_default();
                                            Command::Error(RpcError::new(RpcErrorCode::MethodNotFound, &format!("Invalid shv path {}:{}()", shv_path, method)))
                                        }
                                    }
                                }
                            };
                            let command = if let Command::Custom(cc) = command {
                                match cc {
                                    BrokerCommand::ClientInfo(client_id) => {
                                        match broker.client_info(client_id) {
                                            None => { Command::Result(().into()) }
                                            Some(info) => { Command::Result(info.into()) }
                                        }
                                    }
                                    BrokerCommand::MountedClientInfo(mount_point) => {
                                        match broker.mounted_client_info(&mount_point) {
                                            None => { Command::Result(().into()) }
                                            Some(info) => { Command::Result(info.into()) }
                                        }
                                    }
                                    BrokerCommand::Clients => {
                                        Command::Result(broker.clients().into())
                                    }
                                    BrokerCommand::Mounts => {
                                        Command::Result(broker.mounts().into())
                                    }
                                    BrokerCommand::DisconnectClient(client_id) => {
                                        broker.disconnect_client(client_id).await;
                                        Command::Result(().into())
                                    }
                                    BrokerCommand::CurrentClientInfo => {
                                        match broker.client_info(client_id) {
                                            None => { Command::Result(().into()) }
                                            Some(info) => { Command::Result(info.into()) }
                                        }
                                    }
                                    BrokerCommand::Subscribe(subscr) => {
                                        broker.subscribe(client_id, subscr);
                                        Command::Result(().into())
                                    }
                                    BrokerCommand::Unsubscribe(subscr) => {
                                        let ok = broker.unsubscribe(client_id, &subscr);
                                        Command::Result(ok.into())
                                    }
                                }
                            } else {
                                command
                            };
                            match command {
                                RequestCommand::Noop => {}
                                command => {
                                    if let Ok(meta) = response_meta {
                                        let peer = broker.peers.get(&client_id).unwrap();
                                        let command = if let Command::PropertyChanged(value) = command {
                                            let sig = RpcMessage::new_signal(&shv_path, SIG_CHNG, Some(value));
                                            peer.sender.send(PeerEvent::Message(sig)).await.unwrap();
                                            Command::Result(().into())
                                        } else {
                                            command
                                        };
                                        let mut resp = RpcMessage::from_meta(meta);
                                        match command {
                                            RequestCommand::Result(value) => {
                                                resp.set_result(value);
                                            }
                                            RequestCommand::Error(error) => {
                                                resp.set_error(error);
                                            }
                                            _ => {
                                            }
                                        }
                                        if resp.is_success() || resp.is_error() {
                                            if let Err(error) = peer.sender.send(PeerEvent::Message(resp)).await {
                                                error!("Error writing to peer channel: {error}");
                                            }
                                        }
                                    } else {
                                        warn!("Invalid request frame received.");
                                    }
                                }
                            }
                        } else if frame.is_response() {
                            if let Some(client_id) = frame.pop_caller_id() {
                                let peer = broker.peers.get(&client_id).unwrap();
                                peer.sender.send(PeerEvent::Frame(frame)).await.unwrap();
                            }
                        } else if frame.is_signal() {
                            if let Some(mount_point) = broker.client_id_to_mount_point(client_id) {
                                let new_path = util::join_path(&mount_point, frame.shv_path().unwrap_or_default());
                                for (cli_id, peer) in broker.peers.iter() {
                                    if &client_id != cli_id {
                                        if peer.is_signal_subscribed(&new_path, frame.method().unwrap_or_default()) {
                                            let mut frame = frame.clone();
                                            frame.set_shvpath(&new_path);
                                            peer.sender.send(PeerEvent::Frame(frame)).await.unwrap();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ClientEvent::NewClient { client_id, sender } => match broker.peers.entry(client_id) {
                        Entry::Occupied(..) => (),
                        Entry::Vacant(entry) => {
                            entry.insert(Peer {
                                sender,
                                user: "".to_string(),
                                mount_point: None,
                                subscriptions: vec![],
                            });
                        }
                    },
                    ClientEvent::RegisterDevice { client_id, device_id, mount_point } => {
                        broker.mount_device(client_id, device_id, mount_point);
                    },
                    ClientEvent::ClientGone { client_id } => {
                        broker.peers.remove(&client_id);
                        info!("Client id: {} disconnected.", client_id);
                        if let Some(path) = broker.client_id_to_mount_point(client_id) {
                            info!("Unmounting path: '{}'", path);
                            broker.mounts.remove(&path);
                        }
                        let client_path = format!(".app/broker/client/{}", client_id);
                        broker.mounts.remove(&client_path);
                    }
                    ClientEvent::GetPassword { client_id, user } => {
                        let shapwd = broker.sha_password(&user);
                        let peer = broker.peers.get_mut(&client_id).unwrap();
                        peer.user = user.clone();
                        peer.sender.send(PeerEvent::PasswordSha1(shapwd)).await.unwrap();
                    }
                }
            }
        }
    }
    //drop(peers);
}

fn spawn_and_log_error<F>(fut: F) -> task::JoinHandle<()>
    where
        F: Future<Output = Result<()>> + Send + 'static,
{
    task::spawn(async move {
        if let Err(e) = fut.await {
            eprintln!("{}", e)
        }
    })
}
