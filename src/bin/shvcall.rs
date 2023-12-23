use async_std::{io, prelude::*};
use structopt::StructOpt;
use async_std::io::{BufReader};
use async_std::net::TcpStream;
use shv::{client, RpcMessage, RpcValue};
use async_std::task;
use log::*;
use percent_encoding::percent_decode;
use simple_logger::SimpleLogger;
use url::Url;
use shv::client::LoginParams;
use shv::connection::FrameReader;
use shv::util::parse_log_verbosity;

type Result = shv::Result<()>;

#[derive(StructOpt, Debug)]
//#[structopt(name = "shvcall", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = "SHV call")]
struct Opts {
    ///Url to connect to, example tcp://localhost:3755, localsocket:path/to/socket
    #[structopt(name = "url", short = "-s", long = "--url")]
    url: String,
    #[structopt(short = "p", long = "path")]
    path: Option<String>,
    #[structopt(short = "m", long = "method")]
    method: Option<String>,
    #[structopt(short = "a", long = "param")]
    param: Option<String>,
    /// Write only result, trim RPC message
    #[structopt(short = "r", long = "trim-meta")]
    trim_meta: bool,
    /// Verbose mode (module, .)
    #[structopt(short = "v", long = "verbose")]
    verbose: Option<String>,
    ///Output as Chainpack instead of default CPON
    #[structopt(short = "-x", long = "--chainpack")]
    chainpack: bool,
}

// const DEFAULT_RPC_TIMEOUT_MSEC: u64 = 5000;
pub(crate) fn main() -> Result {
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

    task::block_on(try_main(&url, &opts))
}

async fn make_call(url: &Url, opts: &Opts) -> Result {
    // Establish a connection
    let address = format!("{}:{}", url.host_str().unwrap_or_default(), url.port().unwrap_or(3755));
    let stream = TcpStream::connect(&address).await?;
    let (reader, mut writer) = (&stream, &stream);

    let mut brd = BufReader::new(reader);
    let mut frame_reader = shv::connection::FrameReader::new(&mut brd);

    // login
    let login_params = LoginParams {
        user: url.username().to_string(),
        password: percent_decode(url.password().unwrap_or("").as_bytes()).decode_utf8()?.into(),
        heartbeat_interval: None,
        ..Default::default()
    };

    client::login(&mut frame_reader, &mut writer, &login_params).await?;
    info!("Connected to broker.");
    async fn print_resp(stdout: &mut io::Stdout, resp: &RpcMessage, to_chainpack: bool, trim_meta: bool) -> Result {
        let rpcval = if resp.is_success() {
            if trim_meta {
                resp.result().expect("result should be set").clone()
            } else {
                resp.as_rpcvalue().clone()
            }
        } else {
            resp.error().expect("error should be set").to_rpcvalue()
        };
        if to_chainpack {
            let bytes = rpcval.to_chainpack();
            stdout.write_all(&bytes).await?;
            Ok(stdout.flush().await?)
        } else {
            let bytes = rpcval.to_cpon().into_bytes();
            stdout.write_all(&bytes).await?;
            Ok(stdout.write_all("\n".as_bytes()).await?)
        }
    }
    async fn print_error(stdout: &mut io::Stdout, err: &str, to_chainpack: bool) -> Result {
        if to_chainpack {
            Ok(())
        } else {
            stdout.write_all(err.as_bytes()).await?;
            Ok(stdout.write_all("\n".as_bytes()).await?)
        }
    }
    async fn send_request(mut writer: &TcpStream, reader: &mut FrameReader<'_, BufReader<&TcpStream>>, path: &str, method: &str, param: &str) -> shv::Result<RpcMessage> {
        let param = if param.is_empty() {
            None
        } else {
            Some(RpcValue::from_cpon(param)?)
        };
        let rpcmsg = RpcMessage::new_request(path, method, param);
        shv::connection::send_message(&mut writer, &rpcmsg).await?;

        let resp = reader.receive_message().await?.ok_or("Receive error")?;
        Ok(resp)

    }
    if opts.path.is_none() && opts.method.is_some() {
        return Err("--path parameter missing".into())
    }
    if opts.path.is_some() && opts.method.is_none() {
        return Err("--method parameter missing".into())
    }
    let mut stdout = io::stdout();
    if opts.path.is_none() && opts.method.is_none() {
        let stdin = io::stdin();
        loop {
            let mut line = String::new();
            match stdin.read_line(&mut line).await {
                Ok(nbytes) => {
                    if nbytes == 0 {
                        // stream closed
                        break;
                    } else {
                        let method_ix = match line.find(':') {
                            None => {
                                print_error(&mut stdout, &format!("Invalid line format, method not found: {line}"), opts.chainpack).await?;
                                break;
                            }
                            Some(ix) => { ix }
                        };
                        let param_ix = line.find(' ');
                        let path = line[..method_ix].trim();
                        let (method, param) = match param_ix {
                            None => { (line[method_ix + 1 .. ].trim(), "") }
                            Some(ix) => { (line[method_ix + 1 .. ix].trim(), line[ix + 1 ..].trim()) }
                        };
                        let resp = send_request(writer, &mut frame_reader, &path, &method, &param).await?;
                        print_resp(&mut stdout, &resp, opts.chainpack, opts.trim_meta).await?;
                    }
                }
                Err(err) => { return Err(format!("Read line error: {err}").into()) }
            }
        }
    } else {
        let path = opts.path.clone().unwrap_or_default();
        let method = opts.method.clone().unwrap_or_default();
        let param = opts.param.clone().unwrap_or_default();
        let resp = send_request(writer, &mut frame_reader, &path, &method, &param).await?;
        print_resp(&mut stdout, &resp, opts.chainpack, opts.trim_meta).await?;
    }

    Ok(())
}

async fn try_main(url: &Url, opts: &Opts) -> Result {
    match make_call(url, opts).await {
        Ok(_) => { Ok(()) }
        Err(err) => {
            eprintln!("{err}");
            Err(err)
        }
    }
}
