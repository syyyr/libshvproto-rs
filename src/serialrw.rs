use std::io::BufReader;
use async_trait::async_trait;
use crc::CRC_32_ISO_HDLC;
use crate::writer::Writer;
use crate::rpcframe::{Protocol, RpcFrame};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use log::*;
use crate::{ChainPackReader, ChainPackWriter, Reader};
use crate::framerw::{FrameReader, FrameWriter};

const STX: u8 = 0xA2;
const ETX: u8 = 0xA;
const ATX: u8 = 0xA4;
const ESC: u8 = 0xAA;

const ESTX: u8 = 0x02;
const EETX: u8 = 0x03;
const EATX: u8 = 0x04;
const EESC: u8 = 0x0A;
enum Byte {
    Data(u8),
    Stx,
    Etx,
    Atx,
    FramingError(u8),
}
pub struct SerialFrameReader<R: AsyncRead + Unpin + Send> {
    reader: R,
    with_crc: bool,
}
impl<R: AsyncRead + Unpin + Send> SerialFrameReader<R> {
    pub fn new(reader: R, with_crc: bool) -> Self {
        Self {
            reader,
            with_crc,
        }
    }
    async fn get_byte(&mut self) -> crate::Result<u8> {
        let mut buff = [0u8; 1];
        let n = self.reader.read(&mut buff[..]).await?;
        assert!(n == 1);
        Ok(buff[0])
    }
    async fn get_escaped_byte(&mut self) -> crate::Result<crate::serialrw::Byte> {
        match self.get_byte().await? {
            STX => {
                return Ok(crate::serialrw::Byte::Stx);
            }
            ETX => {
                return Ok(crate::serialrw::Byte::Etx);
            }
            ATX => {
                return Ok(crate::serialrw::Byte::Atx);
            }
            ESC => {
                match self.get_byte().await? {
                    ESTX => { return Ok(crate::serialrw::Byte::Data(STX)) }
                    EETX => { return Ok(crate::serialrw::Byte::Data(ETX)) }
                    EATX => { return Ok(crate::serialrw::Byte::Data(ATX)) }
                    EESC => { return Ok(crate::serialrw::Byte::Data(ESC)) }
                    b => {
                        warn!("Framing error, invalid escape byte {}", b);
                        return Ok(crate::serialrw::Byte::FramingError(b));
                    }
                }
            }
            b => { return Ok(crate::serialrw::Byte::Data(b)) }
        }
    }
}
#[async_trait]
impl<R: AsyncRead + Unpin + Send> FrameReader for SerialFrameReader<R> {
    async fn receive_frame(&mut self) -> crate::Result<RpcFrame> {
        let mut has_stx = false;
        'read_frame: loop {
            if !has_stx {
                loop {
                    match self.get_escaped_byte().await? {
                        Byte::Stx => { break }
                        _ => { continue }
                    }
                }
            }
            has_stx = false;
            let mut data: Vec<u8> = vec![];
            loop {
                match self.get_escaped_byte().await? {
                    Byte::Stx => {
                        has_stx = true;
                        continue 'read_frame
                    }
                    Byte::Data(b) => { data.push(b) }
                    Byte::Etx => { break }
                    _ => { continue 'read_frame }
                }
            }
            let mut crc_data = [0u8; 4];
            for i in 0..4 {
                match self.get_escaped_byte().await? {
                    Byte::Stx => {
                        has_stx = true;
                        continue 'read_frame
                    }
                    Byte::Data(b) => { crc_data[i] = b }
                    _ => { continue 'read_frame }
                }
            }
            if self.with_crc {
                fn as_u32_be(array: &[u8; 4]) -> u32 {
                    ((array[0] as u32) << 24) +
                        ((array[1] as u32) << 16) +
                        ((array[2] as u32) <<  8) +
                        ((array[3] as u32) <<  0)
                }
                let crc1 = as_u32_be(&crc_data);
                let gen = crc::Crc::<u32>::new(&CRC_32_ISO_HDLC);
                let crc2 = gen.checksum(&data);
                if crc1 != crc2 {
                    log!(target: "Serial", Level::Debug, "CRC error");
                    continue 'read_frame
                }
            }
            let protocol = data[0];
            if protocol != Protocol::ChainPack as u8 {
                log!(target: "Serial", Level::Debug, "Not chainpack message");
                continue 'read_frame
            }
            let mut buffrd = BufReader::new(&data[1 ..]);
            let mut rd = ChainPackReader::new(&mut buffrd);
            if let Ok(Some(meta)) = rd.try_read_meta() {
                let pos = rd.position() + 1;
                let data: Vec<_> = data.drain(pos .. ).collect();
                return Ok(RpcFrame { protocol: Protocol::ChainPack, meta, data })
            } else {
                log!(target: "Serial", Level::Debug, "Meta data read error");
                continue 'read_frame
            }
        }
    }
}

pub struct SerialFrameWriter<W: AsyncWrite + Unpin + Send> {
    writer: W,
    with_crc: bool,
}
impl<W: AsyncWrite + Unpin + Send> SerialFrameWriter<W> {
    pub fn new(writer: W, with_crc: bool) -> Self {
        Self {
            writer,
            with_crc,
        }
    }
    async fn write_escaped(&mut self, digest: &mut crc::Digest<'_, u32>, data: &[u8]) -> crate::Result<()> {
        for b in data {
            match *b {
                STX => { let data = [ESC, ESTX]; digest.update(&data);  self.writer.write(&data).await? }
                ETX => { let data = [ESC, EETX]; digest.update(&data);  self.writer.write(&data).await? }
                ATX => { let data = [ESC, EATX]; digest.update(&data);  self.writer.write(&data).await? }
                ESC => { let data = [ESC, EESC]; digest.update(&data);  self.writer.write(&data).await? }
                b => { let data = [b]; digest.update(&data);  self.writer.write(&data).await? }
            };
        }
        Ok(())
    }
}
#[async_trait]
impl<W: AsyncWrite + Unpin + Send> FrameWriter for SerialFrameWriter<W> {
    async fn send_frame(&mut self, frame: RpcFrame) -> crate::Result<()> {
        log!(target: "RpcMsg", Level::Debug, "S<== {}", &frame.to_rpcmesage().unwrap_or_default());
        let gen = crc::Crc::<u32>::new(&CRC_32_ISO_HDLC);
        let mut digest = gen.digest();
        let mut meta_data = Vec::new();
        {
            let mut wr = ChainPackWriter::new(&mut meta_data);
            wr.write_meta(&frame.meta)?;
        }
        self.writer.write(&[STX]).await?;
        let protocol = [Protocol::ChainPack as u8];
        self.writer.write(&protocol).await?;
        digest.update(&protocol);
        self.write_escaped(&mut digest, &meta_data).await?;
        self.write_escaped(&mut digest, &frame.data).await?;
        self.writer.write(&[ETX]).await?;
        if self.with_crc {
            fn u32_to_bytes(x:u32) -> [u8;4] {
                let b1 : u8 = ((x >> 24) & 0xff) as u8;
                let b2 : u8 = ((x >> 16) & 0xff) as u8;
                let b3 : u8 = ((x >> 8) & 0xff) as u8;
                let b4 : u8 = (x & 0xff) as u8;
                return [b4, b3, b2, b1]
            }
            let crc_bytes = u32_to_bytes(digest.finalize());
            let mut digest = gen.digest();
            self.write_escaped(&mut digest, &crc_bytes).await?;
        }
        // Ensure the encoded frame is written to the socket. The calls above
        // are to the buffered stream and writes. Calling `flush` writes the
        // remaining contents of the buffer to the socket.
        self.writer.flush().await?;
        Ok(())
    }
}


