use std::{
    collections::hash_map::{Entry, HashMap},
};
use std::collections::BTreeMap;
use futures::{select, FutureExt, StreamExt};
use async_std::{channel, io::BufReader, net::{TcpListener, TcpStream, ToSocketAddrs}, prelude::*, task};
use rand::distributions::{Alphanumeric, DistString};
use log::*;
use structopt::StructOpt;
use shv::rpcframe::{RpcFrame};
use shv::{RpcMessage, RpcValue, util};
use shv::rpcmessage::{CliId, RpcError, RpcErrorCode};
use shv::RpcMessageMetaTags;
use simple_logger::SimpleLogger;
use shv::shvnode::{find_longest_prefix, dir_ls, ShvNode, ProcessRequestResult};
use shv::util::{glob_match, sha1_hash};
use crate::config::{Config, default_config, Password};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Sender<T> = async_std::channel::Sender<T>;
type Receiver<T> = async_std::channel::Receiver<T>;

mod config;
#[derive(StructOpt, Debug)]
#[structopt()]
struct Opt {
    /// Verbose mode (module, .)
    #[structopt(short = "v", long = "verbose")]
    verbose: Option<String>,
}

pub(crate) fn main() -> Result<()> {
    let opt = Opt::from_args();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Info);
    if let Some(module_names) = opt.verbose {
        for module_name in module_names.split(',') {
            let module_name = if module_name == "." {
                module_path!().to_string()
            } else {
                module_name.to_string()
            };
            logger = logger.with_module_level(&module_name, LevelFilter::Trace);
        }
    }
    logger.init().unwrap();

    trace!("trace message");
    debug!("debug message");
    info!("info message");
    warn!("warn message");
    error!("error message");
    log!(target: "RpcMsg", Level::Debug, "RPC message");

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
                            //PeerEvent::FatalError(errmsg) => {
                            //    error!("Fatal client error: {}", errmsg);
                            //    break;
                            //}
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
    //FatalError(String),
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
            if subs.match_signal(path, method) {
                return true;
            }
        }
        false
    }
}
struct Device {
    client_id: CliId,
}
type Node = Box<dyn ShvNode + Send + Sync>;
enum Mount {
    Device(Device),
    Node(Node),
}
#[derive(Debug)]
struct PathPattern(String);
impl PathPattern {
    fn match_glob(&self, _: &str) -> bool {
        return true
    }
}
#[derive(Debug)]
struct MethodPattern(String);
impl MethodPattern {
    fn match_glob(&self, _: &str) -> bool {
        return true
    }
}
#[derive(Debug)]
struct Subscription {
    path: PathPattern,
    method: MethodPattern,
}
impl Subscription {
    fn match_signal(&self, path: &str, method: &str) -> bool {
        self.path.match_glob(path) && self.method.match_glob(method)
    }
}
struct Broker {
    peers: HashMap<CliId, Peer>,
    mounts: BTreeMap<String, Mount>,
    config: Config,
}

