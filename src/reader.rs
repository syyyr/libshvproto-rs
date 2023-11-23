use std::fmt::{Display, Formatter};
use std::io::Read;
use crate::{MetaMap, RpcValue};
use crate::rpcvalue::Value;

#[derive(Debug)]
pub struct ReadError {
    pub msg: String,
    pub pos: usize,
    pub line: usize,
    pub col: usize,
}

impl Display for ReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ReadError: {}, pos {}, line: {}, col: {}", self.msg, self.pos, self.line, self.col)
    }
}

impl std::error::Error for ReadError {}

pub(crate) struct ByteReader<'a, R>
{
    pub read: &'a mut R,
    peeked: Option<u8> ,
    new_line_read: bool,
    pub pos: usize,
    line: usize,
    col: usize,
}

impl<'a, R> ByteReader<'a, R>
where R: Read
{
    pub(crate) fn new(read: &'a mut R) -> ByteReader<'a, R> {
        ByteReader {
            read,
            peeked: None,
            new_line_read: false,
            pos: 0,
            line: 0,
            col: 0,
        }
    }

    pub(crate) fn peek_byte(&mut self) -> u8 {
        if let Some(b) = self.peeked {
            return b
        }
        let mut arr: [u8; 1] = [0];
        let r = self.read.read(&mut arr);
        match r {
            Ok(n) => {
                if n == 0 {
                    return 0
                }
                self.peeked = Some(arr[0]);
                arr[0]
            }
            _ => 0
        }
    }
    pub(crate) fn get_byte(&mut self) -> Result<u8, ReadError> {
        let ret_b;
        if let Some(b) = self.peeked {
            self.peeked = None;
            ret_b = b;
        } else {
            let mut arr: [u8; 1] = [0];
            let r = self.read.read(&mut arr);
            match r {
                Ok(n) => {
                    if n == 0 {
                        return Err(self.make_error("Unexpected end of stream."))
                    }
                    ret_b = arr[0];
                }
                Err(e) => return Err(self.make_error(&e.to_string()))
            }
        }
        self.pos += 1;
        if self.new_line_read {
            self.new_line_read = false;
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
        if ret_b == b'\n' {
            self.new_line_read = true;
        }
        Ok(ret_b)
    }

    pub(crate) fn make_error(&self, msg: &str) -> ReadError {
        ReadError { msg: msg.to_string(), pos: self.pos, line: self.line, col: self.col }
    }
}

pub type ReadResult = Result<RpcValue, ReadError>;
//pub type ReadValueResult = Result<Value, ReadError>;

pub trait Reader {
    fn read(&mut self) -> ReadResult {
        let m = self.try_read_meta()?;
        let v = self.read_value()?;
        let rv = RpcValue::new(v, m);
        return Ok(rv)
    }
    fn try_read_meta(&mut self) -> Result<Option<MetaMap>, ReadError>;
    fn read_value(&mut self) -> Result<Value, ReadError>;
}
