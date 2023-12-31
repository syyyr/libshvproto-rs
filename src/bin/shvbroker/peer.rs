use async_std::{channel, future};
use async_std::io::BufReader;
use async_std::net::TcpStream;
use futures::select;
use futures::FutureExt;
use log::{debug, error, info, warn};
use rand::distributions::{Alphanumeric, DistString};
use url::Url;
use shv::{client, RpcMessage, RpcMessageMetaTags, RpcValue};
use shv::client::LoginParams;
use shv::rpcframe::RpcFrame;
use shv::shvnode::METH_PING;
use shv::util::{join_path, login_from_url, sha1_hash};
use crate::broker::{ClientEvent, LoginResult, PeerEvent};
use crate::config::ParentBrokerConfig;
use crate::Sender;

pub(crate) async fn peer_loop(client_id: i32, broker_writer: Sender<ClientEvent>, stream: TcpStream) -> crate::Result<()> {
    let (socket_reader, mut writer) = (&stream, &stream);
    let (peer_writer, peer_receiver) = channel::unbounded::<PeerEvent>();

    broker_writer.send(ClientEvent::NewPeer { client_id, sender: peer_writer }).await.unwrap();

    //let stream_wr = stream.clone();
    let mut brd = BufReader::new(socket_reader);

    let mut frame_reader = shv::connection::FrameReader::new(&mut brd);
    let mut frame_writer = shv::connection::FrameWriter::new(&mut writer);

    let mut device_options = RpcValue::null();
    let login_result = loop {
        let nonce = {
            let frame = match frame_reader.receive_frame().await? {
                None => break LoginResult::ClientSocketClosed,
                Some(frame) => { frame }
            };
            let rpcmsg = frame.to_rpcmesage()?;
            let resp_meta = RpcFrame::prepare_response_meta(&frame.meta)?;
            if rpcmsg.method().unwrap_or("") != "hello" {
                frame_writer.send_error(resp_meta, "Invalid login message.").await?;
                continue;
            }
            let nonce = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
            let mut result = shv::Map::new();
            result.insert("nonce".into(), RpcValue::from(&nonce));
            frame_writer.send_result(resp_meta, result.into()).await?;
            nonce
        };
        {
            let frame = match frame_reader.receive_frame().await? {
                None => break LoginResult::ClientSocketClosed,
                Some(frame) => { frame }
            };
            let rpcmsg = frame.to_rpcmesage()?;
            let resp_meta = RpcFrame::prepare_response_meta(&frame.meta)?;
            if rpcmsg.method().unwrap_or("") != "login" {
                frame_writer.send_error(resp_meta, "Invalid login message.").await?;
                continue;
            }
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
                        frame_writer.send_result(resp_meta, result.into()).await?;
                        if let Some(options) = params.get("options") {
                            if let Some(device) = options.as_map().get("device") {
                                device_options = device.clone();
                            }
                        }
                        break LoginResult::Ok;
                    } else {
                        frame_writer.send_error(resp_meta, &format!("Invalid login credentials.")).await?;
                        continue;
                    }
                }
                _ => {
                    panic!("Internal error, PeerEvent::PasswordSha1 expected");
                }
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
                                frame_writer.send_frame(frame).await?;
                            }
                            PeerEvent::Message(rpcmsg) => {
                                // log!(target: "RpcMsg", Level::Debug, "<---- Send message, client id: {}", client_id);
                                frame_writer.send_message(rpcmsg).await?;
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
pub async fn parent_broker_peer_loop(client_id: i32, config: ParentBrokerConfig, broker_writer: Sender<ClientEvent>) -> shv::Result<()> {
    info!("Connecting to parent broker: {}", &config.client.url);
    let url = Url::parse(&config.client.url)?;
    let (scheme, host, port) = (url.scheme(), url.host_str().unwrap_or_default(), url.port().unwrap_or(3755));
    if scheme != "tcp" {
        return Err(format!("Scheme {scheme} is not supported yet.").into());
    }
    let address = format!("{host}:{port}");
    // Establish a connection
    info!("Connecting to parent broker: {address}");
    let stream = TcpStream::connect(&address).await?;
    let (reader, mut writer) = (&stream, &stream);

    let mut brd = BufReader::new(reader);
    let mut frame_reader = shv::connection::FrameReader::new(&mut brd);
    let mut frame_writer = shv::connection::FrameWriter::new(&mut writer);

    // login
    let (user, password) = login_from_url(&url);
    let heartbeat_interval = config.client.heartbeat_interval_duration()?;
    let login_params = LoginParams{
        user,
        password,
        mount_point: (&config.client.mount.clone().unwrap_or_default()).to_owned(),
        device_id: (&config.client.device_id.clone().unwrap_or_default()).to_owned(),
        heartbeat_interval,
        ..Default::default()
    };

    info!("Parent broker connected OK");
    warn!("Heartbeat interval set to: {:?}", &heartbeat_interval);
    client::login(&mut frame_reader, &mut frame_writer, &login_params).await?;

    let (peer_writer, peer_receiver) = channel::unbounded::<PeerEvent>();
    broker_writer.send(ClientEvent::NewPeer { client_id, sender: peer_writer }).await.unwrap();

    loop {
        let fut_timeout = future::timeout(heartbeat_interval, future::pending::<()>());
        select! {
            res_timeout = fut_timeout.fuse() => {
                assert!(res_timeout.is_err());
                // send heartbeat
                let msg = RpcMessage::new_request(".app", METH_PING, None);
                frame_writer.send_message(msg).await?;
            },
            res_frame = frame_reader.receive_frame().fuse() => match res_frame {
                Ok(None) => {
                    return Err("Parent broker socket closed".into());
                }
                Ok(Some(mut frame)) => {
                    if frame.is_request() {
                        let shv_path = frame.shv_path().unwrap_or_default();
                        let shv_path = join_path(&config.exported_root, shv_path);
                        frame.set_shvpath(&shv_path);
                        broker_writer.send(ClientEvent::Frame { client_id, frame }).await.unwrap();
                    }
                }
                Err(e) => {
                    error!("Parent broker receive frame error - {e}");
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
                            frame_writer.send_frame(frame).await?;
                        }
                        PeerEvent::Message(rpcmsg) => {
                            // log!(target: "RpcMsg", Level::Debug, "<---- Send message, client id: {}", client_id);
                            frame_writer.send_message(rpcmsg).await?;
                        },
                    }
                }
            }
        }
    };
    Ok(())
}
