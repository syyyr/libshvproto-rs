use std::collections::{BTreeMap};
use std::time::{Duration, Instant};
use async_std::io::BufReader;
use async_std::net::TcpStream;
use crate::{client, MetaMap, RpcMessage, RpcMessageMetaTags, RpcValue};
use async_std::{future};
use async_std::channel::{Receiver, Sender, unbounded};
use async_trait::async_trait;
use log::*;
use url::Url;
use crate::client::{ClientConfig, LoginParams};
use crate::metamethod::{MetaMethod};
use crate::rpcframe::RpcFrame;
use crate::rpcmessage::{RpcError, RpcErrorCode, RqId};
use crate::shvnode::{find_longest_prefix, ShvNode, process_local_dir_ls, DIR_APP, METH_PING};
use crate::util::{login_from_url};
use duration_str::{parse};
use futures::select;
use futures::FutureExt;
use futures::io::BufWriter;
use crate::framerw::{FrameReader, FrameWriter};
use crate::streamrw::{StreamFrameReader, StreamFrameWriter};

#[derive(Debug)]
pub enum DeviceCommand {
    NewPeer (Sender<DeviceToPeerCommand>),
    SendFrame (RpcFrame),
    FrameReceived (RpcFrame),
    SendResponse {meta: MetaMap, result: Result<RpcValue, RpcError>},
    SendSignal {shv_path: String, method: String, value: Option<RpcValue>},
    #[allow(dead_code)]
    RpcCall{
        request: RpcMessage,
        response_sender: Sender<RpcFrame>,
    },
}
#[derive(Debug)]
pub enum DeviceToPeerCommand {
    SendFrame(RpcFrame),
}
pub async fn main_async<S: State + Send + 'static>(config: ClientConfig, state: S) -> crate::Result<()> {
    let device = Device::new(state);
    crate::spawn_and_log_error(peer_loop_reconnect(config, device.command_sender.clone()));
    device_loop(device).await
}
async fn peer_loop_reconnect(config: ClientConfig, device_sender: Sender<DeviceCommand>) -> crate::Result<()> {
    if let Some(time_str) = &config.reconnect_interval {
        match parse(time_str) {
            Ok(interval) => {
                info!("Reconnect interval set to: {:?}", interval);
                loop {
                    match peer_loop(&config, device_sender.clone()).await {
                        Ok(_) => {
                            info!("Finished without error");
                            return Ok(())
                        }
                        Err(err) => {
                            error!("Error in main loop: {err}");
                            info!("Reconnecting after: {:?}", interval);
                            async_std::task::sleep(interval).await;
                        }
                    }
                }
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    } else {
        peer_loop(&config, device_sender).await
    }
}
async fn peer_loop(config: &ClientConfig, device_sender: Sender<DeviceCommand>) -> crate::Result<()> {
    let url = Url::parse(&config.url)?;
    let (scheme, host, port) = (url.scheme(), url.host_str().unwrap_or_default(), url.port().unwrap_or(3755));
    if scheme != "tcp" {
        return Err(format!("Scheme {scheme} is not supported yet.").into());
    }
    let address = format!("{host}:{port}");
    // Establish a connection
    info!("Connecting to: {address}");
    let stream = TcpStream::connect(&address).await?;
    let (reader, writer) = (&stream, &stream);

    let brd = BufReader::new(reader);
    let bwr = BufWriter::new(writer);
    let mut frame_reader = StreamFrameReader::new(brd);
    let mut frame_writer = StreamFrameWriter::new(bwr);

    // login
    let (user, password) = login_from_url(&url);
    let heartbeat_interval = config.heartbeat_interval_duration()?;
    let login_params = LoginParams{
        user,
        password,
        mount_point: (&config.mount.clone().unwrap_or_default()).to_owned(),
        device_id: (&config.device_id.clone().unwrap_or_default()).to_owned(),
        heartbeat_interval,
        ..Default::default()
    };

    info!("Connected OK");
    info!("Heartbeat interval set to: {:?}", heartbeat_interval);
    client::login(&mut frame_reader, &mut frame_writer, &login_params).await?;

    let (sender, receiver) = unbounded();
    device_sender.send(DeviceCommand::NewPeer(sender)).await?;
    let heartbeat_interval = config.heartbeat_interval_duration()?;
    let mut ping_rq_id = 0;
    let mut time_to_ping = heartbeat_interval;
    let mut recent_ping_ts = Instant::now();
    loop {
        select! {
            _ = future::timeout(time_to_ping, future::pending::<()>()).fuse() => {
                let msg = RpcMessage::new_request(DIR_APP, METH_PING, None);
                debug!("sending ping: {:?}", msg);
                let frame = RpcFrame::from_rpcmessage(&msg)?;
                ping_rq_id = frame.try_request_id()?;
                frame_writer.send_frame(frame).await?;
                recent_ping_ts = Instant::now();
            },
            frame = frame_reader.receive_frame().fuse() => match frame {
                Ok(frame) => {
                    if frame.request_id().unwrap_or_default() == ping_rq_id {
                        debug!("ping response received: {:?}", frame);
                    } else {
                        device_sender.send(DeviceCommand::FrameReceived(frame)).await?;
                    }
                }
                Err(err) => {
                    error!("Receive frame error: {}", err);
                }
            },
            command = receiver.recv().fuse() => match command {
                Ok(command) => {
                    match command {
                        DeviceToPeerCommand::SendFrame(frame) => {
                            recent_ping_ts = Instant::now();
                            frame_writer.send_frame(frame).await?
                        }
                    }
                }
                Err(err) => {
                    warn!("Receive command error: {}", err);
                }
            },
        }
        let elapsed = recent_ping_ts.elapsed();
        //debug!("elapsed: {:?}", elapsed);
        if elapsed < heartbeat_interval {
            time_to_ping = heartbeat_interval - elapsed;
        } else {
            time_to_ping = Duration::from_millis(1);
        }
        //debug!("time to ping: {:?}", time_to_ping);
    }

}
async fn device_loop<S: State>(mut device: Device<S>) -> crate::Result<()> {
    device.main_loop().await
}
//type RequestHandler = fn (&mut DeviceState, RpcFrame, NodeRequestContext) -> Pin<Box<dyn Future<Output = crate::Result<()>>>>;
#[async_trait]
pub trait State {
    fn mounts(&self) -> BTreeMap<String, ShvNode>;
    async fn process_node_request(&mut self, frame: RpcFrame, ctx: NodeRequestContext) -> crate::Result<()>;

}
struct Device<S: State> {
    command_sender: Sender<DeviceCommand>,
    command_receiver: Receiver<DeviceCommand>,
    peer_sender: Option<Sender<DeviceToPeerCommand>>,
    mounts: BTreeMap<String, ShvNode>,
    //handlers: HashMap<String, RequestHandler>,
    pending_rpc_calls: Vec<PendingRpcCall>,

    state: S,
}
impl<S: State> Device<S> {
    fn new(state: S) -> Self {
        let (command_sender, command_receiver) = unbounded();
        Self {
            command_sender,
            command_receiver,
            peer_sender: Default::default(),
            mounts: state.mounts(),
            pending_rpc_calls: vec![],
            state,
        }
    }
    async fn main_loop(&mut self) -> crate::Result<()> {
        loop {
            debug!("SELECT");
            select! {
                command = self.command_receiver.recv().fuse() => match command {
                    Ok(command) => {
                        debug!("command received: {:?}", command);
                        if let Err(err) = self.process_command(command).await {
                            warn!("Process command error: {}", err);
                        }
                    }
                    Err(err) => {
                        warn!("Receive command error: {}", err);
                    }
                },
            }
        }
    }
    async fn process_command(&mut self, command: DeviceCommand) -> crate::Result<()> {
        match command {
            DeviceCommand::SendResponse { meta, result } => {
                if let Some(sender) = &self.peer_sender {
                    let mut msg = RpcMessage::from_meta(meta);
                    msg.set_result_or_error(result);
                    sender.send(DeviceToPeerCommand::SendFrame(RpcFrame::from_rpcmessage(&msg)?)).await?;
                }
            }
            DeviceCommand::SendSignal { shv_path, method, value } => {
                if let Some(sender) = &self.peer_sender {
                    let msg = RpcMessage::new_signal(&shv_path, &method, value);
                    sender.send(DeviceToPeerCommand::SendFrame(RpcFrame::from_rpcmessage(&msg)?)).await?;
                }
            }
            DeviceCommand::RpcCall { request, response_sender } => {
                let rq_id = request.try_request_id()?;
                match self.command_sender.send(DeviceCommand::SendFrame(request.to_frame()?)).await {
                    Ok(_) => {
                        self.pending_rpc_calls.push(PendingRpcCall{ request_id: rq_id, response_sender });
                    }
                    Err(err) => {
                        warn!("Error write to command queue {err}");
                    }
                }
            }
            DeviceCommand::NewPeer(sender) => {
                self.peer_sender = Some(sender);
            }
            DeviceCommand::FrameReceived(frame) => {
                self.process_rpc_frame(frame).await?;
            }
            DeviceCommand::SendFrame(frame) => {
                if let Some(peer_sender) = &self.peer_sender {
                    if let Err(err) = peer_sender.send(DeviceToPeerCommand::SendFrame(frame)).await {
                        warn!("Error write to peer queue {err}");
                        self.peer_sender = None;
                    }
                }
            }
        }
        Ok(())
    }
    async fn process_rpc_frame(&mut self, frame: RpcFrame) -> crate::Result<()> {
        if frame.is_request() {
            let shv_path = frame.shv_path().unwrap_or_default().to_string();
            let method = frame.method().unwrap_or_default().to_string();
            debug!("request received: {}:{}", shv_path, method);
            let response_meta= RpcFrame::prepare_response_meta(&frame.meta)?;
            // println!("response meta: {:?}", response_meta);
            if let Some(result) = process_local_dir_ls(&self.mounts, &frame) {
                self.command_sender.send(DeviceCommand::SendResponse { meta: response_meta, result }).await?;
                return Ok(())
            }
            if let Some((mount_point, node_path)) = find_longest_prefix(&self.mounts, &shv_path) {
                let node = self.mounts.get(mount_point).expect("shv node should be there");
                let mut frame = frame;
                frame.set_shvpath(node_path);
                if let Some(method) = node.is_request_granted(&frame) {
                    let ctx = NodeRequestContext {
                        command_sender: self.command_sender.clone(),
                        mount_point: mount_point.to_string(),
                        methods: node.methods.clone(),
                        method,
                    };
                    self.state.process_node_request(frame, ctx).await?;
                } else {
                    let err = RpcError::new(RpcErrorCode::PermissionDenied, format!("Method doesn't exist or request to call {}:{} is not granted.", shv_path, frame.method().unwrap_or_default()));
                    self.command_sender.send(DeviceCommand::SendResponse { meta: response_meta, result: Err(err) }).await?;
                }
            } else {
                let err = RpcError::new(RpcErrorCode::MethodNotFound, format!("Invalid shv path {}:{}()", shv_path, method));
                self.command_sender.send(DeviceCommand::SendResponse { meta: response_meta, result: Err(err) }).await?;
                return Ok(());
            }
        } else if frame.is_response() {
            self.process_pending_rpc_call(frame).await?;
        } else if frame.is_signal() {
        }
        Ok(())
    }
    /*
    fn handle_app_ping_async(&mut self, frame: RpcFrame, ctx: crate::NodeRequestContext) -> Pin<Box<dyn Future<Output = crate::Result<()>>>> {
        //BoxFuture::new(Self::handle_app_ping(self, frame, ctx))
        Box::pin(Self::handle_app_ping(self, frame, ctx))
    }
    async fn handle_app_ping(&mut self, frame: RpcFrame, ctx: crate::NodeRequestContext) -> crate::Result<()> {
        let meta = RpcFrame::prepare_response_meta(&frame.meta)?;
        let cmd = Command::SendResponse { meta, result: Ok(().into()) };
        ctx.command_sender.send(cmd).await?;
        return Ok(())
    }
     */
    async fn process_pending_rpc_call(&mut self, response_frame: RpcFrame) -> crate::Result<()> {
        let rqid = response_frame.request_id().ok_or_else(|| "Request ID must be set.")?;
        let mut pending_call_ix = None;
        for (ix, pc) in self.pending_rpc_calls.iter().enumerate() {
            if pc.request_id == rqid {
                pending_call_ix = Some(ix);
                break;
            }
        }
        if let Some(ix) = pending_call_ix {
            let pending_call = self.pending_rpc_calls.remove(ix);
            pending_call.response_sender.send(response_frame).await?;
        }
        Ok(())
    }
}
struct PendingRpcCall {
    request_id: RqId,
    response_sender: Sender<RpcFrame>,
}
pub struct NodeRequestContext {
    pub command_sender: Sender<DeviceCommand>,
    pub mount_point: String,
    pub methods: Vec<&'static MetaMethod>,
    pub method: &'static MetaMethod,
}
pub const DIR_NUMBER: &str = "number";

