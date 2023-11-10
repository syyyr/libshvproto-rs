use crate::{RpcValue, MetaMap, metamap::MetaKey, Decimal, DateTime, WriteResult, Value};
use std::io;
use crate::writer::{ByteWriter, Writer};
use std::io::{Write, Read};
use std::collections::BTreeMap;
use crate::reader::{Reader, ByteReader, ReadError};
use crate::rpcvalue::{Map, IMap};

#[warn(non_camel_case_types)]
#[allow(dead_code)]
pub(crate) enum PackingSchema {
    Null = 128,
    UInt,
    Int,
    Double,
    Bool,
    Blob,
    String,
    DateTimeEpochDepricated, // deprecated
    List,
    Map,
    IMap,
    MetaMap,
    Decimal,
    DateTime,
    CString,
    FALSE = 253,
    TRUE = 254,
    TERM = 255,
}

const SHV_EPOCH_MSEC: i64 = 1517529600000;

pub struct ChainPackWriter<'a, W>
    where W: Write
{
    byte_writer: ByteWriter<'a, W>,
}

impl<'a, W> ChainPackWriter<'a, W>
    where W: 'a + io::Write
{
    pub fn new(write: &'a mut W) -> Self {
        ChainPackWriter {
            byte_writer: ByteWriter::new(write),
        }
    }

    fn write_byte(&mut self, b: u8) -> WriteResult {
        self.byte_writer.write_byte(b)
    }
    fn write_bytes(&mut self, b: &[u8]) -> WriteResult {
        self.byte_writer.write_bytes(b)
    }

    /// see https://en.wikipedia.org/wiki/Find_first_set#CLZ
    fn significant_bits_part_length(num: u64) -> u32 {
        let mut len = 0;
        let mut n = num;
        if (n & 0xFFFFFFFF00000000) != 0 {
            len += 32;
            n >>= 32;
        }
        if (n & 0xFFFF0000) != 0 {
            len += 16;
            n >>= 16;
        }
        if (n & 0xFF00) != 0 {
            len += 8;
            n >>= 8;
        }
        if (n & 0xF0) != 0 {
            len += 4;
            n >>= 4;
        }
        const SIG_TABLE_4BIT: [u8; 16] =  [ 0, 1, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4 ];
        len += SIG_TABLE_4BIT[n as usize];
        return len as u32
    }
    /// number of bytes needed to encode bit_len
    fn bytes_needed(bit_len: u32) -> u32 {
        let cnt;
        if bit_len == 0 {
            cnt = 1;
        } else if bit_len <= 28 {
            cnt = (bit_len - 1) / 7 + 1;
        } else {
            cnt = (bit_len - 1) / 8 + 2;
        }
        return cnt
    }
    /// return max bit length >= bit_len, which can be encoded by same number of bytes
    fn expand_bit_len(bit_len: u32) -> u32 {
        let byte_cnt = Self::bytes_needed(bit_len);
        if bit_len <= 28 {
            byte_cnt * (8 - 1) - 1
        } else {
            (byte_cnt - 1) * 8 - 1
        }
    }
    /** UInt
    0 ...  7 bits  1  byte  |0|x|x|x|x|x|x|x|<-- LSB
    8 ... 14 bits  2  bytes |1|0|x|x|x|x|x|x| |x|x|x|x|x|x|x|x|<-- LSB
    15 ... 21 bits  3  bytes |1|1|0|x|x|x|x|x| |x|x|x|x|x|x|x|x| |x|x|x|x|x|x|x|x|<-- LSB
    22 ... 28 bits  4  bytes |1|1|1|0|x|x|x|x| |x|x|x|x|x|x|x|x| |x|x|x|x|x|x|x|x| |x|x|x|x|x|x|x|x|<-- LSB
    29+       bits  5+ bytes |1|1|1|1|n|n|n|n| |x|x|x|x|x|x|x|x| |x|x|x|x|x|x|x|x| |x|x|x|x|x|x|x|x| ... <-- LSB
                    n ==  0 ->  4 bytes number (32 bit number)
                    n ==  1 ->  5 bytes number
                    n == 14 -> 18 bytes number
                    n == 15 -> for future (number of bytes will be specified in next byte)
    */
    fn write_uint_data_helper(&mut self, number: u64, bit_len: u32) -> WriteResult {
        const BYTE_CNT_MAX: u32 = 32;
        let byte_cnt = Self::bytes_needed(bit_len);
        assert!(byte_cnt <= BYTE_CNT_MAX, "Max int byte size {} exceeded", BYTE_CNT_MAX);
        let mut bytes: [u8; BYTE_CNT_MAX as usize] = [0; BYTE_CNT_MAX as usize];
        let mut num = number;
        for i in (0 .. byte_cnt).rev() {
            let r = (num & 255) as u8;
            bytes[i as usize] = r;
            num = num >> 8;
        }
        if bit_len <= 28 {
            let mut mask = 0xf0 << (4 - byte_cnt);
            bytes[0] = bytes[0] & ((!mask) as u8);
            mask <<= 1;
            bytes[0] |= mask;
        }
        else {
            bytes[0] = (0xf0 | (byte_cnt - 5)) as u8;
        }
        let cnt = self.byte_writer.count();
        for i in 0 .. byte_cnt {
            let r = bytes[i as usize];
            self.write_byte(r)?;
        }
        return Ok(self.byte_writer.count() - cnt)
    }
    pub fn write_uint_data(&mut self, number: u64) -> WriteResult {
        let bitlen = Self::significant_bits_part_length(number);
        let cnt = self.write_uint_data_helper(number, bitlen)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_int_data(&mut self, number: i64) -> WriteResult {
        let mut num;
        let neg;
        if number < 0 {
            num = (-number) as u64;
            neg = true;
        } else {
            num = number as u64;
            neg = false;
        };

        let bitlen = Self::significant_bits_part_length(num as u64) + 1; // add sign bit
        if neg {
            let sign_pos = Self::expand_bit_len(bitlen);
            let sign_bit_mask = (1 as u64) << sign_pos;
            num |= sign_bit_mask;
        }
        let cnt = self.write_uint_data_helper(num as u64, bitlen)?;
        Ok(self.byte_writer.count() - cnt)
    }

    fn write_int(&mut self, n: i64) -> WriteResult {
        let cnt = self.byte_writer.count();
        if n >= 0 && n < 64 {
            self.write_byte(((n % 64) + 64) as u8)?;
        }
        else {
            self.write_byte(PackingSchema::Int as u8)?;
            self.write_int_data(n)?;
        }
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_uint(&mut self, n: u64) -> WriteResult {
        let cnt = self.byte_writer.count();
        if n < 64 {
            self.write_byte((n % 64) as u8)?;
        }
        else {
            self.write_byte(PackingSchema::UInt as u8)?;
            self.write_uint_data(n)?;
        }
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_double(&mut self, n: f64) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::Double as u8)?;
        let bytes = n.to_le_bytes();
        self.write_bytes(&bytes)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_decimal(&mut self, decimal: &Decimal) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::Decimal as u8)?;
        let (mantisa, exponent) = decimal.decode();
        self.write_int_data(mantisa)?;
        self.write_int_data(exponent as i64)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_datetime(&mut self, dt: &DateTime) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::DateTime as u8)?;
        let mut msecs = dt.epoch_msec() - SHV_EPOCH_MSEC;
        let offset = (dt.utc_offset() / 60 / 15) & 0x7F;
        let ms = msecs % 1000;
        if ms == 0 {
            msecs /= 1000;
        }
        if offset != 0 {
            msecs <<= 7;
            msecs |= offset as i64;
        }
        msecs <<= 2;
        if offset != 0 {
            msecs |= 1;
        }
        if ms == 0 {
            msecs |= 2;
        }
        self.write_int_data(msecs)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_list(&mut self, lst: &Vec<RpcValue>) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::List as u8)?;
        for v in lst {
            self.write(v)?;
        }
        self.write_byte(PackingSchema::TERM as u8)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_map(&mut self, map: &Map) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::Map as u8)?;
        for (k, v) in map {
            self.write_string(k)?;
            self.write(v)?;
        }
        self.write_byte(PackingSchema::TERM as u8)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_imap(&mut self, map: &IMap) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::IMap as u8)?;
        for (k, v) in map {
            self.write_int(*k as i64)?;
            self.write(v)?;
        }
        self.write_byte(PackingSchema::TERM as u8)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_string(&mut self, s: &str) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::String as u8)?;
        let data = s.as_bytes();
        self.write_uint_data(data.len() as u64)?;
        self.write_bytes(data)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_blob(&mut self, data: &[u8]) -> WriteResult {
        let cnt = self.write_byte(PackingSchema::Blob as u8)?;
        self.write_uint_data(data.len() as u64)?;
        self.write_bytes(data)?;
        Ok(self.byte_writer.count() - cnt)
    }
}

