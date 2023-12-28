use std::collections::BTreeMap;
use structopt::StructOpt;
use async_std::io::BufReader;
use async_std::net::TcpStream;
use shv::{client, RpcMessage, RpcMessageMetaTags, RpcValue};
use async_std::task;
use log::*;
use simple_logger::SimpleLogger;
use url::Url;
use shv::client::LoginParams;
use shv::metamethod::{MetaMethod};
use shv::rpcframe::RpcFrame;
use shv::rpcmessage::{RpcError, RpcErrorCode};
use shv::shvnode::{AppDeviceNode, find_longest_prefix, ShvNode, RequestCommand, AppNode, DIR_LS_METHODS, process_local_dir_ls, METH_GET, METH_SET, SIG_CHNG, PROPERTY_METHODS};
use shv::util::{login_from_url, parse_log_verbosity};
use duration_str::{parse};

#[derive(StructOpt, Debug)]
//#[structopt(name = "device", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = "SHV call")]
struct Opts {
    ///Url to connect to, example tcp://admin@localhost:3755?password=dj4j5HHb, localsocket:path/to/socket
    #[structopt(name = "url", short = "-s", long = "--url")]
    url: String,
    #[structopt(short = "-i", long = "--device-id")]
    device_id: Option<String>,
    /// Mount point on broker connected to, note that broker might not accept any path.
    #[structopt(short = "-m", long = "--mount")]
    mount: Option<String>,
    /// Device tries to reconnect to broker after this interval, if connection to broker is lost.
    /// Example values: 1s, 1h, etc.
    #[structopt(short = "r", long = "--reconnect-interval")]
    reconnect_interval: Option<String>,
    /// Verbose mode (module, .)
    #[structopt(short = "v", long = "verbose")]
    verbose: Option<String>,
}

pub(crate) fn main() -> shv::Result<()> {
    let opts = Opts::from_args();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Info);
    if let Some(module_names) = &opts.verbose {
        for (module, level) in parse_log_verbosity(&module_names, module_path!()) {
            logger = logger.with_module_level(module, level);
        }
    }
    logger.init().unwrap();

    log::info!("=====================================================");
    log::info!("{} starting", std::module_path!());
    log::info!("=====================================================");

    // let rpc_timeout = Duration::from_millis(DEFAULT_RPC_TIMEOUT_MSEC);
    let url = Url::parse(&opts.url)?;
    task::block_on(try_main_reconnect(&url, &opts))
}

async fn try_main_reconnect(url: &Url, opts: &Opts) -> shv::Result<()> {
    if let Some(time_str) = &opts.reconnect_interval {
        match parse(time_str) {
            Ok(interval) => {
                info!("Reconnect interval set to: {:?}", interval);
                loop {
                    match try_main(&url, &opts).await {
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
        try_main(&url, &opts).await
    }
}
async fn try_main(url: &Url, opts: &Opts) -> shv::Result<()> {

    let address = format!("{}:{}", url.host_str().unwrap_or_default(), url.port().unwrap_or(3755));
    // Establish a connection
    info!("Connecting to: {address}");
    let stream = TcpStream::connect(&address).await?;
    let (reader, mut writer) = (&stream, &stream);

    let mut brd = BufReader::new(reader);
    let mut frame_reader = shv::connection::FrameReader::new(&mut brd);

    // login
    let (user, password) = login_from_url(url);
    let login_params = LoginParams{
        user,
        password,
        mount_point: match opts.mount { None => {"".to_string()} Some(ref str) => {str.clone()} },
        device_id: match opts.device_id { None => {"".to_string()} Some(ref str) => {str.clone()} },
        ..Default::default()
    };

    info!("Connected OK");
    client::login(&mut frame_reader, &mut writer, &login_params).await?;


    let mut mounts: BTreeMap<String, Box<dyn ShvNode<()>>> = BTreeMap::new();
    mounts.insert(".app".into(), Box::new(AppNode { app_name: "device", ..Default::default() }));
    mounts.insert(".app/device".into(), Box::new(AppDeviceNode { device_name: "example", ..Default::default() }));
    mounts.insert("number".into(), Box::new(IntPropertyNode{ value: 0 }));
    loop {
        match frame_reader.receive_frame().await {
            Err(e) => {
                warn!("Invalid frame received: {}", &e);
                continue;
            }
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
