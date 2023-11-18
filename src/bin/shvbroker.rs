use std::{
    collections::hash_map::{Entry, HashMap},
};
use std::collections::BTreeMap;
use futures::{select, FutureExt, StreamExt};
use async_std::{channel, io::BufReader, net::{TcpListener, TcpStream, ToSocketAddrs}, prelude::*, task};
use tryvial::try_block;
use rand::distributions::{Alphanumeric, DistString};
use log::*;
use structopt::StructOpt;
use shv::rpcframe::RpcFrame;
use shv::{RpcMessage, RpcValue};
use shv::{Map};
use shv::rpcmessage::{RpcError, RpcErrorCode};
use shv::RpcMessageMetaTags;
use simple_logger::SimpleLogger;
use shv::connection::FrameReader;
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

async fn client_loop(client_id: i32, broker: Sender<ClientEvent>, stream: TcpStream) -> Result<()> {
    let (socket_reader, mut socket_writer) = (&stream, &stream);
    let (peer_sender, peer_receiver) = channel::unbounded::<PeerEvent>();
    broker
        .send(ClientEvent::NewClient {
            client_id,
            sender: peer_sender,
        })
        .await
        .unwrap();

    //let stream_wr = stream.clone();
    let mut brd = BufReader::new(socket_reader);
    let mut reader = shv::connection::FrameReader::new(&mut brd);
    async fn login_fn(frame_reader: &mut FrameReader<BufReader<&TcpStream>>, mut frame_writer: &TcpStream, broker_reader: Receiver<PeerEvent>, broker_writer: Sender<PeerEvent>) -> std::result::Result<(), String> {
        let frame = frame_reader.receive_frame().await?;
        let rpcmsg = frame.to_rpcmesage()?;
        if rpcmsg.is_request() && rpcmsg.method().unwrap_or("") == "hello" {
            let resp = rpcmsg.prepare_response()?;
            let nonce = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
            let mut result = shv::Map::new();
            result.insert("nonce".into(), RpcValue::from(&nonce));
            resp.set_result(RpcValue::from(result));
            shv::connection::send_message(&mut frame_writer, &rpcmsg).await?;

            let frame = frame_reader.receive_frame().await?;
            let rpcmsg = frame.to_rpcmesage()?;
            if rpcmsg.is_request() && rpcmsg.method().unwrap_or("") == "login" {
                let params = rpcmsg.params()?.as_map();
                let login = params.get("login")?.as_map();
                let user = login.get("user")?;
                Ok(())
            } else {
                Err(format!("Invalid login message received from: {:?}", frame_writer.peer_addr()))
            }
            //    {
            //    let login = params.as_map().get("login").unwrap_or(&RpcValue::null()).clone();
            //    if check_login(login.as_map(), &nonce) {
            //        let mut result = Map::new();
            //        result.insert("clientId".into(), RpcValue::from(client_id));
            //        resp.set_result(RpcValue::from(result));
            //        peer.sender.send(PeerEvent::Message(resp)).await.unwrap();
            //        peer.stage = PeerStage::Run;
            //        continue;
            //    } else {
            //        "Wrong user name or password"
            //    }
            //} else {
            //    "Login params missing"
            //}
            //                                 if let Ok(rpcmsg) = frame.to_rpcmesage() {
            //                     if rpcmsg.is_request() {
            //                         let peer = broker.peers.get_mut(&client_id).unwrap();
            //                         if let Ok(mut resp) = rpcmsg.prepare_response() {
            //                             let errmsg = if rpcmsg.method().unwrap_or("") == "login" {
            //                             } else {
            //                                 "login() call expected"
            //                             };
            //                             resp.set_error(RpcError::new(RpcErrorCode::MethodCallException, &errmsg));
            //                             peer.sender.send(PeerEvent::Message(resp)).await.unwrap();
            //                         }
            //                     }
            //                 }


        } else {
            Err(format!("Invalid hello message received from: {:?}", frame_writer.peer_addr()));
        }
    };
    loop {
        match login_fn().await {
            Ok(_) => {
                break;
            }
            Err(errmsg) => {
                warn!("{}", errmsg);
            }
        }
    }
    loop {
        select! {
            frame = reader.receive_frame().fuse() => match frame {
                Ok(frame) => {
                    match frame {
                        None => {
                            debug!("Client socket closed");
                            break;
                        }
                        Some(frame) => {
                            broker.send(ClientEvent::Frame { client_id, frame }).await.unwrap();
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
                        PeerEvent::FatalError(errmsg) => {
                            error!("Fatal client error: {}", errmsg);
                            break;
                        }
                        PeerEvent::Frame(frame) => {
                            shv::connection::send_frame(&mut socket_writer, frame).await?;
                        }
                        PeerEvent::Message(rpcmsg) => {
                            shv::connection::send_message(&mut socket_writer, &rpcmsg).await?;
                        }
                    }
                }
            }
        }
    }
    broker.send(ClientEvent::ClientGone { client_id }).await.unwrap();

    Ok(())
}

#[derive(Debug)]
enum ClientEvent {
    GetPassword {
        user: String,
    },
    NewClient {
        client_id: i32,
        sender: Sender<PeerEvent>,
    },
    Frame {
        client_id: i32,
        frame: RpcFrame,
    },
    ClientGone {
        client_id: i32,
    },
}

#[derive(Debug)]
enum PeerEvent {
    PasswordSha1(Vec<u8>),
    Frame(RpcFrame),
    Message(RpcMessage),
    FatalError(String),
}
#[derive(Debug, Clone)]
enum PeerStage {
    Hello,
    Login(String),
    Run,
}
#[derive(Debug)]
struct Peer {
    sender: Sender<PeerEvent>,
    stage: PeerStage,
}
struct Device {
    client_id: i32,
}
type Node = Box<dyn ShvNode + Send + Sync>;
enum Mount {
    Device(Device),
    Node(Node),
}
struct Broker {
    peers: HashMap<i32, Peer>,
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
}
fn check_login(login: &Map, nonce: &str) -> bool {
    let def_user = "admin";
    let def_password = "admin";
    if let Some(user) = login.get("user") {
        if let Some(password) = login.get("password") {
            if let Some(login_type) = login.get("type") {
                if login_type.as_str() == "PLAIN" {
                    return user.as_str() == def_user && password.as_str() == def_password;
                }
            }
            let hash = shv::connection::sha1_password_hash(def_password.as_bytes(), nonce.as_bytes());
            return user.as_str() == def_user && password.as_str().as_bytes() == &hash;
        }
    }
    false
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
                    ClientEvent::Frame { client_id, frame} => {
                        let peer_stage = &broker.peers.get(&client_id).unwrap().stage.clone();
                        match peer_stage {
                            PeerStage::Hello => {
                                let peer = broker.peers.get_mut(&client_id).unwrap();
                                if let Ok(rpcmsg) = frame.to_rpcmesage() {
                                    if rpcmsg.is_request() && rpcmsg.method().unwrap_or("") == "hello" {
                                        let mut resp = match  rpcmsg.prepare_response() {
                                            Ok(resp) => {resp}
                                            Err(e) => {
                                                warn!("Client id: {} hello error: {}", client_id, &e);
                                                continue;
                                            }
                                        };
                                        let nonce = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
                                        let mut result = shv::Map::new();
                                        result.insert("nonce".into(), RpcValue::from(&nonce));
                                        resp.set_result(RpcValue::from(result));
                                        peer.sender.send(PeerEvent::Message(resp)).await.unwrap();
                                        peer.stage = PeerStage::Login(nonce);
                                    } else {
                                        peer.sender.send(PeerEvent::FatalError("Invalid hello message".into())).await.unwrap();
                                    }
                                }
                                continue;
                            }
                            PeerStage::Login(nonce) => {
                                if let Ok(rpcmsg) = frame.to_rpcmesage() {
                                    if rpcmsg.is_request() {
                                        let peer = broker.peers.get_mut(&client_id).unwrap();
                                        if let Ok(mut resp) = rpcmsg.prepare_response() {
                                            let errmsg = if rpcmsg.method().unwrap_or("") == "login" {
                                                if let Some(params) = rpcmsg.params() {
                                                    let login = params.as_map().get("login").unwrap_or(&RpcValue::null()).clone();
                                                    if check_login(login.as_map(), &nonce) {
                                                        let mut result = Map::new();
                                                        result.insert("clientId".into(), RpcValue::from(client_id));
                                                        resp.set_result(RpcValue::from(result));
                                                        peer.sender.send(PeerEvent::Message(resp)).await.unwrap();
                                                        peer.stage = PeerStage::Run;
                                                        continue;
                                                    } else {
                                                        "Wrong user name or password"
                                                    }
                                                } else {
                                                    "Login params missing"
                                                }
                                            } else {
                                                "login() call expected"
                                            };
                                            resp.set_error(RpcError::new(RpcErrorCode::MethodCallException, &errmsg));
                                            peer.sender.send(PeerEvent::Message(resp)).await.unwrap();
                                        }
                                    }
                                }
                                continue;
                            }
                            PeerStage::Run => if frame.is_request() {
                                let shv_path = frame.shv_path().unwrap_or("");
                                let result: Option<std::result::Result<RpcValue, String>> = if let Some((mount, node_path)) = broker.find_mount(shv_path) {
                                        match mount {
                                            Mount::Device(device) => {
                                                let client_id = device.client_id;
                                                let mut frame2 = frame.clone();
                                                frame2.push_caller_id(client_id);
                                                frame2.set_shvpath(node_path);
                                                let _ = broker.peers.get(&client_id).unwrap().sender.send(PeerEvent::Frame(frame2)).await;
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
                                            match broker.ls(shv_path) {
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
                                    if let Ok(meta) = RpcFrame::prepare_response_meta(&frame.meta) {
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
                            },
                        }
                    }
                    ClientEvent::NewClient { client_id, sender } => match broker.peers.entry(client_id) {
                        Entry::Occupied(..) => (),
                        Entry::Vacant(entry) => {
                            entry.insert(Peer {
                                sender,
                                stage: PeerStage::Hello,
                            });
                        }
                    },
                    ClientEvent::ClientGone { client_id } => {
                        assert!(broker.peers.remove(&client_id).is_some());
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
