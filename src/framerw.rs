use async_trait::async_trait;
use crate::rpcframe::{RpcFrame};
use crate::{MetaMap, RpcMessage, RpcMessageMetaTags, RpcValue};
use crate::rpcmessage::{RpcError, RpcErrorCode, RqId};
#[async_trait]
pub trait FrameReader {
    async fn receive_frame(&mut self) -> crate::Result<Option<RpcFrame>>;

    async fn receive_message(&mut self) -> crate::Result<Option<RpcMessage>> {
        match self.receive_frame().await? {
            None => { return Ok(None); }
            Some(frame) => {
                let msg = frame.to_rpcmesage()?;
                return Ok(Some(msg));
            }
        }
    }
}

#[async_trait]
pub trait FrameWriter {
    async fn send_frame(&mut self, frame: RpcFrame) -> crate::Result<()>;
    async fn send_message(&mut self, msg: RpcMessage) -> crate::Result<()> {
        let frame = RpcFrame::from_rpcmessage(msg)?;
        self.send_frame(frame).await?;
        Ok(())
    }
    async fn send_error(&mut self, meta: MetaMap, errmsg: &str) -> crate::Result<()> {
        let mut msg = RpcMessage::from_meta(meta);
        msg.set_error(RpcError{ code: RpcErrorCode::MethodCallException, message: errmsg.into()});
        self.send_message(msg).await
    }
    async fn send_result(&mut self, meta: MetaMap, result: RpcValue) -> crate::Result<()> {
        let mut msg = RpcMessage::from_meta(meta);
        msg.set_result(result);
        self.send_message(msg).await
    }
    async fn send_request(&mut self, shv_path: &str, method: &str, param: Option<RpcValue>) -> crate::Result<RqId> {
        let rpcmsg = RpcMessage::new_request(shv_path, method, param);
        let rqid = rpcmsg.request_id().expect("Request ID should exist here.");
        self.send_message(rpcmsg).await?;
        Ok(rqid)
    }
}


