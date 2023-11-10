use async_std::{io,prelude::*};
use structopt::StructOpt;
use shv::client::LoginParams;
use async_std::io::BufReader;
use async_std::net::TcpStream;
use shv::{RpcMessage, RpcValue};
use async_std::task;
use log::*;
use simple_logger::SimpleLogger;
use url::Url;
use percent_encoding::percent_decode;

#[derive(StructOpt, Debug)]
//#[structopt(name = "shvcall", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = "SHV call")]
struct Opts {
    ///Url to connect to, example tcp://localhost:3755, localsocket:path/to/socket
    #[structopt(name = "url", short = "-s", long = "--url")]
    url: String,
    #[structopt(short = "-p", long = "--path")]
    path: String,
    #[structopt(short = "-m", long = "--method")]
    method: String,
    #[structopt(short = "-a", long = "--params")]
    params: Option<String>,
    /// Verbose mode (module, .)
    #[structopt(short = "v", long = "verbose")]
    verbose: Option<String>,
    ///Output as Chainpack instead of default CPON
    #[structopt(short = "-x", long = "--chainpack")]
    chainpack: bool,
}

// const DEFAULT_RPC_TIMEOUT_MSEC: u64 = 5000;
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
    let mut reader = shv::rpc::FrameReader::new(&mut brd);

    // login
    {
        let rq = RpcMessage::create_request("", "hello", None);
        shv::rpc::send_message(&mut writer, &rq).await?;
        let resp = reader.receive_message().await?.unwrap_or_default();
        if !resp.is_success() {
            return Err(resp.error().unwrap().to_rpcvalue().to_cpon().into());
        }
        let nonce = resp.result().ok_or("Bad result")?.as_map()
            .get("nonce").ok_or("Bad nonce")?.as_str();
        let password = percent_decode(url.password().unwrap_or("").as_bytes()).decode_utf8()?;
        let hash = shv::rpc::sha1_password_hash(password.as_bytes(), nonce.as_bytes());
        let login_params = LoginParams{
            user: url.username().to_string(),
            password: std::str::from_utf8(&hash)?.into(),
            heartbeat_interval: None,
            ..Default::default()
        };
        let rq = RpcMessage::create_request("", "login", Some(login_params.to_rpcvalue()));
        shv::rpc::send_message(&mut writer, &rq).await?;
        let resp = reader.receive_message().await?.ok_or("Receive error")?;
        if !resp.is_success() {
            return Err(resp.error().unwrap().message.into());
        }
    }

    let params = match &opts.params {
        None => None,
        Some(p) => {
            Some(RpcValue::from_cpon(&p)?)
        },
    };
    let rpcmsg = RpcMessage::create_request(&opts.path, &opts.method, params);
    shv::rpc::send_message(&mut writer, &rpcmsg).await?;

    let resp = reader.receive_message().await?.ok_or("Receive error")?;
    let mut stdout = io::stdout();
    let response_bytes = if opts.chainpack {
        resp.as_rpcvalue().to_chainpack()
    } else {
        resp.to_cpon().into_bytes()
    };
    stdout.write_all(&response_bytes).await?;
    stdout.flush().await?;
    Ok(())
}
