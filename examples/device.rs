use std::collections::BTreeMap;
use async_std::io::BufReader;
use async_std::net::TcpStream;
use shv::{client, RpcMessage, RpcMessageMetaTags, RpcValue};
use async_std::{future, task};
use log::*;
use simple_logger::SimpleLogger;
use url::Url;
use shv::client::{ClientConfig, LoginParams};
use shv::metamethod::{MetaMethod};
use shv::rpcframe::RpcFrame;
use shv::rpcmessage::{RpcError, RpcErrorCode};
use shv::shvnode::{AppDeviceNode, find_longest_prefix, ShvNode, RequestCommand, AppNode, DIR_LS_METHODS, process_local_dir_ls, METH_GET, METH_SET, SIG_CHNG, PROPERTY_METHODS, METH_PING};
use shv::util::{login_from_url, parse_log_verbosity};
use duration_str::{parse};
use clap::{Parser};

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

pub(crate) fn main() -> shv::Result<()> {
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
    if let Some(reconnect_interval) = cli_opts.reconnect_interval { config.reconnect_interval = Some(reconnect_interval) }
    config.heartbeat_interval = cli_opts.heartbeat_interval;

    task::block_on(try_main_reconnect(&config))
}

async fn try_main_reconnect(config: &ClientConfig) -> shv::Result<()> {
    if let Some(time_str) = &config.reconnect_interval {
        match parse(time_str) {
            Ok(interval) => {
                info!("Reconnect interval set to: {:?}", interval);
                loop {
                    match try_main(config).await {
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
        try_main(config).await
    }
}
async fn try_main(config: &ClientConfig) -> shv::Result<()> {
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
    client::login(&mut frame_reader, &mut writer, &login_params).await?;

    let mut mounts: BTreeMap<String, Box<dyn ShvNode<()>>> = BTreeMap::new();
    mounts.insert(".app".into(), Box::new(AppNode { app_name: "device", ..Default::default() }));
    mounts.insert(".app/device".into(), Box::new(AppDeviceNode { device_name: "example", ..Default::default() }));
    mounts.insert("number".into(), Box::new(IntPropertyNode{ value: 0 }));
    loop {
        let fut_receive_frame = frame_reader.receive_frame();
        match future::timeout(heartbeat_interval, fut_receive_frame).await {
            Err(_e) => {
                // send heartbeat
                let msg = RpcMessage::new_request(".app", METH_PING, None);
                shv::connection::send_message(&mut writer, &msg).await?;
                continue;
            }
            Ok(recv_result) => {
                match recv_result {
                    Ok(None) => {
                        return Err("Broker socket closed".into());
                    }
                    Ok(Some(frame)) => {
                        if frame.is_request() {
                            if let Ok(mut rpcmsg) = frame.to_rpcmesage() {
                                let shv_path = frame.shv_path().unwrap_or_default();
                                let local_result = process_local_dir_ls(&mounts, &frame);
                                type Command = RequestCommand<()>;
                                let command = match local_result {
                                    None => {
                                        if let Some((mount, path)) = find_longest_prefix(&mounts, &shv_path) {
                                            let node = mounts.get_mut(mount).unwrap();
                                            rpcmsg.set_shvpath(path);
                                            node.process_request(&rpcmsg)
                                        } else {
                                            let method = frame.method().unwrap_or_default();
                                            Command::Error(RpcError::new(RpcErrorCode::MethodNotFound, &format!("Invalid shv path {}:{}()", shv_path, method)))
                                        }
                                    }
                                    Some(command) => { command }
                                };
                                let response_meta= RpcFrame::prepare_response_meta(&frame.meta);
                                if let Ok(meta) = response_meta {
                                    let command = if let Command::PropertyChanged(value) = &command {
                                        let sig = RpcMessage::new_signal(shv_path, SIG_CHNG, Some(value.clone()));
                                        shv::connection::send_message(&mut writer, &sig).await?;
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
                                            shv::connection::send_message(&mut writer, &resp).await?;
                                        }
                                        _ => {  }
                                    };
                                    if resp.is_success() || resp.is_error() {
                                        if let Err(error) = shv::connection::send_message(&mut writer, &resp).await {
                                            error!("Error writing to peer socket: {error}");
                                        }
                                    }
                                } else {
                                    warn!("Invalid request frame received.");
                                }
                            } else {
                                warn!("Invalid shv request");
                            }
                        }
                    }
                    Err(e) => {
                        error!("Receive frame error - {e}");
                    }
                }
            }
        }
    }
}
struct IntPropertyNode {
    value: i32,
}
impl ShvNode<()> for IntPropertyNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(PROPERTY_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rq: &RpcMessage) -> RequestCommand<()> {
        match rq.method() {
            Some(METH_GET) => {
                RequestCommand::Result(self.value.into())
            }
            Some(METH_SET) => {
                match rq.param() {
                    None => RequestCommand::Error(RpcError::new(RpcErrorCode::InvalidParam, "Invalid parameter")),
                    Some(v) => {
                        if v.is_int() {
                            let v = v.as_i32();
                            if v == self.value {
                                RequestCommand::Result(RpcValue::null())
                            } else {
                                self.value = v;
                                RequestCommand::PropertyChanged(v.into())
                            }
                        } else {
                            RequestCommand::Error(RpcError::new(RpcErrorCode::InvalidParam, "Invalid parameter"))
                        }
                    }
                }
            }
            _ => {
                ShvNode::<()>::process_dir_ls(self, rq)
            }
        }
    }
}
