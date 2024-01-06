use async_std::{channel, future};
use async_std::io::BufReader;
use async_std::net::TcpStream;
use futures::select;
use futures::FutureExt;
use log::{debug, error, info};
use rand::distributions::{Alphanumeric, DistString};
use url::Url;
use crate::{client, RpcMessage, RpcMessageMetaTags, RpcValue};
use crate::client::LoginParams;
use crate::rpcframe::RpcFrame;
use crate::shvnode::{DOT_LOCAL_DIR, DOT_LOCAL_GRANT, DOT_LOCAL_HACK, METH_PING};
use crate::util::{join_path, login_from_url, sha1_hash};
use crate::broker::{PeerToBrokerMessage, LoginResult, BrokerToPeerMessage, PeerKind};
use crate::broker::config::ParentBrokerConfig;
use crate::broker::Sender;

pub(crate) async fn peer_loop(client_id: i32, broker_writer: Sender<PeerToBrokerMessage>, stream: TcpStream) -> crate::Result<()> {
    debug!("Entreing peer loop client ID: {client_id}.");
    let (socket_reader, mut writer) = (&stream, &stream);
    let (peer_writer, peer_reader) = channel::unbounded::<BrokerToPeerMessage>();

    broker_writer.send(PeerToBrokerMessage::NewPeer { client_id, sender: peer_writer, peer_kind: PeerKind::Client }).await?;

    //let stream_wr = stream.clone();
    let mut brd = BufReader::new(socket_reader);

    let mut frame_reader = crate::connection::FrameReader::new(&mut brd);
    let mut frame_writer = crate::connection::FrameWriter::new(&mut writer);

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
            debug!("Client ID: {client_id}, hello received.");
            let nonce = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
            let mut result = crate::Map::new();
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
            debug!("Client ID: {client_id}, login received.");
            let params = rpcmsg.param().ok_or("No login params")?.as_map();
            let login = params.get("login").ok_or("Invalid login params")?.as_map();
            let user = login.get("user").ok_or("User login param is missing")?.as_str();
            let password = login.get("password").ok_or("Password login param is missing")?.as_str();
            let login_type = login.get("type").map(|v| v.as_str()).unwrap_or("");

            broker_writer.send(PeerToBrokerMessage::GetPassword { client_id, user: user.to_string() }).await.unwrap();
            match peer_reader.recv().await? {
                BrokerToPeerMessage::PasswordSha1(broker_shapass) => {
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
                        debug!("Client ID: {client_id}, password OK.");
                        let mut result = crate::Map::new();
                        result.insert("clientId".into(), RpcValue::from(client_id));
                        frame_writer.send_result(resp_meta, result.into()).await?;
                        if let Some(options) = params.get("options") {
                            if let Some(device) = options.as_map().get("device") {
                                device_options = device.clone();
                            }
                        }
                        break LoginResult::Ok;
                    } else {
                        debug!("Client ID: {client_id}, login credentials.");
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
        debug!("Client ID: {client_id} login sucess.");
        let register_device = PeerToBrokerMessage::RegisterDevice {
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
                                broker_writer.send(PeerToBrokerMessage::FrameReceived { client_id, frame }).await?;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Read socket error: {}", &e);
                        break;
                    }
                },
                event = peer_reader.recv().fuse() => match event {
                    Err(e) => {
                        debug!("Peer channel closed: {}", &e);
                        break;
                    }
                    Ok(event) => {
                        match event {
                            BrokerToPeerMessage::PasswordSha1(_) => {
                                panic!("PasswordSha1 cannot be received here")
                            }
                            BrokerToPeerMessage::DisconnectByBroker => {
                                info!("Disconnected by broker, client ID: {client_id}");
                                break;
                            }
                            BrokerToPeerMessage::SendFrame(frame) => {
                                // log!(target: "RpcMsg", Level::Debug, "<---- Send frame, client id: {}", client_id);
                                frame_writer.send_frame(frame).await?;
                            }
                            BrokerToPeerMessage::SendMessage(rpcmsg) => {
                                // log!(target: "RpcMsg", Level::Debug, "<---- Send message, client id: {}", client_id);
                                frame_writer.send_message(rpcmsg).await?;
                            },
                        }
                    }
                }
            }
        }
    }
    broker_writer.send(PeerToBrokerMessage::PeerGone { client_id }).await?;
    info!("Client loop exit, client id: {}", client_id);
    Ok(())
}
pub(crate) async fn parent_broker_peer_loop_with_reconnect(client_id: i32, config: ParentBrokerConfig, broker_writer: Sender<PeerToBrokerMessage>) -> crate::Result<()> {
    let url = Url::parse(&config.client.url)?;
    if url.scheme() != "tcp" {
        return Err(format!("Scheme {} is not supported yet.", url.scheme()).into());
    }
    let reconnect_interval: std::time::Duration = loop {
        if let Some(time_str) = &config.client.reconnect_interval {
            if let Ok(interval) = duration_str::parse(time_str) {
                break interval
            }
        }
        const DEFAULT_RECONNECT_INTERVAL_SEC: u64 = 10;
        info!("Parent broker connection reconnect interval is not set explicitly, default value {DEFAULT_RECONNECT_INTERVAL_SEC} will be used.");
        break std::time::Duration::from_secs(DEFAULT_RECONNECT_INTERVAL_SEC)
    };
    info!("Reconnect interval set to: {:?}", reconnect_interval);
    loop {
        match parent_broker_peer_loop(client_id, config.clone(), broker_writer.clone()).await {
            Ok(_) => {
                info!("Parent broker peer loop finished without error");
            }
            Err(err) => {
                error!("Parent broker peer loop finished with error: {err}");
            }
        }
        info!("Reconnecting to parent broker after: {:?}", reconnect_interval);
        async_std::task::sleep(reconnect_interval).await;
    }
}

