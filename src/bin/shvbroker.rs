use std::{
    collections::hash_map::{Entry, HashMap},
};
use futures::{select, FutureExt, StreamExt};
use async_std::{channel, io::BufReader, net::{TcpListener, TcpStream, ToSocketAddrs}, prelude::*, task};

use rand::distributions::{Alphanumeric, DistString};
use log::*;
use structopt::StructOpt;
use shv::rpcframe::RpcFrame;
use shv::{List, RpcMessage, RpcValue, shvnode};
use shv::{Map};
use shv::rpcmessage::{RpcError, RpcErrorCode};
use shv::RpcMessageMetaTags;
use simple_logger::SimpleLogger;
use shv::metamethod::{MetaMethod};
use shv::shvnode::ShvNode;

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
        spawn_and_log_error(connection_loop(client_id, broker_sender.clone(), stream));
    }
    drop(broker_sender);
    broker.await;
    Ok(())
}

async fn connection_loop(client_id: i32, broker: Sender<ClientEvent>, stream: TcpStream) -> Result<()> {
    let (socket_reader, mut socket_writer) = (&stream, &stream);
    let (peer_sender, peer_receiver) = channel::unbounded::<PeerEvent>();
    broker
        .send(ClientEvent::NewPeer {
            client_id,
            sender: peer_sender,
        })
        .await
        .unwrap();

    //let stream_wr = stream.clone();
    let mut brd = BufReader::new(socket_reader);
    let mut reader = shv::connection::FrameReader::new(&mut brd);
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
                        PeerEvent::Message(rpcmsg) => {
                            shv::connection::send_message(&mut socket_writer, &rpcmsg).await?;
                        }
                    }
                }
            }
        }
    }
    broker.send(ClientEvent::PeerGone { client_id }).await.unwrap();

    Ok(())
}

#[derive(Debug)]
enum ClientEvent {
    NewPeer {
        client_id: i32,
        sender: Sender<PeerEvent>,
    },
    Frame {
        client_id: i32,
        frame: RpcFrame,
    },
    PeerGone {
        client_id: i32,
    },
}

#[derive(Debug)]
enum PeerEvent {
    Message(RpcMessage),
    FatalError(String),
}
#[derive(Debug)]
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
    clientId: i32,
    node: Box<dyn ShvNode<MethodIterator=impl ShvNode::MethodIterator>>,
}
struct Broker {
    peers: HashMap<i32, Peer>,
    
    mounts: HashMap<String, i32>,
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
        mounts: HashMap::new(),
    };
    loop {
        match events.recv().await {
            Err(e) => {
                info!("Client channel closed: {}", &e);
                break;
            }
            Ok(event) => {
                match event {
                    ClientEvent::Frame { client_id, frame} => {
                        if let Some(peer) = broker.peers.get_mut(&client_id) {
                            let rpcmsg = match frame.to_rpcmesage() {
                                Ok(msg) => { msg }
                                Err(e) => {
                                    peer.sender.send(PeerEvent::FatalError(format!("RPC error: {}", &e))).await.unwrap();
                                    continue;
                                }
                            };
                            match &peer.stage {
                                PeerStage::Hello => {
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
                                        continue;
                                    }
                                    peer.sender.send(PeerEvent::FatalError("Invalid hello message".into())).await.unwrap();
                                    continue;
                                }
                                PeerStage::Login(nonce) => {
                                    if rpcmsg.is_request() {
                                        let mut resp = match rpcmsg.prepare_response() {
                                            Ok(resp) => { resp }
                                            Err(e) => {
                                                peer.sender.send(PeerEvent::FatalError(format!("Login error: {}", &e))).await.unwrap();
                                                continue;
                                            }
                                        };
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
                                    //peer.sender.send(PeerEvent::FatalError("Invalid login message".into())).await.unwrap();
                                    continue;
                                }
                                PeerStage::Run => {
                                    if rpcmsg.is_request() {
                                        let mut resp = match  rpcmsg.prepare_response() {
                                            Ok(resp) => {resp}
                                            Err(e) => {
                                                warn!("Client id: {} cannot create response error: {}", client_id, &e);
                                                continue;
                                            }
                                        };
                                        let result = match rpcmsg.shv_path() {
                                            Some(".app") => {
                                                match rpcmsg.method() {
                                                    Some("dir") => {
                                                        let methods = [
                                                            MetaMethod { name: "dir".into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
                                                            MetaMethod { name: "ls".into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
                                                            MetaMethod { name: "ping".into(), ..Default::default() },
                                                        ];
                                                        Ok(shvnode::dir(methods.iter(), rpcmsg.params().into()))
                                                    }
                                                    Some("ls") => {
                                                        match rpcmsg.params() {
                                                            None => Ok(List::new().into()),
                                                            Some(_) => Ok(false.into()),
                                                        }
                                                    }
                                                    Some("ping") => {
                                                        Ok(RpcValue::null())
                                                    }
                                                    Some("name") => {
                                                        Ok("shvbroker".into())
                                                    }
                                                    _ => {
                                                        Err(format!("Invalid method: {:?}", rpcmsg.method()))
                                                    }
                                                }
                                            }
                                            Some(shv_path) => {
                                                match rpcmsg.method() {
                                                    None => {
                                                        Err(format!("Empty method"))
                                                    }
                                                    Some("ls") => {
                                                        Err(format!("Invalid path: {:?}", rpcmsg.shv_path()))
                                                    }
                                                    Some("dir") => {
                                                        Err(format!("Invalid path: {:?}", rpcmsg.shv_path()))
                                                    }
                                                    Some(method) => {
                                                        Err(format!("Invalid method: {:?}", method))
                                                    }
                                                }
                                            }
                                            None => {
                                                Err(format!("Empty path"))
                                            }
                                        };
                                        match result {
                                            Ok(value) => {
                                                resp.set_result(value);
                                            }
                                            Err(errmsg) => {
                                                resp.set_error(RpcError::new(RpcErrorCode::MethodCallException, &errmsg));
                                            }
                                        }
                                        peer.sender.send(PeerEvent::Message(resp)).await.unwrap();
                                    };
                                    continue;
                                }
                            }
                        }
                    }
                    ClientEvent::NewPeer { client_id, sender } => match broker.peers.entry(client_id) {
                        Entry::Occupied(..) => (),
                        Entry::Vacant(entry) => {
                            entry.insert(Peer {
                                sender,
                                stage: PeerStage::Hello,
                            });
                        }
                    },
                    ClientEvent::PeerGone { client_id } => {
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
