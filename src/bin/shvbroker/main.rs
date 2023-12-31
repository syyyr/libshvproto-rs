use std::{fs};
use std::path::Path;
use futures::{StreamExt};
use async_std::{channel, net::{TcpListener}, prelude::*, task};
use log::*;
use simple_logger::SimpleLogger;
use shv::util::{join_path, parse_log_verbosity};
use crate::config::{AccessControl, BrokerConfig};
use clap::{Parser};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Sender<T> = async_std::channel::Sender<T>;
type Receiver<T> = async_std::channel::Receiver<T>;

mod config;
mod node;
mod peer;
mod broker;

#[derive(Parser, Debug)]
struct CliOpts {
    /// Config file path
    #[arg(long)]
    config: Option<String>,
    /// Create default config file if one specified by --config is not found
    #[arg(short, long)]
    create_default_config: bool,
    /// Verbose mode (module, .)
    #[arg(short = 'v', long = "verbose")]
    verbose: Option<String>,
}

pub(crate) fn main() -> Result<()> {
    let cli_opts = CliOpts::parse();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Info);
    if let Some(module_names) = cli_opts.verbose {
        for (module, level) in parse_log_verbosity(&module_names, module_path!()) {
            logger = logger.with_module_level(module, level);
        }
    }
    logger.init().unwrap();

    //trace!("trace message");
    //debug!("debug message");
    //info!("info message");
    //warn!("warn message");
    //error!("error message");
    log!(target: "RpcMsg", Level::Debug, "RPC message");
    log!(target: "Acc", Level::Debug, "Access control message");

    let config = if let Some(config_file) = &cli_opts.config {
        BrokerConfig::from_file_or_default(config_file, cli_opts.create_default_config)?
    } else {
        Default::default()
    };
    let (access, create_editable_access_file) = loop {
        let mut create_editable_access_file = false;
        if let Some(data_dir) = &config.data_directory {
            if config.editable_access {
                let file_name = join_path(data_dir, "access.yaml");
                if Path::new(&file_name).exists() {
                    info!("Loading access file {file_name}");
                    match AccessControl::from_file(&file_name) {
                        Ok(acc) => {
                            break (acc, false);
                        }
                        Err(err) => {
                            error!("Cannot read access file: {file_name} - {err}");
                        }
                    }
                } else {
                    create_editable_access_file = true;
                }
            }
        }
        break (config.access.clone(), create_editable_access_file);
    };
    if create_editable_access_file {
        let data_dir = &config.data_directory.clone().unwrap_or("/tmp/shvbroker/data".into());
        fs::create_dir_all(data_dir)?;
        let access_file = join_path(data_dir, "access.yaml");
        info!("Creating access file {access_file}");
        fs::write(access_file, serde_yaml::to_string(&access)?)?;
    }
    task::block_on(accept_loop(config, access))
}

async fn accept_loop(config: BrokerConfig, access: AccessControl) -> Result<()> {
    if let Some(address) = config.listen.tcp.clone() {
        let (broker_sender, broker_receiver) = channel::unbounded();
        let parent_broker_peer_config = config.parent_broker.clone();
        let broker_task = task::spawn(broker::broker_loop(broker_receiver, access));
        if parent_broker_peer_config.enabled {
            spawn_and_log_error(peer::parent_broker_peer_loop(1, parent_broker_peer_config, broker_sender.clone()));
        }
        info!("Listening on TCP: {}", address);
        let listener = TcpListener::bind(address).await?;
        info!("bind OK");
        let mut client_id = 2; // parent broker has client_id == 1
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream?;
            debug!("Accepting from: {}", stream.peer_addr()?);
            spawn_and_log_error(peer::peer_loop(client_id, broker_sender.clone(), stream));
            client_id += 1;
        }
        drop(broker_sender);
        broker_task.await;
    } else {
        return Err("No port to listen on specified".into());
    }
    Ok(())
}

fn spawn_and_log_error<F>(fut: F) -> task::JoinHandle<()>
    where
        F: Future<Output = Result<()>> + Send + 'static,
{
    task::spawn(async move {
        if let Err(e) = fut.await {
            eprintln!("{}", e)
        }
    })
}