impl Broker {
    pub fn find_mount<'a, 'b>(&'a mut self, shv_path: &'b str) -> Option<(&'a mut Mount, &'b str)> {
        if let Some((mount_dir, node_dir)) = find_longest_prefix(&self.mounts, shv_path) {
            Some((self.mounts.get_mut(mount_dir).unwrap(), node_dir))
        } else {
            None
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
        if let Some(mount_path) = loop {
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
            if let Some(peer) = self.peers.get_mut(&client_id) {
                peer.mount_point = Some(mount_path.clone());
            }
            self.mounts.insert(mount_path, Mount::Device(Device { client_id }));
        }
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
        log!(target: "Acl", Level::Info, "======================= grant_for_request {}", &frame);
        let shv_path = frame.shv_path_or_empty();
        let method = frame.method().unwrap_or("");
        if method.is_empty() {
            None
        } else {
            if let Some(peer) = self.peers.get(&client_id) {
                if let Some(flatten_roles) = self.flatten_roles(&peer.user) {
                    log!(target: "Acl", Level::Info, "user: {}, flatten roles: {:?}", &peer.user, flatten_roles);
                    for role_name in flatten_roles {
                        if let Some(role) = self.config.roles.get(role_name) {
                            log!(target: "Acl", Level::Info, "----------- access for role: {}", role_name);
                            for rule in &role.access {
                                log!(target: "Acl", Level::Info, "\trule: {}:{}", rule.path, rule.method);
                                if rule.method.is_empty() || &rule.method == method {
                                    if glob_match(shv_path, &rule.path) {
                                        log!(target: "Acl", Level::Info, "\t\t HIT");
                                        return Some(rule.grant.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }
    }
}
async fn broker_loop(events: Receiver<ClientEvent>) {
    let mut broker = Broker {
        peers: HashMap::new(),
        mounts: BTreeMap::new(),
        config: default_config(),
    };
    broker.mounts.insert(".app".into(), Mount::Node(Box::new(shv::shvnode::AppNode { app_name: "shvbroker", ..Default::default() })));
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
                            let shv_path = frame.shv_path_or_empty().to_string();
                            let response_meta= RpcFrame::prepare_response_meta(&frame.meta);
                            let result: Option<ProcessRequestResult> = match broker.grant_for_request(client_id, &frame) {
                                None => {
                                    Some(Err(RpcError::new(RpcErrorCode::PermissionDenied, &format!("Cannot resolve call method grant for current user."))))
                                }
                                Some(grant) => {
                                    if let Some((mount, node_path)) = broker.find_mount(&shv_path) {
                                        match mount {
                                            Mount::Device(device) => {
                                                let device_client_id = device.client_id;
                                                frame.push_caller_id(client_id);
                                                frame.set_shvpath(node_path);
                                                frame.set_access(&grant);
                                                let _ = broker.peers.get(&device_client_id).unwrap().sender.send(PeerEvent::Frame(frame)).await;
                                                None
                                            }
                                            Mount::Node(node) => {
                                                if let Ok(rpcmsg) = frame.to_rpcmesage() {
                                                    let mut rpcmsg2 = rpcmsg;
                                                    rpcmsg2.set_shvpath(node_path);
                                                    rpcmsg2.set_access(&grant);
                                                    Some(node.process_request(&rpcmsg2))
                                                } else {
                                                    Some(Err(RpcError::new(RpcErrorCode::InvalidRequest, &format!("Cannot convert RPC frame to Rpc message"))))
                                                }
                                            }
                                        }
                                    } else {
                                        if let Ok(rpcmsg) = frame.to_rpcmesage() {
                                            Some(dir_ls(&broker.mounts, rpcmsg))
                                        } else {
                                            Some(Err(RpcError::new(RpcErrorCode::InvalidRequest, &format!("Cannot convert RPC frame to Rpc message"))))
                                        }
                                    }
                                }
                            };
                            if let Some(result) = result {
                                if let Ok(meta) = response_meta {
                                    let mut resp = RpcMessage::from_meta(meta);
                                    match result {
                                        Ok((value, _signal)) => {
                                            resp.set_result(value);
                                        }
                                        Err(err) => {
                                            resp.set_error(err);
                                        }
                                    }
                                    let peer = broker.peers.get(&client_id).unwrap();
                                    peer.sender.send(PeerEvent::Message(resp)).await.unwrap();
                                }
                            }
                        } else if frame.is_response() {
                            if let Some(client_id) = frame.pop_caller_id() {
                                let peer = broker.peers.get(&client_id).unwrap();
                                peer.sender.send(PeerEvent::Frame(frame)).await.unwrap();
                            }
                        } else if frame.is_signal() {
                            // send signal to all clients until subscribe method will be implemented
                            if let Some(mount_point) = broker.client_id_to_mount_point(client_id) {
                                let new_path = util::join_path(&mount_point, frame.shv_path_or_empty());
                                for (cli_id, peer) in broker.peers.iter() {
                                    if &client_id != cli_id {
                                        if peer.is_signal_subscribed(frame.shv_path_or_empty(), frame.method().unwrap_or("")) {
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
                        if let Some(path) = broker.client_id_to_mount_point(client_id) {
                            info!("Client id: {} disconnected, unmounting path: '{}'", client_id, path);
                            broker.mounts.remove(&path);
                        }
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
