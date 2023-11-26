use std::collections::BTreeMap;
use async_std::{prelude::*};
use structopt::StructOpt;
use async_std::io::BufReader;
use async_std::net::TcpStream;
use shv::{client, RpcMessage, RpcMessageMetaTags, RpcValue, shvnode};
use async_std::task;
use log::*;
use percent_encoding::percent_decode;
use simple_logger::SimpleLogger;
use url::Url;
use shv::client::LoginParams;
use shv::rpcframe::RpcFrame;
use shv::rpcmessage::{RpcError, RpcErrorCode};
use shv::shvnode::{AppNode, find_longest_prefix, shv_dir_methods, ShvNode};

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

    let mut mounts = BTreeMap::new();
    mounts.insert(".app".into(), Box::new(AppNode{ app_name: "device".to_string() }));
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
                    if let Ok(rpcmsg) = frame.to_rpcmesage() {
                        let shv_path = frame.shv_path().unwrap_or("");
                        let response_meta= RpcFrame::prepare_response_meta(&frame.meta);
                        let result = if let Some((mount, path)) = find_longest_prefix(&mounts, &shv_path) {
                            let node = mounts.get_mut(mount).unwrap();
                            let mut rpcmsg2 = rpcmsg;
                            rpcmsg2.set_shvpath(path);
                            match node.process_request(&rpcmsg2) {
                                Ok(res) => Ok(res.unwrap_or(RpcValue::null())),
                                Err(err) => { Err(err.to_string()) }
                            }
                        } else {
                            match rpcmsg.method() {
                                None => {
                                    Err(format!("Shv call method missing."))
                                }
                                Some("dir") => {
                                    Ok(shvnode::dir(shv_dir_methods().into_iter(), rpcmsg.param().into()))
                                }
                                Some("ls") => {
                                    shvnode::ls(&mounts, shv_path, rpcmsg.param().into())
                                }
                                Some(method) => {
                                    Err(format!("Unknown method {}", method))
                                }
                            }
                        };
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
                            let bytes = resp.as_rpcvalue().to_chainpack();
                            writer.write_all(&bytes).await?;
                            writer.flush().await?;
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

