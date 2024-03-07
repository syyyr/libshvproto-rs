#[cfg(feature = "async-std")]
pub mod broker;
pub mod chainpack;
pub mod client;
pub mod cpon;
pub mod datetime;
pub mod decimal;
// #[cfg(feature = "async-std")]
#[cfg(feature = "async-std")]
pub mod device;
pub mod framerw;
pub mod metamap;
pub mod metamethod;
pub mod reader;
pub mod rpc;
pub mod rpcframe;
pub mod rpcmessage;
pub mod rpctype;
pub mod rpcvalue;
pub mod serialrw;
pub mod shvnode;
pub mod streamrw;
pub mod util;
pub mod writer;

pub use datetime::DateTime;
pub use decimal::Decimal;
pub use metamap::MetaMap;
pub use reader::{ReadError, ReadResult, Reader};
pub use rpcmessage::{RpcMessage, RpcMessageMetaTags};
pub use rpcvalue::Value;
pub use rpcvalue::{Blob, List, Map, RpcValue};
pub use writer::{WriteResult, Writer};

pub use chainpack::{ChainPackReader, ChainPackWriter};
pub use cpon::{CponReader, CponWriter};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "async-std")]
mod spawn {
    use log::error;
    use std::future::Future;
    use async_std::task;
    pub fn spawn_and_log_error<F>(fut: F) -> task::JoinHandle<()>
        where
            F: Future<Output = crate::Result<()>> + Send + 'static,
    {
        task::spawn(async move {
            if let Err(e) = fut.await {
                error!("{}", e)
            }
        })
    }
}
#[cfg(feature = "async-std")]
pub use spawn::spawn_and_log_error;
