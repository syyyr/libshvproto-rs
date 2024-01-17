use std::collections::{BTreeMap};
use std::time::{Duration, Instant};
use async_std::io::BufReader;
use async_std::net::TcpStream;
use shv::{client, MetaMap, RpcMessage, RpcMessageMetaTags, RpcValue, shvnode};
use async_std::{future, task};
use async_std::channel::{Receiver, Sender, unbounded};
use log::*;
use simple_logger::SimpleLogger;
use url::Url;
use shv::client::{ClientConfig, LoginParams};
use shv::metamethod::{Access, Flag, MetaMethod};
use shv::rpcframe::RpcFrame;
use shv::rpcmessage::{RpcError, RpcErrorCode, RqId};
use shv::shvnode::{find_longest_prefix, ShvNode, AppNode, process_local_dir_ls, METH_GET, METH_SET, SIG_CHNG, META_METHOD_DIR, META_METHOD_LS, DIR_APP, DIR_APP_DEVICE, AppDeviceNode, METH_DIR, METH_LS, METH_PING};
use shv::util::{join_path, login_from_url, parse_log_verbosity};
use duration_str::{parse};
use clap::{Parser};
use futures::select;
use futures::FutureExt;

#[derive(Parser, Debug)]
//#[structopt(name = "device", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = "SHV call")]
struct Opts {
    /// Config file path
    #[arg(long)]
    config: Option<String>,
    /// Create default config file if one specified by --config is not found
    #[arg(short, long)]
    create_default_config: bool,
    ///Url to connect to, example tcp://admin@localhost:3755?password=dj4j5HHb, localsocket:path/to/socket
    #[arg(short = 's', long)]
    url: Option<String>,
    #[arg(short = 'i', long)]
    device_id: Option<String>,
    /// Mount point on broker connected to, note that broker might not accept any path.
    #[arg(short, long)]
    mount: Option<String>,
    /// Device tries to reconnect to broker after this interval, if connection to broker is lost.
    /// Example values: 1s, 1h, etc.
    #[arg(short, long)]
    reconnect_interval: Option<String>,
    /// Client should ping broker with this interval. Broker will disconnect device, if ping is not received twice.
    /// Example values: 1s, 1h, etc.
    #[arg(long, default_value = "1m")]
    heartbeat_interval: String,
    /// Verbose mode (module, .)
    #[arg(short, long)]
    verbose: Option<String>,
}
#[derive(Debug)]
enum DeviceCommand {
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
enum DeviceToPeerCommand {
    SendFrame(RpcFrame),
}
fn main() -> shv::Result<()> {
    let cli_opts = Opts::parse();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Info);
    if let Some(module_names) = &cli_opts.verbose {
        for (module, level) in parse_log_verbosity(&module_names, module_path!()) {
            logger = logger.with_module_level(module, level);
        }
    }
    logger.init().unwrap();

    log::info!("=====================================================");
    log::info!("{} starting", std::module_path!());
    log::info!("=====================================================");

    let mut config = if let Some(config_file) = &cli_opts.config {
        ClientConfig::from_file_or_default(config_file, cli_opts.create_default_config)?
    } else {
        Default::default()
    };
    if let Some(url) = cli_opts.url { config.url = url }
    if let Some(device_id) = cli_opts.device_id { config.device_id = Some(device_id) }
    if let Some(mount) = cli_opts.mount { config.mount = Some(mount) }
    config.reconnect_interval = cli_opts.reconnect_interval;
    config.heartbeat_interval = cli_opts.heartbeat_interval;

