use std::io::Write;
use crate::{RpcValue, Value, MetaMap};

pub type WriteResult = std::io::Result<usize>;

pub(crate) struct ByteWriter<'a, W>
{
    write: &'a mut W,
    cnt: usize,
}

impl<'a, W> ByteWriter<'a, W>
    where W: Write
{
    pub(crate) fn new(write: &'a mut W) -> Self {
        ByteWriter {
            write,
            cnt: 0,
        }
    }
    pub(crate) fn count(&self) -> usize { self.cnt }
    pub(crate) fn write_byte(&mut self, b: u8) -> WriteResult {
        let arr: [u8; 1] = [b];
        let n = self.write.write(&arr)?;
        self.cnt += n;
        Ok(n)
    }
    pub(crate) fn write_bytes(&mut self, b: &[u8]) -> WriteResult {
        let n = self.write.write(b)?;
        self.cnt += n;
        Ok(n)
    }
}

pub trait Writer {
    fn write(&mut self, rv: &RpcValue) -> WriteResult;
    fn write_meta(&mut self, m: &MetaMap) -> WriteResult;
    fn write_value(&mut self, v: &Value) -> WriteResult;
}