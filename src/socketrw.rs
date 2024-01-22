use async_trait::async_trait;
use crate::writer::Writer;
use crate::rpcframe::{RpcFrame};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use log::*;
use crate::{ChainPackWriter};
use crate::framerw::{FrameReader, FrameWriter};

pub struct SocketFrameReader<R: AsyncRead + Unpin + Send> {
    buffer: Vec<u8>,
    reader: R,
}
impl<R: AsyncRead + Unpin + Send> SocketFrameReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            buffer: vec![],
            reader
        }
    }
}
#[async_trait]
impl<R: AsyncRead + Unpin + Send> FrameReader for SocketFrameReader<R> {
    async fn receive_frame(&mut self) -> crate::Result<Option<RpcFrame>> {
        loop {
            while !&self.buffer.is_empty() {
                let buff = &self.buffer[..];
                // trace!("parsing: {:?}", buff);
                match RpcFrame::try_parse_socket_data(buff) {
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
}

pub struct SocketFrameWriter<W: AsyncWrite + Unpin + Send> {
    writer: W,
}
impl<W: AsyncWrite + Unpin + Send> SocketFrameWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
        }
    }
}
#[async_trait]
impl<W: AsyncWrite + Unpin + Send> FrameWriter for SocketFrameWriter<W> {
    async fn send_frame(&mut self, frame: RpcFrame) -> crate::Result<()> {
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
}


