use std::{
    collections::hash_map::{Entry, HashMap},
};
use std::collections::BTreeMap;
use futures::{select, FutureExt, StreamExt};
use async_std::{channel, io::BufReader, net::{TcpListener, TcpStream, ToSocketAddrs}, prelude::*, task};
use rand::distributions::{Alphanumeric, DistString};
use log::*;
use structopt::StructOpt;
use shv::rpcframe::RpcFrame;
use shv::{RpcMessage, RpcValue};
use shv::rpcmessage::{CliId, RpcError, RpcErrorCode};
use shv::RpcMessageMetaTags;
use simple_logger::SimpleLogger;
use shv::shvnode::{ShvNode};

mod br;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Sender<T> = async_std::channel::Sender<T>;
type Receiver<T> = async_std::channel::Receiver<T>;

#[derive(Debug)]
enum Void {}

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
async fn send_error(request: &RpcFrame, mut writer: &TcpStream, errmsg: &str) -> Result<()> {
    let mut resp = RpcMessage::prepare_response_from_meta(&request.meta)?;
    resp.set_error(RpcError{ code: RpcErrorCode::MethodCallException, message: errmsg.into()});
    shv::connection::send_message(&mut writer, &resp).await
}
async fn send_result(request: &RpcFrame, mut writer: &TcpStream, result: RpcValue) -> Result<()> {
    let mut resp = RpcMessage::prepare_response_from_meta(&request.meta)?;
    resp.set_result(result);
    shv::connection::send_message(&mut writer, &resp).await
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
                send_result(&frame, frame_writer, result.into()).await?;

                let frame = match frame_reader.receive_frame().await? {
                    None => return crate::Result::Ok(LoginResult::ClientSocketClosed),
                    Some(frame) => { frame }
                };
                let rpcmsg = frame.to_rpcmesage()?;
                let resp = rpcmsg.prepare_response()?;
                if rpcmsg.method().unwrap_or("") == "login" {
                    let params = rpcmsg.params().ok_or("No login params")?.as_map();
                    let login = params.get("login").ok_or("Invalid login params")?.as_map();
                    let user = login.get("user").ok_or("User login param is missing")?.as_str();
                    let password = login.get("password").ok_or("Password login param is missing")?.as_str();
                    let login_type = login.get("type").map(|v| v.as_str()).unwrap_or("");

                    broker_writer.send(ClientEvent::GetPassword { client_id, user: user.to_string() }).await.unwrap();
                    match peer_receiver.recv().await? {
                        PeerEvent::PasswordSha1(broker_shapass) => {
                            let chkpwd = || {
                                if login_type == "PLAIN" {
                                    let client_shapass = shv::connection::sha1_hash(password.as_bytes());
                                    client_shapass == broker_shapass
                                } else {
                                    //info!("nonce: {}", nonce);
                                    //info!("broker password: {}", std::str::from_utf8(&broker_shapass).unwrap());
                                    //info!("client password: {}", password);
                                    let mut data = nonce.as_bytes().to_vec();
                                    data.extend_from_slice(&broker_shapass[..]);
                                    let broker_shapass = shv::connection::sha1_hash(&data);
                                    password.as_bytes() == broker_shapass
                                }
                            };
                            if chkpwd() {
                                let mut result = shv::Map::new();
                                result.insert("clientId".into(), RpcValue::from(client_id));
                                send_result(&frame, frame_writer, result.into()).await?;
                                if let Some(options) = params.get("options") {
                                    if let Some(device) = options.as_map().get("device") {
                                        device_options = device.clone();
                                    }
                                }
                                crate::Result::Ok(LoginResult::Ok)
                            } else {
                                send_error(&frame, frame_writer, &format!("Invalid login credentials received.")).await?;
                                Ok(LoginResult::LoginError)
                            }
                        }
                        _ => {
                            panic!("Internal error, PeerEvent::PasswordSha1 expected");
                        }
                    }
                } else {
                    send_error(&frame, frame_writer, &format!("login message expected.")).await?;
                    Ok(LoginResult::LoginError)
                }
            } else {
                send_error(&frame, frame_writer, &format!("hello message expected.")).await?;
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
        let register_device = ClientEvent::RegiterDevice {
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
                                shv::connection::send_frame(&mut frame_writer, frame).await?;
                            }
                            PeerEvent::Message(rpcmsg) => {
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
    RegiterDevice {
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
    PasswordSha1(Vec<u8>),
    Frame(RpcFrame),
    Message(RpcMessage),
    //FatalError(String),
}
#[derive(Debug)]
struct Peer {
    sender: Sender<PeerEvent>,
}
struct Device {
    client_id: CliId,
}
type Node = Box<dyn ShvNode + Send + Sync>;
enum Mount {
    Device(Device),
    Node(Node),
}
struct Broker {
    peers: HashMap<CliId, Peer>,
    mounts: BTreeMap<String, Mount>,
}

impl Broker {
    pub fn ls(&self, path: &str) -> Option<Vec<String>> {
        //let parent_dir = path.to_string();
        let mut dirs: Vec<String> = Vec::new();
        let mut dir_exists = false;
        for (key, _) in self.mounts.range(path.to_string()..) {
            if key.starts_with(path) {
                dir_exists = true;
                if path.is_empty()
                    || key.len() == path.len()
                    || key.as_bytes()[path.len()] == ('/' as u8)
                {
                    if key.len() > path.len() {
                        let dir_rest_start = if path.is_empty() {
                            0
                        } else {
                            path.len() + 1
                        };
                        let mut updirs = key[dir_rest_start..].split('/');
                        if let Some(dir) = updirs.next() {
                            dirs.push(dir.to_string());
                        }
                    }
                }
            } else {
                break;
            }
        }
        if dir_exists {
            Some(dirs)
        } else {
            None
        }
    }
    fn find_longest_prefix<'a, 'b, V>(map: &'a BTreeMap<String, V>, shv_path: &'b str) -> Option<(&'b str, &'b str)> {
        let mut path = &shv_path[..];
        let mut rest = "";
        loop {
            if map.contains_key(path) {
                return Some((path, rest))
            }
            if path.is_empty() {
                break;
            }
            if let Some(slash_ix) = path.rfind('/') {
                path = &shv_path[..slash_ix];
                rest = &shv_path[(slash_ix + 1)..];
            } else {
                path = "";
                rest = shv_path;
            };
        }
        None
    }
    pub fn find_mount<'a, 'b>(&'a mut self, shv_path: &'b str) -> Option<(&'a mut Mount, &'b str)> {
        if let Some((mount_dir, node_dir)) = Self::find_longest_prefix(&self.mounts, shv_path) {
            Some((self.mounts.get_mut(mount_dir).unwrap(), node_dir))
        } else {
            None
        }
    }
    pub fn sha_password(&self, user: &str) -> Vec<u8> {
        shv::connection::sha1_hash(user.as_bytes())
    }
    pub fn mount_device(&mut self, client_id: i32, device_id: Option<String>, mount_point: Option<String>) {
        if let Some(mount_point) = mount_point {
            if mount_point.starts_with("test/") {
                self.mounts.insert(mount_point, Mount::Device(Device { client_id }));
                return;
            }
        }
        if let Some(device_id) = device_id {
            let mount_point = "test/".to_owned() + &device_id;
            if mount_point.starts_with("test/") {
                self.mounts.insert(mount_point, Mount::Device(Device { client_id }));
                return;
            }
        }
    }
}
async fn broker_loop(events: Receiver<ClientEvent>) {
    let mut broker = Broker {
        peers: HashMap::new(),
        mounts: BTreeMap::new(),
    };
    broker.mounts.insert(".app".into(), Mount::Node(Box::new(br::nodes::AppNode{})));
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
                            let shv_path = frame.shv_path().unwrap_or("").to_owned();
                            let response_meta= RpcFrame::prepare_response_meta(&frame.meta);
                            let result: Option<std::result::Result<RpcValue, String>> = if let Some((mount, node_path)) = broker.find_mount(&shv_path) {
                                match mount {
                                    Mount::Device(device) => {
                                        let client_id = device.client_id;
                                        frame.push_caller_id(client_id);
                                        frame.set_shvpath(node_path);
                                        let _ = broker.peers.get(&client_id).unwrap().sender.send(PeerEvent::Frame(frame)).await;
                                        None
                                    }
                                    Mount::Node(node) => {
                                        if let Ok(rpcmsg) = frame.to_rpcmesage() {
                                            let mut rpcmsg2 = rpcmsg;
                                            rpcmsg2.set_shvpath(node_path);
                                            match node.process_request(&rpcmsg2) {
                                                Ok(res) => {
                                                    match res {
                                                        None => {None}
                                                        Some(res) => {Some(Ok(res))}
                                                    }
                                                }
                                                Err(err) => { Some(Err(err.to_string())) }
                                            }
                                        } else {
                                            Some(Err(format!("Invalid shv request")))
                                        }
                                    }
                                }
                            } else {
                                match frame.method() {
                                    Some("dir") => {
                                        Some(Err(format!("Dir on path: {} NIY", shv_path)))
                                    }
                                    Some("ls") => {
                                        match broker.ls(&shv_path) {
                                            None => {
                                                Some(Err(format!("Invalid shv path: {}", shv_path)))
                                            }
                                            Some(dirs) => {
                                                let result: Vec<RpcValue> = dirs.into_iter().map(|s| RpcValue::from(s)).collect();
                                                Some(Ok(RpcValue::from(result)))
                                            }
                                        }
                                    }
                                    Some(method) => {
                                        Some(Err(format!("Method {}:{}() is not implemented", shv_path, method)))
                                    }
                                    None => {
                                        Some(Err(format!("Empty method on path: {}", shv_path)))
                                    }
                                }
                            };
                            if let Some(result) = result {
                                if let Ok(meta) = response_meta {
                                    let mut resp = RpcMessage::from_meta(meta);
                                    match result {
                                        Ok(value) => {
                                            resp.set_result(value);
                                        }
                                        Err(errmsg) => {
                                            resp.set_error(RpcError::new(RpcErrorCode::MethodCallException, &errmsg));
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
                        }
                    }
                    ClientEvent::NewClient { client_id, sender } => match broker.peers.entry(client_id) {
                        Entry::Occupied(..) => (),
                        Entry::Vacant(entry) => {
                            entry.insert(Peer {
                                sender,
                            });
                        }
                    },
                    ClientEvent::RegiterDevice { client_id, device_id, mount_point } => {
                        broker.mount_device(client_id, device_id, mount_point);
                    },
                    ClientEvent::ClientGone { client_id } => {
                        assert!(broker.peers.remove(&client_id).is_some());
                    }
                    ClientEvent::GetPassword { client_id, user } => {
                        let shapwd = broker.sha_password(&user);
                        let peer = broker.peers.get(&client_id).unwrap();
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
