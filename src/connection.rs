use crate::writer::Writer;
use async_std::io;
use crate::rpcframe::{Protocol, RpcFrame};
use futures::{AsyncReadExt, AsyncWriteExt};
use log::*;
use crate::{ChainPackWriter, MetaMap, RpcMessage, RpcMessageMetaTags, RpcValue};
use crate::rpcmessage::{RpcError, RpcErrorCode, RqId};
// use log::*;

//pub trait Reader = async_std::io::Write + std::marker::Unpin;
// pub async fn send_frame<W: io::Write + std::marker::Unpin>(writer: &mut W, frame: RpcFrame) -> crate::Result<()> {
//     log!(target: "RpcMsg", Level::Debug, "S<== {}", &frame.to_rpcmesage().unwrap_or_default());
//     let mut meta_data = Vec::new();
//     {
//         let mut wr = ChainPackWriter::new(&mut meta_data);
//         wr.write_meta(&frame.meta)?;
//     }
//     let mut header = Vec::new();
//     let mut wr = ChainPackWriter::new(&mut header);
//     let msg_len = 1 + meta_data.len() + frame.data.len();
//     wr.write_uint_data(msg_len as u64)?;
//     header.push(frame.protocol as u8);
//     writer.write(&header).await?;
//     writer.write(&meta_data).await?;
//     writer.write(&frame.data).await?;
//     // Ensure the encoded frame is written to the socket. The calls above
//     // are to the buffered stream and writes. Calling `flush` writes the
//     // remaining contents of the buffer to the socket.
//     writer.flush().await?;
//     Ok(())
// }
// pub async fn send_message<W: async_std::io::Write + std::marker::Unpin>(writer: &mut W, msg: &RpcMessage) -> crate::Result<()> {
//     let frame = RpcFrame::from_rpcmessage(Protocol::ChainPack, &msg)?;
//     send_frame(writer, frame).await?;
//     Ok(())
// }

pub struct FrameReader<'a, R: async_std::io::Read + std::marker::Unpin> {
    buffer: Vec<u8>,
    reader: &'a mut R,
}

impl<'a, R: io::Read + std::marker::Unpin> FrameReader<'a, R> {
    pub fn new(reader: &'a mut R) -> Self {
        Self {
            buffer: vec![],
            reader
        }
    }

    pub async fn receive_frame(&mut self) -> crate::Result<Option<RpcFrame>> {
        loop {
            while !&self.buffer.is_empty() {
                let buff = &self.buffer[..];
                // trace!("parsing: {:?}", buff);
                match RpcFrame::parse(buff) {
                    Ok(maybe_frame) => {
                        match maybe_frame {
                            None => { break; }
                            Some((frame_len, frame)) => {
                                self.buffer = self.buffer[frame_len..].to_vec();
                                log!(target: "RpcMsg", Level::Debug, "R==> {}", &frame.to_rpcmesage().unwrap_or_default());
                                return Ok(Some(frame))
                            }
                        }
                    }
                    Err(_) => {
                        // invalid frame will be silently ignored, data will be thrown away
                        self.buffer.clear();
                    }
                }
            }
            let mut buff = [0u8; 1024];
            match self.reader.read(&mut buff).await {
                Ok(0) => {
                    // end of stream
                    return Ok(None);
                }
                Ok(n) => {
                    // trace!("{:?}", &buff[.. n]);
                    self.buffer.extend_from_slice(&buff[.. n]);
                    // trace!("self.buffer: {:?}", &self.buffer);
                },
                Err(e) => return Err(format!("Read error: {}", &e).into())
            };
        }
    }

    pub async fn receive_message(&mut self) -> crate::Result<Option<RpcMessage>> {
        match self.receive_frame().await? {
            None => { return Ok(None); }
            Some(frame) => {
                let msg = frame.to_rpcmesage()?;
                return Ok(Some(msg));
            }
        }
    }
}




pub struct FrameWriter<'a, W: async_std::io::Write + std::marker::Unpin> {
    writer: &'a mut W,
}
impl<'a, W: io::Write + std::marker::Unpin> FrameWriter<'a, W> {
    pub fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
        }
    }

    pub async fn send_frame(&mut self, frame: RpcFrame) -> crate::Result<()> {
        log!(target: "RpcMsg", Level::Debug, "S<== {}", &frame.to_rpcmesage().unwrap_or_default());
        let mut meta_data = Vec::new();
        {
            let mut wr = ChainPackWriter::new(&mut meta_data);
            wr.write_meta(&frame.meta)?;
        }
        let mut header = Vec::new();
        let mut wr = ChainPackWriter::new(&mut header);
        let msg_len = 1 + meta_data.len() + frame.data.len();
        wr.write_uint_data(msg_len as u64)?;
        header.push(frame.protocol as u8);
        self.writer.write(&header).await?;
        self.writer.write(&meta_data).await?;
        self.writer.write(&frame.data).await?;
        // Ensure the encoded frame is written to the socket. The calls above
        // are to the buffered stream and writes. Calling `flush` writes the
        // remaining contents of the buffer to the socket.
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn send_message(&mut self, msg: RpcMessage) -> crate::Result<()> {
        let frame = RpcFrame::from_rpcmessage(Protocol::ChainPack, &msg)?;
        self.send_frame(frame).await?;
        Ok(())
    }
    pub async fn send_error(&mut self, meta: MetaMap, errmsg: &str) -> crate::Result<()> {
        let mut msg = RpcMessage::from_meta(meta);
        msg.set_error(RpcError{ code: RpcErrorCode::MethodCallException, message: errmsg.into()});
        self.send_message(msg).await
    }
    pub async fn send_result(&mut self, meta: MetaMap, result: RpcValue) -> crate::Result<()> {
        let mut msg = RpcMessage::from_meta(meta);
        msg.set_result(result);
        self.send_message(msg).await
    }
    pub async fn send_request(&mut self, shv_path: &str, method: &str, param: Option<RpcValue>) -> crate::Result<RqId> {
        let rpcmsg = RpcMessage::new_request(shv_path, method, param);
        let rqid = rpcmsg.request_id().expect("Request ID should exist here.");
        self.send_message(rpcmsg).await?;
        Ok(rqid)
    }

}