    task::block_on(main_async(config))
}
async fn main_async(config: ClientConfig) -> shv::Result<()> {
    let device = Device::new();
    shv::spawn_and_log_error(peer_loop_reconnect(config, device.command_sender.clone()));
    shv::spawn_and_log_error(device_loop(device)).await;
    Ok(())
}
async fn peer_loop_reconnect(config: ClientConfig, device_sender: Sender<DeviceCommand>) -> shv::Result<()> {
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
async fn peer_loop(config: &ClientConfig, device_sender: Sender<DeviceCommand>) -> shv::Result<()> {
    let url = Url::parse(&config.url)?;
    let (scheme, host, port) = (url.scheme(), url.host_str().unwrap_or_default(), url.port().unwrap_or(3755));
    if scheme != "tcp" {
        return Err(format!("Scheme {scheme} is not supported yet.").into());
    }
    let address = format!("{host}:{port}");
    // Establish a connection
    info!("Connecting to: {address}");
    let stream = TcpStream::connect(&address).await?;
    let (reader, mut writer) = (&stream, &stream);

    let mut brd = BufReader::new(reader);
    let mut frame_reader = shv::connection::FrameReader::new(&mut brd);
    let mut frame_writer = shv::connection::FrameWriter::new(&mut writer);

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
                let frame = RpcFrame::from_rpcmessage(msg)?;
                ping_rq_id = frame.try_request_id()?;
                frame_writer.send_frame(frame).await?;
                recent_ping_ts = Instant::now();
            },
            frame = frame_reader.receive_frame().fuse() => match frame {
                Ok(frame) => {
                    match frame {
                        None => {
                            return Err("Broker socket closed".into());
                        }
                        Some(frame) => {
                            if frame.request_id().unwrap_or_default() == ping_rq_id {
                                debug!("ping response received: {:?}", frame);
                            } else {
                                device_sender.send(DeviceCommand::FrameReceived(frame)).await?;
                            }
                        }
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
async fn device_loop(mut device: Device) -> shv::Result<()> {
    device.main_loop().await
}
//type RequestHandler = fn (&mut DeviceState, RpcFrame, NodeRequestContext) -> Pin<Box<dyn Future<Output = shv::Result<()>>>>;
struct Device {
    command_sender: Sender<DeviceCommand>,
    command_receiver: Receiver<DeviceCommand>,
    peer_sender: Option<Sender<DeviceToPeerCommand>>,
    mounts: BTreeMap<String, ShvNode>,
    //handlers: HashMap<String, RequestHandler>,

    pending_rpc_calls: Vec<PendingRpcCall>,

    node_app: AppNode,
    node_device: AppDeviceNode,
    node_property_number: IntPropertyNode,
}

impl Device {
    fn new() -> Self {
        let (command_sender, command_receiver) = unbounded();
        let mut dev = Self {
            command_sender,
            command_receiver,
            peer_sender: Default::default(),
            mounts: Default::default(),
            pending_rpc_calls: vec![],
            node_app: AppNode{
                app_name: "testdeviceapp",
                shv_version_major: 3,
                shv_version_minor: 0,
            },
            node_device: AppDeviceNode {
                device_name: "device",
                version: "1.0",
                serial_number: Some("SN:5626d169ab20".into()),
            },
            node_property_number: IntPropertyNode { value: 0 },
        };
        dev.mounts.insert(DIR_APP.into(), dev.node_app.new_shvnode());
        dev.mounts.insert(DIR_APP_DEVICE.into(), dev.node_app.new_shvnode());
        dev.mounts.insert(DIR_NUMBER.into(), dev.node_property_number.new_shvnode());

        dev
    }
    async fn main_loop(&mut self) -> shv::Result<()> {
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
    async fn process_command(&mut self, command: DeviceCommand) -> shv::Result<()> {
        match command {
            DeviceCommand::SendResponse { meta, result } => {
                if let Some(sender) = &self.peer_sender {
                    let mut msg = RpcMessage::from_meta(meta);
                    msg.set_result_or_error(result);
                    sender.send(DeviceToPeerCommand::SendFrame(RpcFrame::from_rpcmessage(msg)?)).await?;
                }
            }
            DeviceCommand::SendSignal { shv_path, method, value } => {
                if let Some(sender) = &self.peer_sender {
                    let msg = RpcMessage::new_signal(&shv_path, &method, value);
                    sender.send(DeviceToPeerCommand::SendFrame(RpcFrame::from_rpcmessage(msg)?)).await?;
                }
            }
            DeviceCommand::RpcCall { request, response_sender } => {
                let rq_id = request.try_request_id()?;
                let frame = RpcFrame::from_rpcmessage(request)?;
                match self.command_sender.send(DeviceCommand::SendFrame(frame)).await {
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
    async fn process_rpc_frame(&mut self, frame: RpcFrame) -> shv::Result<()> {
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
                        method,
                    };
                    self.process_node_request(frame, ctx).await?;
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
    fn handle_app_ping_async(&mut self, frame: RpcFrame, ctx: crate::NodeRequestContext) -> Pin<Box<dyn Future<Output = shv::Result<()>>>> {
        //BoxFuture::new(Self::handle_app_ping(self, frame, ctx))
        Box::pin(Self::handle_app_ping(self, frame, ctx))
    }
    async fn handle_app_ping(&mut self, frame: RpcFrame, ctx: crate::NodeRequestContext) -> shv::Result<()> {
        let meta = RpcFrame::prepare_response_meta(&frame.meta)?;
        let cmd = Command::SendResponse { meta, result: Ok(().into()) };
        ctx.command_sender.send(cmd).await?;
        return Ok(())
    }
     */
    async fn process_node_request(&mut self, frame: RpcFrame, ctx: NodeRequestContext) -> shv::Result<()> {
        let response_meta = RpcFrame::prepare_response_meta(&frame.meta)?;
        let send_result_cmd = |result: Result<RpcValue, RpcError>| -> DeviceCommand {
            DeviceCommand::SendResponse {
                meta: response_meta,
                result,
            }
        };
        let rq = frame.to_rpcmesage()?;
        if ctx.method.name == METH_DIR {
            let node = self.mounts.get(&ctx.mount_point).expect("Shv node");
            let result = node.process_dir(&rq);
            ctx.command_sender.send(send_result_cmd(result)).await?;
            return Ok(())
        }
        if ctx.method.name == METH_LS {
            let node = self.mounts.get(&ctx.mount_point).expect("Shv node");
            let result = node.process_ls(&rq);
            ctx.command_sender.send(send_result_cmd(result)).await?;
            return Ok(())
        }
        match ctx.mount_point.as_str() {
            DIR_APP => {
                match ctx.method.name {
                    shvnode::METH_NAME => {
                        ctx.command_sender.send(send_result_cmd(Ok(self.node_app.app_name.into()))).await?;
                        return Ok(())
                    }
                    shvnode::METH_SHV_VERSION_MAJOR => {
                        ctx.command_sender.send(send_result_cmd(Ok(self.node_app.shv_version_major.into()))).await?;
                        return Ok(())
                    }
                    shvnode::METH_SHV_VERSION_MINOR => {
                        ctx.command_sender.send(send_result_cmd(Ok(self.node_app.shv_version_minor.into()))).await?;
                        return Ok(())
                    }
                    shvnode::METH_PING => {
                        ctx.command_sender.send(send_result_cmd(Ok(().into()))).await?;
                        return Ok(())
                    }
                    _ => {}
                }
            }
            DIR_APP_DEVICE => {
                match ctx.method.name {
                    shvnode::METH_NAME => {
                        ctx.command_sender.send(send_result_cmd(Ok(self.node_device.device_name.into()))).await?;
                        return Ok(())
                    }
                    shvnode::METH_VERSION => {
                        ctx.command_sender.send(send_result_cmd(Ok(self.node_device.version.into()))).await?;
                        return Ok(())
                    }
                    shvnode::METH_SERIAL_NUMBER => {
                        ctx.command_sender.send(send_result_cmd(Ok(self.node_device.serial_number.clone().unwrap_or_default().into()))).await?;
                        return Ok(())
                    }
                    _ => {}
                }
            }
            DIR_NUMBER => {
                match ctx.method.name {
                    METH_GET => {
                        ctx.command_sender.send(send_result_cmd(Ok(self.node_property_number.value.into()))).await?;
                        return Ok(())
                    }
                    METH_SET => {
                        let new_val = rq.param().unwrap_or_default().as_i32();
                        if new_val != self.node_property_number.value {
                            self.node_property_number.value = new_val;
                            ctx.command_sender.send(DeviceCommand::SendSignal {
                                shv_path: join_path(&ctx.mount_point, rq.shv_path().unwrap_or_default()),
                                method: SIG_CHNG.to_string(),
                                value: Some(new_val.into()),
                            }).await?;
                        }
                        ctx.command_sender.send(send_result_cmd(Ok(().into()))).await?;
                        return Ok(())
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        let err = RpcError::new(RpcErrorCode::MethodNotFound, format!("Unknown method {}:{}", ctx.mount_point, frame.method().unwrap_or_default()).into());
        ctx.command_sender.send(send_result_cmd(Err(err))).await?;
        Ok(())
    }
    async fn process_pending_rpc_call(&mut self, response_frame: RpcFrame) -> shv::Result<()> {
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
struct NodeRequestContext {
    command_sender: Sender<DeviceCommand>,
    mount_point: String,
    method: &'static MetaMethod,
}
pub const DIR_NUMBER: &str = "number";

struct IntPropertyNode {
    value: i32,
}

const META_METH_GET: MetaMethod = MetaMethod { name: METH_GET, flags: Flag::IsGetter as u32, access: Access::Read, param: "", result: "Int", description: "" };
const META_METH_SET: MetaMethod = MetaMethod { name: METH_SET, flags: Flag::IsSetter as u32, access: Access::Write, param: "Int", result: "", description: "" };
const META_SIG_CHNG: MetaMethod = MetaMethod { name: SIG_CHNG, flags: Flag::IsSignal as u32, access: Access::Read, param: "Int", result: "", description: "" };

impl IntPropertyNode {
    pub fn new_shvnode(&self) -> ShvNode {
        ShvNode { methods: vec![
            &META_METHOD_DIR,
            &META_METHOD_LS,
            &META_METH_GET,
            &META_METH_SET,
            &META_SIG_CHNG,
        ] }
    }
}
