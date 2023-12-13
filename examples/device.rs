use std::collections::BTreeMap;
use structopt::StructOpt;
use async_std::io::BufReader;
use async_std::net::TcpStream;
use shv::{client, RpcMessage, RpcMessageMetaTags, RpcValue};
use async_std::task;
use log::*;
use percent_encoding::percent_decode;
use simple_logger::SimpleLogger;
use url::Url;
use shv::client::LoginParams;
use shv::metamethod::{MetaMethod};
use shv::rpcframe::RpcFrame;
use shv::rpcmessage::{RpcError, RpcErrorCode};
use shv::shvnode::{AppDeviceNode, find_longest_prefix, ShvNode, ProcessRequestResult, Signal, AppNode, DIR_LS_METHODS, process_local_dir_ls, METH_GET, METH_SET, SIG_CHNG, PROPERTY_METHODS};

#[derive(StructOpt, Debug)]
//#[structopt(name = "device", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = "SHV call")]
struct Opts {
    ///Url to connect to, example tcp://localhost:3755, localsocket:path/to/socket
    #[structopt(name = "url", short = "-s", long = "--url")]
    url: String,
    #[structopt(short = "-i", long = "--device-id")]
    device_id: Option<String>,
    #[structopt(short = "-m", long = "--mount")]
    mount: Option<String>,
    /// Verbose mode (module, .)
    #[structopt(short = "v", long = "verbose")]
    verbose: Option<String>,
}

pub(crate) fn main() -> shv::Result<()> {
    let opts = Opts::from_args();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Error);
    if let Some(module_names) = &opts.verbose {
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

    log::info!("=====================================================");
    log::info!("{} starting", std::module_path!());
    log::info!("=====================================================");

    // let rpc_timeout = Duration::from_millis(DEFAULT_RPC_TIMEOUT_MSEC);
    let url = Url::parse(&opts.url)?;

    task::block_on(try_main(&url, &opts))
}

struct DeviceState {
    int_prop: i32,
}

async fn try_main(url: &Url, opts: &Opts) -> shv::Result<()> {

    // Establish a connection
    let address = format!("{}:{}", url.host_str().unwrap_or_default(), url.port().unwrap_or(3755));
    let stream = TcpStream::connect(&address).await?;
    let (reader, mut writer) = (&stream, &stream);

    let mut brd = BufReader::new(reader);
    let mut frame_reader = shv::connection::FrameReader::new(&mut brd);

    // login
    let login_params = LoginParams{
        user: url.username().to_string(),
        password: percent_decode(url.password().unwrap_or("").as_bytes()).decode_utf8()?.into(),
        mount_point: match opts.mount { None => {"".to_string()} Some(ref str) => {str.clone()} },
        device_id: match opts.device_id { None => {"".to_string()} Some(ref str) => {str.clone()} },
        ..Default::default()
    };
    client::login(&mut frame_reader, &mut writer, &login_params).await?;


    let mut device_state = DeviceState { int_prop: 0 };
    let mut mounts: BTreeMap<String, Box<dyn ShvNode<DeviceState>>> = BTreeMap::new();
    mounts.insert(".app".into(), Box::new(AppNode { app_name: "device", ..Default::default() }));
    mounts.insert(".app/device".into(), Box::new(AppDeviceNode { device_name: "example", ..Default::default() }));
    mounts.insert("number".into(), Box::new(IntPropertyNode{}));
    loop {
        match frame_reader.receive_frame().await {
            Err(e) => {
                warn!("Invalid frame received: {}", &e);
                continue;
            }
            Ok(None) => {
                debug!("Broker socket closed");
                break;
            }
            Ok(Some(frame)) => {
                if frame.is_request() {
                    if let Ok(mut rpcmsg) = frame.to_rpcmesage() {
                        let shv_path = frame.shv_path().unwrap_or_default();
                        let local_result = process_local_dir_ls(&mounts, &frame);
                        let result = match local_result {
                            None => {
                                if let Some((mount, path)) = find_longest_prefix(&mounts, &shv_path) {
                                    let node = mounts.get_mut(mount).unwrap();
                                    rpcmsg.set_shvpath(path);
                                    node.process_request(&rpcmsg, &mut device_state)
                                } else {
                                    let method = frame.method().unwrap_or_default();
                                    Err(RpcError::new(RpcErrorCode::MethodNotFound, &format!("Invalid shv path {}:{}()", shv_path, method)))
                                }
                            }
                            Some(result) => { result }
                        };
                        let response_meta= RpcFrame::prepare_response_meta(&frame.meta);
                        if let Ok(meta) = response_meta {
                            let mut resp = RpcMessage::from_meta(meta);
                            match result {
                                Ok((value, signal)) => {
                                    resp.set_result(value);
                                    shv::connection::send_message(&mut writer, &resp).await?;
                                    if let Some(signal) = signal {
                                        let sig = RpcMessage::new_signal(shv_path, signal.method, Some(signal.value));
                                        shv::connection::send_message(&mut writer, &sig).await?;
                                    }
                                }
                                Err(errmsg) => {
                                    resp.set_error(errmsg);
                                    shv::connection::send_message(&mut writer, &resp).await?;
                                }
                            }
                        }
                    } else {
                        warn!("Invalid shv request");
                    }
                }
            }
        }
    }

    Ok(())
}
struct IntPropertyNode {
}
impl ShvNode<DeviceState> for IntPropertyNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(PROPERTY_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rq: &RpcMessage, state: &mut DeviceState) -> ProcessRequestResult {
        match rq.method() {
            Some(METH_GET) => {
                Ok((state.int_prop.into(), None))
            }
            Some(METH_SET) => {
                match rq.param() {
                    None => Err(RpcError::new(RpcErrorCode::InvalidParam, "Invalid parameter")),
                    Some(v) => {
                        if v.is_int() {
                            let v = v.as_i32();
                            if v == state.int_prop {
                                Ok((RpcValue::from(false), None))
                            } else {
                                state.int_prop = v;
                                Ok((RpcValue::from(true), Some(Signal{ value: v.into(), method: SIG_CHNG })))
                            }
                        } else {
                            Err(RpcError::new(RpcErrorCode::InvalidParam, "Invalid parameter"))
                        }
                    }
                }
            }
            _ => {
                ShvNode::<DeviceState>::process_dir_ls(self, rq)
            }
        }
    }
}
