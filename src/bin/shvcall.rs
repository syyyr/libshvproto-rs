use async_std::{io, prelude::*};
use structopt::StructOpt;
use async_std::io::{BufReader};
use async_std::net::TcpStream;
use shv::{client, RpcMessage, RpcMessageMetaTags, RpcValue};
use async_std::task;
use log::*;
use percent_encoding::percent_decode;
use simple_logger::SimpleLogger;
use url::Url;
use shv::client::LoginParams;
use shv::rpcmessage::{RqId};
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
    /// Output format: [ cpon | chainpack | simple | value ], default is 'cpon'
    #[structopt(short = "o", long = "output-format", default_value = "cpon")]
    output_format: String,
    /// Verbose mode (module, .)
    #[structopt(short = "v", long = "verbose")]
    verbose: Option<String>,
}
enum OutputFormat {
    Cpon,
    ChainPack,
    Simple,
    Value,
}
impl From<&str> for OutputFormat {
    fn from(value: &str) -> Self {
        let s = value.to_ascii_lowercase();
        if "chainpack".starts_with(&s) { return Self::ChainPack }
        if "simple".starts_with(&s) { return Self::Simple }
        if "value".starts_with(&s) { return Self::Value }
        Self::Cpon
    }
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
    async fn print_resp(stdout: &mut io::Stdout, resp: &RpcMessage, output_format: OutputFormat) -> Result {
        let bytes = match output_format {
            OutputFormat::Cpon => {
                let mut s = resp.as_rpcvalue().to_cpon();
                s.push('\n');
                s.as_bytes().to_owned()
            }
            OutputFormat::ChainPack => {
                resp.as_rpcvalue().to_chainpack().to_owned()
            }
            OutputFormat::Simple => {
                let s = if resp.is_request() {
                    format!("REQ {}:{} {}\n", resp.shv_path().unwrap_or_default(), resp.method().unwrap_or_default(), resp.param().unwrap_or_default().to_cpon())
                } else if resp.is_response() {
                    match resp.result() {
                        Ok(res) => {
                            format!("RES {}\n", res.to_cpon())
                        }
                        Err(err) => {
                            format!("ERR {}\n", err.to_string())
                        }
                    }
                } else {
                    format!("SIG {}:{} {}\n", resp.shv_path().unwrap_or_default(), resp.method().unwrap_or_default(), resp.param().unwrap_or_default().to_cpon())
                };
                s.as_bytes().to_owned()
            }
            OutputFormat::Value => {
                let mut s = if resp.is_request() {
                    resp.param().unwrap_or_default().to_cpon()
                } else if resp.is_response() {
                    match resp.result() {
                        Ok(res) => {
                            res.to_cpon()
                        }
                        Err(err) => {
                            err.to_string()
                        }
                    }
                } else {
                    resp.param().unwrap_or_default().to_cpon()
                };
                s.push('\n');
                s.as_bytes().to_owned()
            }
        };
        stdout.write_all(&bytes).await?;
        Ok(stdout.flush().await?)
    }

    async fn send_request(mut writer: &TcpStream, path: &str, method: &str, param: &str) -> shv::Result<RqId> {
        let param = if param.is_empty() {
            None
        } else {
            Some(RpcValue::from_cpon(param)?)
        };
        let rpcmsg = RpcMessage::new_request(path, method, param);
        shv::connection::send_message(&mut writer, &rpcmsg).await?;
        Ok(rpcmsg.request_id().expect("Request ID should exist here."))
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
                                return Err(format!("Invalid line format, method not found: {line}").into());
                            }
                            Some(ix) => { ix }
                        };
                        let param_ix = line.find(' ');
                        let path = line[..method_ix].trim();
                        let (method, param) = match param_ix {
                            None => { (line[method_ix + 1 .. ].trim(), "") }
                            Some(ix) => { (line[method_ix + 1 .. ix].trim(), line[ix + 1 ..].trim()) }
                        };
                        let rqid = send_request(writer, &path, &method, &param).await?;
                        loop {
                            let resp = frame_reader.receive_message().await?.ok_or("Receive error")?;
                            print_resp(&mut stdout, &resp, (&*opts.output_format).into()).await?;
                            if resp.is_response() && resp.request_id().unwrap_or_default() == rqid {
                                break;
                            }
                        }
                    }
                }
                Err(err) => { return Err(format!("Read line error: {err}").into()) }
            }
        }
    } else {
        let path = opts.path.clone().unwrap_or_default();
        let method = opts.method.clone().unwrap_or_default();
        let param = opts.param.clone().unwrap_or_default();
        send_request(writer, &path, &method, &param).await?;
        let resp = frame_reader.receive_message().await?.ok_or("Receive error")?;
        print_resp(&mut stdout, &resp, (&*opts.output_format).into()).await?;
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