async fn parent_broker_peer_loop(client_id: i32, config: ParentBrokerConfig, broker_writer: Sender<PeerToBrokerMessage>) -> crate::Result<()> {
    let url = Url::parse(&config.client.url)?;
    let (scheme, host, port) = (url.scheme(), url.host_str().unwrap_or_default(), url.port().unwrap_or(3755));
    if scheme != "tcp" {
        return Err(format!("Scheme {scheme} is not supported yet.").into());
    }
    let address = format!("{host}:{port}");
    // Establish a connection
    info!("Connecting to parent broker: tcp://{address}");
    let stream = TcpStream::connect(&address).await?;
    let (reader, mut writer) = (&stream, &stream);

    let mut brd = BufReader::new(reader);
    let mut frame_reader = crate::connection::FrameReader::new(&mut brd);
    let mut frame_writer = crate::connection::FrameWriter::new(&mut writer);

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
    info!("Heartbeat interval set to: {:?}", &heartbeat_interval);
    client::login(&mut frame_reader, &mut frame_writer, &login_params).await?;

    let (broker_to_peer_sender, broker_to_peer_receiver) = channel::unbounded::<BrokerToPeerMessage>();
    broker_writer.send(PeerToBrokerMessage::NewPeer { client_id, sender: broker_to_peer_sender, peer_kind: PeerKind::ParentBroker }).await?;

    loop {
        let fut_timeout = future::timeout(heartbeat_interval, future::pending::<()>());
        select! {
            res_timeout = fut_timeout.fuse() => {
                assert!(res_timeout.is_err());
                // send heartbeat
                let msg = RpcMessage::new_request(".app", METH_PING, None);
                debug!("sending ping");
                frame_writer.send_message(msg).await?;
            },
            res_frame = frame_reader.receive_frame().fuse() => match res_frame {
                Ok(None) => {
                    info!("Parent broker socket closed");
                    return Err("Parent broker socket closed".into());
                }
                Ok(Some(mut frame)) => {
                    if frame.is_request() {
                        fn is_dot_local_granted(frame: &RpcFrame) -> bool {
                                let access = frame.access().unwrap_or_default();
                                access.split(',').find(|s| *s == DOT_LOCAL_GRANT).is_some()
                        }
                        fn is_dot_local_request(frame: &RpcFrame) -> bool {
                            let shv_path = frame.shv_path().unwrap_or_default();
                            if shv_path.starts_with(DOT_LOCAL_DIR)&& (shv_path.len() == DOT_LOCAL_DIR.len() || shv_path[DOT_LOCAL_DIR.len() ..].starts_with('/')) {
                                return is_dot_local_granted(frame);
                            }
                            false
                        }
                        let shv_path = frame.shv_path().unwrap_or_default().to_owned();
                        let shv_path = if shv_path.starts_with("/") {
                            // parent broker can send requests with absolute path
                            // to call subscribe(), rejectNotSubscribe() etc.
                            shv_path[1 ..].to_string()
                        } else if is_dot_local_request(&frame) {
                            let shv_path = &shv_path[DOT_LOCAL_DIR.len() ..];
                            let shv_path = if shv_path.starts_with('/') { &shv_path[1 ..] } else { shv_path };
                            shv_path.to_string()
                        } else {
                            if shv_path.is_empty() && is_dot_local_granted(&frame) {
                                frame.meta.insert(DOT_LOCAL_HACK, true.into());
                            }
                            join_path(&config.exported_root, &shv_path)
                        };
                        frame.set_shvpath(&shv_path);
                        broker_writer.send(PeerToBrokerMessage::FrameReceived { client_id, frame }).await.unwrap();
                    }
                }
                Err(e) => {
                    error!("Parent broker receive frame error - {e}");
                }
            },
            event = broker_to_peer_receiver.recv().fuse() => match event {
                Err(e) => {
                    debug!("broker loop has closed peer channel, client ID {client_id}");
                    return Err(e.into());
                }
                Ok(event) => {
                    match event {
                        BrokerToPeerMessage::PasswordSha1(_) => {
                            panic!("PasswordSha1 cannot be received here")
                        }
                        BrokerToPeerMessage::DisconnectByBroker => {
                            info!("Disconnected by parent broker, client ID: {client_id}");
                            break;
                        }
                        BrokerToPeerMessage::SendFrame(frame) => {
                            // log!(target: "RpcMsg", Level::Debug, "<---- Send frame, client id: {}", client_id);
                            debug!("Sending rpc frame");
                            frame_writer.send_frame(frame).await?;
                        }
                        BrokerToPeerMessage::SendMessage(rpcmsg) => {
                            // log!(target: "RpcMsg", Level::Debug, "<---- Send message, client id: {}", client_id);
                            debug!("Sending rpc message");
                            frame_writer.send_message(rpcmsg).await?;
                        },
                    }
                }
            }
        }
    };
    Ok(())
}
