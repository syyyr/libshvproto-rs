use crate::writer::Writer;
use async_std::io;
use crate::rpcframe::{Protocol, RpcFrame};
use futures::{AsyncReadExt, AsyncWriteExt};
use log::*;
use sha1::{Sha1, Digest};
use crate::{ChainPackWriter, RpcMessage};
// use log::*;

//pub trait Reader = async_std::io::Write + std::marker::Unpin;
pub async fn send_frame<W: io::Write + std::marker::Unpin>(writer: &mut W, frame: RpcFrame) -> crate::Result<()> {
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
    writer.write(&header).await?;
    writer.write(&meta_data).await?;
    writer.write(&frame.data).await?;
    // Ensure the encoded frame is written to the socket. The calls above
    // are to the buffered stream and writes. Calling `flush` writes the
    // remaining contents of the buffer to the socket.
    writer.flush().await?;
    Ok(())
}
pub async fn send_message<W: async_std::io::Write + std::marker::Unpin>(writer: &mut W, msg: &RpcMessage) -> crate::Result<()> {
    let frame = RpcFrame::from_rpcmessage(Protocol::ChainPack, &msg)?;
    send_frame(writer, frame).await?;
    Ok(())
}

pub struct FrameReader<'a, R: async_std::io::Read + std::marker::Unpin> {
    buffer: Vec<u8>,
    reader: &'a mut R,
}

impl<'a, R: io::Read + std::marker::Unpin> FrameReader<'a, R> {
    pub fn new(reader: &'a mut R) -> FrameReader<'a, R> {
        FrameReader {
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




/*
pub async fn call_rpc_method(&self, request: RpcMessage) -> crate::Result<RpcMessage> {
    if !request.is_request() {
        return Err("Not request".into());
    }
    let rq_id = request.request_id().ok_or("Request ID missing")?;
    trace!("sending RPC request id: {} msg: {}", rq_id, request);
    self.send_message(&request).await?;
    let mut client = self.clone();
    match future::timeout(
        Duration::from_millis(DEFAULT_RPC_CALL_TIMEOUT_MS),
        async move {
            loop {
                let frame = client.receive_frame().await?;
                trace!(target: "rpcmsg", "{} maybe response: {}", rq_id, frame);
                if let Some(id) = frame.request_id() {
                    if id == rq_id {
                        let resp = frame.to_rpcmesage()?;
                        trace!("{} .............. got response: {}", rq_id, resp);
                        return Ok(resp);
                    }
                }
            }
        },
    )
        .await
    {
        Ok(resp) => resp,
        Err(_) => Err(format!(
            "Response to request id: {} didn't arrive within {} msec.",
            rq_id, DEFAULT_RPC_CALL_TIMEOUT_MS
        )
            .into()),
    }
}

 */
pub fn sha1_hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    return hex::encode(&result[..]).as_bytes().to_vec();
}
pub fn sha1_password_hash(password: &[u8], nonce: &[u8]) -> Vec<u8> {
    let mut hash = sha1_hash(password);
    let mut nonce_pass= nonce.to_vec();
    nonce_pass.append(&mut hash);
    return sha1_hash(&nonce_pass);
}
