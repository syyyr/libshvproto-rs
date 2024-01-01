use std::fmt;
use std::io::{Cursor, BufReader};
// use tracing::{instrument};
use bytes::Buf;
use crate::{ChainPackReader, ChainPackWriter, CponReader, MetaMap, RpcMessage, RpcMessageMetaTags, rpctype, RpcValue};
use crate::writer::Writer;
use crate::reader::Reader;
use log::*;

#[derive(Clone, Debug)]
pub struct RpcFrame {
    pub protocol: Protocol,
    pub meta: MetaMap,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub enum Protocol {
    ChainPack = 1,
    Cpon,
}
impl fmt::Display for Protocol {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Protocol::ChainPack => write!(fmt, "{}", "ChainPack"),
            Protocol::Cpon => write!(fmt, "{}", "Cpon"),
        }
    }
}
impl RpcFrame {
    pub fn new(protocol: Protocol, meta: MetaMap, data: Vec<u8>) -> RpcFrame {
        RpcFrame { protocol, meta, data }
    }
    pub fn from_rpcmessage(msg: RpcMessage) -> crate::Result<RpcFrame> {
        let mut data = Vec::new();
        {
            let mut wr = ChainPackWriter::new(&mut data);
            wr.write_value(&msg.as_rpcvalue().value())?;
        }
        let meta = msg.as_rpcvalue().meta().clone();
        Ok(RpcFrame { protocol: Protocol::ChainPack, meta, data })
    }
    pub fn to_rpcmesage(&self) -> crate::Result<RpcMessage> {
        let mut buff = BufReader::new(&*self.data);
        let value;
        match &self.protocol {
            Protocol::ChainPack => {
                let mut rd = ChainPackReader::new(&mut buff);
                value = rd.read_value()?;
            }
            Protocol::Cpon => {
                let mut rd = CponReader::new(&mut buff);
                value = rd.read_value()?;
            }
        }
        Ok(RpcMessage::from_rpcvalue(RpcValue::new(value, Some(self.meta.clone())))?)
    }

    /// The message has already been validated with `check`.
    pub fn parse(buff: &[u8]) -> crate::Result<Option<(usize, RpcFrame)>> {
        // min RpcMessage must have at least 6 bytes
        const MIN_LEN: usize = 6;
        let buff_len = buff.len();
        if buff_len < MIN_LEN {
            return Ok(None)
        }
        let mut buff_cursor = Cursor::new(buff);
        let mut cpk_rd = ChainPackReader::new(&mut buff_cursor);
        let msg_len = cpk_rd.read_uint_data()? as usize;
        let header_len = buff_cursor.position() as usize;
        let frame_len = header_len + msg_len;
        trace!("msg_len: {}, header_len: {}, frame_len: {}", msg_len, header_len, frame_len);
        if buff_len < frame_len {
            return Ok(None)
        }
        let proto = buff_cursor.get_u8();
        let protocol;
        let meta;
        if proto == Protocol::ChainPack as u8 {
            protocol = Protocol::ChainPack;
            let mut rd = ChainPackReader::new(&mut buff_cursor);
            meta = rd.try_read_meta()?.unwrap();
        } else if proto == Protocol::Cpon as u8 {
            protocol = Protocol::Cpon;
            let mut rd = CponReader::new(&mut buff_cursor);
            meta = rd.try_read_meta()?.unwrap();
        } else {
            return Err(format!("Invalid protocol: {}!", proto).into())
        }
        let pos = buff_cursor.position() as usize;
        // debug!("parse pos2: {}", pos);
        // debug!("parse data len: {}", (frame_len - pos));
        let data: Vec<u8> = buff[pos .. frame_len].into();
        return Ok(Some((frame_len, RpcFrame { protocol, meta, data })))
    }
    pub fn prepare_response_meta(src: &MetaMap) -> Result<MetaMap, &'static str> {
        if src.is_request() {
            if let Some(rqid) = src.request_id() {
                let mut dest = MetaMap::new();
                dest.insert(rpctype::Tag::MetaTypeId as i32, RpcValue::from(rpctype::GlobalNS::MetaTypeID::ChainPackRpcMessage as i32));
                dest.set_request_id(rqid);
                dest.set_caller_ids(&src.caller_ids());
                return Ok(dest)
            }
            return Err("Request ID is missing")
        }
        Err("Not RPC Request")
    }
}
impl fmt::Display for RpcFrame {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{{proto:{}, meta:{}, data len: {}}}", self.protocol, self.meta, self.data.len())
    }
}

impl RpcMessageMetaTags for RpcFrame {
    type Target = RpcFrame;

    fn tag(&self, id: i32) -> Option<&RpcValue> {
        self.meta.tag(id)
    }
    fn set_tag(&mut self, id: i32, val: Option<RpcValue>) -> &mut Self::Target {
        self.meta.set_tag(id, val);
        self
    }
}