impl<'a, W> Writer for ChainPackWriter<'a, W>
    where W: io::Write
{
    fn write_meta(&mut self, map: &MetaMap) -> WriteResult {
        let cnt = self.byte_writer.count();
        self.write_byte(PackingSchema::MetaMap as u8)?;
        for k in map.0.iter() {
            match &k.key {
                MetaKey::Str(s) => self.write_string(s)?,
                MetaKey::Int(i) => self.write_int(*i as i64)?,
            };
            self.write(&k.value)?;
        }
        self.write_byte(PackingSchema::TERM as u8)?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write(&mut self, val: &RpcValue) -> WriteResult {
        let cnt = self.byte_writer.count();
        let mm = val.meta();
        if !mm.is_empty() {
            self.write_meta(mm)?;
        }
        self.write_value(val.value())?;
        Ok(self.byte_writer.count() - cnt)
    }
    fn write_value(&mut self, val: &Value) -> WriteResult {
        let cnt = self.byte_writer.count();
        match val {
            Value::Null => self.write_byte(PackingSchema::Null as u8)?,
            Value::Bool(b) => if *b {
                self.write_byte(PackingSchema::TRUE as u8)?
            } else {
                self.write_byte(PackingSchema::FALSE as u8)?
            },
            Value::Int(n) => self.write_int(*n)?,
            Value::UInt(n) => self.write_uint(*n)?,
            Value::String(s) => self.write_string(s)?,
            Value::Blob(b) => self.write_blob(b)?,
            Value::Double(n) => self.write_double(*n)?,
            Value::Decimal(d) => self.write_decimal(d)?,
            Value::DateTime(d) => self.write_datetime(d)?,
            Value::List(lst) => self.write_list(lst)?,
            Value::Map(map) => self.write_map(map)?,
            Value::IMap(map) => self.write_imap(map)?,
        };
        Ok(self.byte_writer.count() - cnt)
    }
}

pub struct ChainPackReader<'a, R>
    where R: Read
{
    byte_reader: ByteReader<'a, R>,
}

impl<'a, R> ChainPackReader<'a, R>
    where R: Read
{
    pub fn new(read: &'a mut R) -> Self {
        ChainPackReader { byte_reader: ByteReader::new(read) }
    }

    fn peek_byte(&mut self) -> u8 {
        self.byte_reader.peek_byte()
    }
    fn get_byte(&mut self) -> Result<u8, ReadError> {
        self.byte_reader.get_byte()
    }
    fn make_error(&self, msg: &str) -> ReadError {
        self.byte_reader.make_error(msg)
    }

    /// return (n, bitlen)
    /// bitlen is used to enable same function usage for signed int unpacking
    fn read_uint_data_helper(&mut self) -> Result<(u64, u8), ReadError> {
        let mut num = 0;
        let head = self.get_byte()?;
        let bytes_to_read_cnt;
        let bitlen;
        if (head & 128) == 0 {bytes_to_read_cnt = 0; num = (head & 127) as u64; bitlen = 7;}
        else if (head &  64) == 0 {bytes_to_read_cnt = 1; num = (head & 63) as u64; bitlen = 6 + 8;}
        else if (head &  32) == 0 {bytes_to_read_cnt = 2; num = (head & 31) as u64; bitlen = 5 + 2*8;}
        else if (head &  16) == 0 {bytes_to_read_cnt = 3; num = (head & 15) as u64; bitlen = 4 + 3*8;}
        else {
            bytes_to_read_cnt = (head & 0xf) + 4;
            bitlen = bytes_to_read_cnt * 8;
        }
        for _ in 0 .. bytes_to_read_cnt {
            let r = self.get_byte()?;
            num = (num << 8) + (r as u64);
        }
        Ok((num, bitlen))
    }
    pub fn read_uint_data(&mut self) -> Result<u64, ReadError> {
        let (num, _) = self.read_uint_data_helper()?;
        return Ok(num);
    }
    fn read_int_data(&mut self) -> Result<i64, ReadError> {
        let (num, bitlen) = self.read_uint_data_helper()?;
        let sign_bit_mask = (1 as u64) << (bitlen - 1);
        let neg = (num & sign_bit_mask) != 0;
        let mut snum = num as i64;
        if neg {
            snum &= !(sign_bit_mask as i64);
            snum = -snum;
        }
        return Ok(snum);
    }

    fn read_cstring_data(&mut self) -> Result<Value, ReadError> {
        let mut buff: Vec<u8> = Vec::new();
        loop {
            let b = self.get_byte()?;
            match &b {
                b'\\' => {
                    let b = self.get_byte()?;
                    match &b {
                        b'\\' => buff.push(b'\\'),
                        b'0' => buff.push(b'\0'),
                        _ => buff.push(b),
                    }
                }
                0 => {
                    // end of string
                    break;
                }
                _ => {
                    buff.push(b);
                }
            }
        }
        let s = std::str::from_utf8(&buff);
        match s {
            Ok(s) => return Ok(Value::from(s)),
            Err(e) => return Err(self.make_error(&format!("Invalid string, Utf8 error: {}", e))),
        }
    }
    fn read_string_data(&mut self) -> Result<Value, ReadError> {
        let len = self.read_uint_data()?;
        let mut buff: Vec<u8> = Vec::new();
        for _ in 0 .. len {
            let b = self.get_byte()?;
            buff.push(b);
        }
        let s = std::str::from_utf8(&buff);
        match s {
            Ok(s) => return Ok(Value::from(s)),
            Err(e) => return Err(self.make_error(&format!("Invalid string, Utf8 error: {}", e))),
        }
    }
    fn read_blob_data(&mut self) -> Result<Value, ReadError> {
        let len = self.read_uint_data()?;
        let mut buff: Vec<u8> = Vec::new();
        for _ in 0 .. len {
            let b = self.get_byte()?;
            buff.push(b);
        }
        return Ok(Value::from(buff))
    }
    fn read_list_data(&mut self) -> Result<Value, ReadError> {
        let mut lst = Vec::new();
        loop {
            let b = self.peek_byte();
            if b == PackingSchema::TERM as u8 {
                self.get_byte()?;
                break;
            }
            let val = self.read()?;
            lst.push(val);
        }
        return Ok(Value::from(lst))
    }
    fn read_map_data(&mut self) -> Result<Value, ReadError> {
        let mut map: Map = Map::new();
        loop {
            let b = self.peek_byte();
            if b == PackingSchema::TERM as u8 {
                self.get_byte()?;
                break;
            }
            let k = self.read()?;
            let key;
            if k.is_string() {
                key = k.as_str();
            }
            else {
                return Err(self.make_error(&format!("Invalid Map key '{}'", k)))
            }
            let val = self.read()?;
            map.insert(key.to_string(), val);
        }
        return Ok(Value::from(map))
    }
    fn read_imap_data(&mut self) -> Result<Value, ReadError> {
        let mut map: BTreeMap<i32, RpcValue> = BTreeMap::new();
        loop {
            let b = self.peek_byte();
            if b == PackingSchema::TERM as u8 {
                self.get_byte()?;
                break;
            }
            let k = self.read()?;
            let key;
            if k.is_int() {
                key = k.as_i32();
            }
            else {
                return Err(self.make_error(&format!("Invalid IMap key '{}'", k)))
            }
            let val = self.read()?;
            map.insert(key, val);
        }
        return Ok(Value::from(map))
    }
    fn read_datetime_data(&mut self) -> Result<Value, ReadError> {
        let mut d = self.read_int_data()?;
        let mut offset = 0;
        let has_tz_offset = (d & 1) != 0;
        let has_not_msec = (d & 2) != 0;
        d >>= 2;
        if has_tz_offset {
            offset = (d & 0x7F) as i8;
            offset <<= 1;
            offset >>= 1; // sign extension
            //log::debug!("1----------> offset: {}", offset);
            d >>= 7;
        }
        if has_not_msec {
            d *= 1000;
        }
        d += SHV_EPOCH_MSEC;
        let dt = DateTime::from_epoch_msec_tz(d, (offset as i32 * 15) * 60);
        return Ok(Value::from(dt))
    }
    fn read_double_data(&mut self) -> Result<Value, ReadError> {
        let mut buff: [u8;8] = [0;8];
        if let Err(e) = self.byte_reader.read.read(&mut buff) {
            return Err(self.make_error(&format!("{}", e)))
        }
        let d = f64::from_le_bytes(buff);
        return Ok(Value::from(d))
    }
    fn read_decimal_data(&mut self) -> Result<Value, ReadError> {
        let mantisa = self.read_int_data()?;
        let exponent = self.read_int_data()?;
        let d = Decimal::new(mantisa, exponent as i8);
        return Ok(Value::from(d))
    }
}

impl<'a, R> Reader for ChainPackReader<'a, R>
    where R: Read
{
    fn try_read_meta(&mut self) -> Result<Option<MetaMap>, ReadError> {
        let b = self.peek_byte();
        if b != PackingSchema::MetaMap as u8 {
            return Ok(None)
        }
        self.get_byte()?;
        let mut map = MetaMap::new();
        loop {
            let b = self.peek_byte();
            if b == PackingSchema::TERM as u8 {
                self.get_byte()?;
                break;
            }
            let key = self.read_value()?;
            let val = self.read()?;
            match key {
                Value::Int(i) => {
                    map.insert(i as i32, val);
                }
                Value::String(s) => {
                    map.insert(&**s.clone(), val);
                }
                _ => {
                    return Err(self.make_error(&format!("MetaMap key must be int or string, got: {}", key.type_name())))
                }
            }
        }
        Ok(Some(map))
    }
    fn read_value(&mut self) -> Result<Value, ReadError> {
        let b = self.get_byte()?;
        let v =
            if b < 128 {
                if (b & 64) == 0 {
                    // tiny UInt
                    Value::from((b & 63) as u64)
                }
                else {
                    // tiny Int
                    Value::from((b & 63) as i64)
                }
            } else if b == PackingSchema::Int as u8 {
                let n = self.read_int_data()?;
                Value::from(n)
            } else if b == PackingSchema::UInt as u8 {
                let n = self.read_uint_data()?;
                Value::from(n)
            } else if b == PackingSchema::Double as u8 {
                let n = self.read_double_data()?;
                Value::from(n)
            } else if b == PackingSchema::Decimal as u8 {
                let n = self.read_decimal_data()?;
                Value::from(n)
            } else if b == PackingSchema::DateTime as u8 {
                let n = self.read_datetime_data()?;
                Value::from(n)
            } else if b == PackingSchema::String as u8 {
                let n = self.read_string_data()?;
                Value::from(n)
            } else if b == PackingSchema::CString as u8 {
                let n = self.read_cstring_data()?;
                Value::from(n)
            } else if b == PackingSchema::Blob as u8 {
                let n = self.read_blob_data()?;
                Value::from(n)
            } else if b == PackingSchema::List as u8 {
                let n = self.read_list_data()?;
                Value::from(n)
            } else if b == PackingSchema::Map as u8 {
                let n = self.read_map_data()?;
                Value::from(n)
            } else if b == PackingSchema::IMap as u8 {
                let n = self.read_imap_data()?;
                Value::from(n)
            } else if b == PackingSchema::TRUE as u8 {
                Value::from(true)
            } else if b == PackingSchema::FALSE as u8 {
                Value::from(false)
            } else if b == PackingSchema::Null as u8 {
                Value::from(())
            } else {
                return Err(self.make_error(&format!("Invalid Packing schema: {}", b)))
            };

        Ok(v)
    }
}