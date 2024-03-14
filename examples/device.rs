use std::collections::{BTreeMap};
use shv::{RpcMessageMetaTags, RpcValue, shvnode};
use async_std::{task};
use async_trait::async_trait;
use log::*;
use simple_logger::SimpleLogger;
use shv::client::{ClientConfig};
use shv::metamethod::{AccessLevel, Flag, MetaMethod};
use shv::rpcframe::RpcFrame;
use shv::rpcmessage::{RpcError, RpcErrorCode};
use shv::shvnode::{ShvNode, AppNode, METH_GET, METH_SET, SIG_CHNG, META_METHOD_DIR, META_METHOD_LS, DIR_APP, DIR_APP_DEVICE, AppDeviceNode, METH_DIR, METH_LS};
use shv::util::{join_path, parse_log_verbosity};
use clap::{Parser};
use shv::device::{DeviceCommand, DIR_NUMBER, main_async, NodeRequestContext};

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
fn main() -> shv::Result<()> {
    let cli_opts = Opts::parse();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Info);
    if let Some(module_names) = &cli_opts.verbose {
        for (module, level) in parse_log_verbosity(module_names, module_path!()) {
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

    let state = DeviceState {
        node_app: AppNode{
            app_name: "testdeviceapp",
            shv_version_major: 3,
            shv_version_minor: 0,
        },
        node_device: AppDeviceNode {
            device_name: "device",
            version: "0.1",
            serial_number: None,
        },
        node_property_number: IntPropertyNode { value: 0 },
    };
    task::block_on(main_async(config, state))
}
struct DeviceState {
    node_app: AppNode,
    node_device: AppDeviceNode,
    node_property_number: IntPropertyNode,
}
impl DeviceState {
}
#[async_trait]
impl shv::device::State for DeviceState {
    fn mounts(&self) -> BTreeMap<String, ShvNode> {
        let mut mounts: BTreeMap<String, ShvNode> = Default::default();
        mounts.insert(DIR_APP.into(), self.node_app.new_shvnode());
        mounts.insert(DIR_APP_DEVICE.into(), self.node_app.new_shvnode());
        mounts.insert(DIR_NUMBER.into(), self.node_property_number.new_shvnode());
        mounts
    }
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
            let result = ShvNode::process_dir(&ctx.methods, &rq);
            ctx.command_sender.send(send_result_cmd(result)).await?;
            return Ok(())
        }
        if ctx.method.name == METH_LS {
            let result = ShvNode::process_ls(&rq);
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
        let err = RpcError::new(RpcErrorCode::MethodNotFound, format!("Unknown method {}:{}", ctx.mount_point, frame.method().unwrap_or_default()));
        ctx.command_sender.send(send_result_cmd(Err(err))).await?;
        Ok(())
    }
}

struct IntPropertyNode {
    value: i32,
}

const META_METH_GET: MetaMethod = MetaMethod { name: METH_GET, flags: Flag::IsGetter as u32, access: AccessLevel::Read, param: "", result: "Int", description: "" };
const META_METH_SET: MetaMethod = MetaMethod { name: METH_SET, flags: Flag::IsSetter as u32, access: AccessLevel::Write, param: "Int", result: "", description: "" };
const META_SIG_CHNG: MetaMethod = MetaMethod { name: SIG_CHNG, flags: Flag::IsSignal as u32, access: AccessLevel::Read, param: "Int", result: "", description: "" };

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
