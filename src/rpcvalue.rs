use std::collections::BTreeMap;
use std::fmt;

use crate::{datetime, DateTime, Decimal};
use crate::decimal;
use crate::metamap::MetaMap;
use crate::reader::Reader;
use crate::{CponReader, ReadResult};
use crate::writer::Writer;
use crate::CponWriter;
use crate::chainpack::ChainPackWriter;
use crate::chainpack::ChainPackReader;
use std::convert::From;
use std::sync::OnceLock;

// see https://github.com/rhysd/tinyjson/blob/master/src/json_value.rs


const EMPTY_STR_REF: &str = "";
const EMPTY_BYTES_REF: &[u8] = EMPTY_STR_REF.as_bytes();
static EMPTY_LIST: OnceLock<List> = OnceLock::new();
static EMPTY_MAP: OnceLock<Map> = OnceLock::new();
static EMPTY_IMAP: OnceLock<IMap> = OnceLock::new();
static EMPTY_METAMAP: OnceLock<MetaMap> = OnceLock::new();

#[macro_export(local_inner_macros)]
macro_rules! make_map {
	($( $key: expr => $val: expr ),*) => {{
		 let mut map = rpcvalue::Map::new();
		 $( map.insert($key.to_string(), RpcValue::from($val)); )*
		 map
	}}
}

pub type Blob = Vec<u8>;
pub type List = Vec<RpcValue>;
pub type Map = BTreeMap<String, RpcValue>;
pub type IMap = BTreeMap<i32, RpcValue>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
	Null,
	Int(i64),
	UInt(u64),
	Double(f64),
	Bool(bool),
	DateTime(datetime::DateTime),
	Decimal(decimal::Decimal),
	String(Box<String>),
	Blob(Box<Blob>),
	List(Box<List>),
	Map(Box<Map>),
	IMap(Box<IMap>),
}

impl Value {
	pub fn type_name(&self) -> &'static str {
		match &self {
			Value::Null => "Null",
			Value::Int(_) => "Int",
			Value::UInt(_) => "UInt",
			Value::Double(_) => "Double",
			Value::Bool(_) => "Bool",
			Value::DateTime(_) => "DateTime",
			Value::Decimal(_) => "Decimal",
			Value::String(_) => "String",
			Value::Blob(_) => "Blob",
			Value::List(_) => "List",
			Value::Map(_) => "Map",
			Value::IMap(_) => "IMap",
		}
	}
	pub fn is_default_value(&self) -> bool {
		match &self {
			Value::Null => true,
			Value::Int(i) => *i == 0,
			Value::UInt(u) => *u == 0,
			Value::Double(d) => *d == 0.0,
			Value::Bool(b) => *b == false,
			Value::DateTime(dt) => dt.epoch_msec() == 0,
			Value::Decimal(d) => d.mantissa() == 0,
			Value::String(s) => s.is_empty(),
			Value::Blob(b) => b.is_empty(),
			Value::List(l) => l.is_empty(),
			Value::Map(m) => m.is_empty(),
			Value::IMap(m) => m.is_empty(),
		}
	}
}

impl From<()> for Value { fn from(_: ()) -> Self { Value::Null }}
impl From<bool> for Value { fn from(val: bool) -> Self { Value::Bool(val) }}
impl From<&str> for Value { fn from(val: &str) -> Self { Value::String(Box::new(val.to_string())) }}
impl From<String> for Value { fn from(val: String) -> Self { Value::String(Box::new(val)) }}
impl From<&String> for Value { fn from(val: &String) -> Self { Value::String(Box::new(val.clone())) }}
impl From<Vec<u8>> for Value { fn from(val: Vec<u8>) -> Self { Value::Blob(Box::new(val)) }}
impl From<&[u8]> for Value { fn from(val: &[u8]) -> Self { Value::Blob(Box::new(val.to_vec())) }}
impl From<i32> for Value { fn from(val: i32) -> Self { Value::Int(val.into()) }}
impl From<i64> for Value { fn from(val: i64) -> Self { Value::Int(val) }}
impl From<isize> for Value { fn from(val: isize) -> Self { Value::Int(val as i64) }}
impl From<u32> for Value { fn from(val: u32) -> Self { Value::UInt(val.into()) }}
impl From<u64> for Value { fn from(val: u64) -> Self { Value::UInt(val) }}
impl From<usize> for Value { fn from(val: usize) -> Self { Value::UInt(val as u64) }}
impl From<f64> for Value { fn from(val: f64) -> Self { Value::Double(val as f64) }}
impl From<Decimal> for Value { fn from(val: Decimal) -> Self { Value::Decimal(val) }}
impl From<List> for Value { fn from(val: List) -> Self { Value::List(Box::new(val)) }}
impl From<Map> for Value { fn from(val: Map) -> Self { Value::Map(Box::new(val)) }}
impl From<IMap> for Value { fn from(val: IMap) -> Self { Value::IMap(Box::new(val)) }}
impl From<datetime::DateTime> for Value { fn from(val: datetime::DateTime) -> Self { Value::DateTime(val) }}
impl From<chrono::NaiveDateTime> for Value { fn from(val: chrono::NaiveDateTime) -> Self { Value::DateTime(DateTime::from_naive_datetime(&val)) }}
impl<Tz: chrono::TimeZone> From<chrono::DateTime<Tz>> for Value { fn from(item: chrono::DateTime<Tz>) -> Self { Value::DateTime(datetime::DateTime::from_datetime(&item)) }}

// cannot use generic implementation
//impl<T> From<T> for RpcValue { fn from(val: T) -> Self { RpcValue { meta: None, value: val.into() }}}
// because of error: error[E0119]: conflicting implementations of trait `std::convert::From<rpcvalue::RpcValue>` for type `rpcvalue::RpcValue`
impl From<()> for RpcValue { fn from(val: ()) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<bool> for RpcValue { fn from(val: bool) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<&bool> for RpcValue { fn from(val: &bool) -> Self { RpcValue { meta: None, value: (*val).into() }}}
impl From<&str> for RpcValue { fn from(val: &str) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<String> for RpcValue { fn from(val: String) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<&String> for RpcValue { fn from(val: &String) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<Vec<u8>> for RpcValue { fn from(val: Vec<u8>) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<&[u8]> for RpcValue { fn from(val: &[u8]) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<i32> for RpcValue { fn from(val: i32) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<i64> for RpcValue { fn from(val: i64) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<isize> for RpcValue { fn from(val: isize) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<u32> for RpcValue { fn from(val: u32) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<u64> for RpcValue { fn from(val: u64) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<usize> for RpcValue { fn from(val: usize) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<f64> for RpcValue { fn from(val: f64) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<Decimal> for RpcValue { fn from(val: Decimal) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<List> for RpcValue { fn from(val: List) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<Map> for RpcValue { fn from(val: Map) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<IMap> for RpcValue { fn from(val: IMap) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<datetime::DateTime> for RpcValue { fn from(val: datetime::DateTime) -> Self { RpcValue { meta: None, value: val.into() }}}
impl From<chrono::NaiveDateTime> for RpcValue { fn from(val: chrono::NaiveDateTime) -> Self { RpcValue { meta: None, value: val.into() }}}
impl<Tz: chrono::TimeZone> From<chrono::DateTime<Tz>> for RpcValue {
	fn from(val: chrono::DateTime<Tz>) -> Self {
		RpcValue {
			meta: None,
			value: val.into(),
		}
	}
}
macro_rules! is_xxx {
    ($name:ident, $variant:pat) => {
        pub fn $name(&self) -> bool {
            match self.value() {
                $variant => true,
                _ => false,
            }
        }
    };
}

pub enum GetKey<'a> {
	Int(i32),
	Str(&'a str),
}
pub trait GetIndex {
	fn make_key(&self) -> GetKey;
}
impl GetIndex for &str {
	fn make_key(&self) -> GetKey {
		GetKey::Str(self)
	}
}
impl GetIndex for i32 {
	fn make_key(&self) -> GetKey {
		GetKey::Int(*self)
	}
}
impl GetIndex for u32 {
	fn make_key(&self) -> GetKey {
		GetKey::Int(*self as i32)
	}
}
impl GetIndex for usize {
	fn make_key(&self) -> GetKey {
		GetKey::Int(*self as i32)
	}
}

#[derive(PartialEq, Clone)]
pub struct RpcValue {
	meta: Option<Box<MetaMap>>,
	value: Value
}

impl RpcValue {
	pub fn null() -> RpcValue {
		RpcValue {
			meta: None,
			value: Value::Null,
		}
	}
	pub fn new(v: Value, m: Option<MetaMap>) -> Self {
		RpcValue {
			meta: match m {
				None => None,
				Some(mm) => Some(Box::new(mm)),
			},
			value: v,
		}
	}
	/*
	pub fn new<I>(val: I) -> RpcValue
		where I: FromValue
	{
		RpcValue {
			meta: None,
			value: val.chainpack_make_value(),
		}
	}
	pub fn new_with_meta<I>(val: I, meta: Option<MetaMap>) -> RpcValue
		where I: FromValue
	{
		let mm = match meta {
			None => None,
			Some(m) => Some(Box::new(m)),
		};
		RpcValue {
			meta: mm,
			value: val.chainpack_make_value(),
		}
	}
		pub fn set_meta(&mut self, m: MetaMap) {
		if m.is_empty() {
			self.meta = None;
		}
		else {
			self.meta = Some(Box::new(m));
		}
	}
	*/
	pub fn set_meta(mut self, meta: Option<MetaMap>) -> Self {
		match meta {
			None => { self.meta = None }
			Some(mm) => { self.meta = Some(Box::new(mm)) }
		}
		self
	}
	pub fn has_meta(&self) -> bool {
		match &self.meta {
			Some(_) => true,
			_ => false,
		}
	}
	pub fn meta(&self) -> &MetaMap {
		match &self.meta {
			Some(mm) => {
				return mm
			}
			None => {
				let mm = EMPTY_METAMAP.get_or_init(|| MetaMap::new());
				return mm
			}
		}
	}
	pub fn meta_mut(&mut self) -> Option<&mut MetaMap> {
		match &mut self.meta {
			Some(mm) => Some(mm.as_mut()),
			_ => None,
		}
	}
	pub fn clear_meta(&mut self) {
		self.meta = None;
	}

	pub fn value(&self) -> &Value {
		&self.value
	}
	pub fn value_mut(&mut self) -> &mut Value {
		&mut self.value
	}

	pub fn type_name(&self) -> &'static str {
		&self.value.type_name()
	}

	is_xxx!(is_null, Value::Null);
	is_xxx!(is_bool, Value::Bool(_));
	is_xxx!(is_int, Value::Int(_));
	is_xxx!(is_string, Value::String(_));
	is_xxx!(is_blob, Value::Blob(_));
	is_xxx!(is_list, Value::List(_));
	is_xxx!(is_map, Value::Map(_));
	is_xxx!(is_imap, Value::IMap(_));

	pub fn is_default_value(&self) -> bool {
		self.value.is_default_value()
	}
	pub fn as_bool(&self) -> bool {
		match &self.value {
			Value::Bool(d) => *d,
			_ => false,
		}
	}
	pub fn as_int(&self) -> i64 {
		return self.as_i64()
	}
	pub fn as_i64(&self) -> i64 {
		match &self.value {
			Value::Int(d) => *d,
			Value::UInt(d) => *d as i64,
			_ => 0,
		}
	}
	pub fn as_i32(&self) -> i32 { self.as_i64() as i32 }
	pub fn as_u64(&self) -> u64 {
		match &self.value {
			Value::Int(d) => *d as u64,
			Value::UInt(d) => *d,
			_ => 0,
		}
	}
	pub fn as_u32(&self) -> u32 { self.as_u64() as u32 }
	pub fn as_f64(&self) -> f64 {
		match &self.value {
			Value::Double(d) => *d,
			_ => 0.,
		}
	}
	pub fn as_usize(&self) -> usize {
		match &self.value {
			Value::Int(d) => *d as usize,
			Value::UInt(d) => *d as usize,
			_ => 0 as usize,
		}
	}
	pub fn as_datetime(&self) -> datetime::DateTime {
		match &self.value {
			Value::DateTime(d) => d.clone(),
			_ => datetime::DateTime::from_epoch_msec(0),
		}
	}
	pub fn to_datetime(&self) -> Option<datetime::DateTime> {
		match &self.value {
			Value::DateTime(d) => Some(d.clone()),
			_ => None,
		}
	}
	pub fn as_decimal(&self) -> decimal::Decimal {
		match &self.value {
			Value::Decimal(d) => d.clone(),
			_ => decimal::Decimal::new(0, 0),
		}
	}
	// pub fn as_str(&self) -> Result<&str, Utf8Error> {
	// 	match &self.value {
	// 		Value::String(b) => std::str::from_utf8(b),
	// 		_ => std::str::from_utf8(EMPTY_BYTES_REF),
	// 	}
	// }
	pub fn as_blob(&self) -> &[u8] {
		match &self.value {
			Value::Blob(b) => b,
			_ => EMPTY_BYTES_REF,
		}
	}
	pub fn as_str(&self) -> &str {
		match &self.value {
			Value::String(b) => b,
			_ => EMPTY_STR_REF,
		}
	}
	pub fn as_list(&self) -> &Vec<RpcValue> {
		match &self.value {
			Value::List(b) => &b,
			_ => EMPTY_LIST.get_or_init(|| List::new()),
		}
	}
	pub fn as_map(&self) -> &Map {
		match &self.value {
			Value::Map(b) => &b,
			_ => EMPTY_MAP.get_or_init(|| Map::new()),
		}
	}
	pub fn as_imap(&self) -> &BTreeMap<i32, RpcValue> {
		match &self.value {
			Value::IMap(b) => &b,
			_ => EMPTY_IMAP.get_or_init(|| IMap::new()),
		}
	}
	pub fn get<I>(&self, key: I) -> Option<&RpcValue>
		where I: GetIndex
	{
		match key.make_key() {
			GetKey::Int(ix) => {
				match &self.value {
					Value::List(lst) => lst.get(ix as usize),
					Value::IMap(map) => map.get(&ix),
					_ => { None }
				}
			}
			GetKey::Str(ix) => {
				match &self.value {
					Value::Map(map) => map.get(ix),
					_ => { None }
				}
			}
		}
	}
	pub fn to_cpon(&self) -> String { self.to_cpon_indented("").unwrap_or("".to_string()) }
	pub fn to_cpon_indented(&self, indent: &str) -> crate::Result<String> {
		let buff = self.to_cpon_bytes_indented(indent.as_bytes())?;
		match String::from_utf8(buff) {
			Ok(s) => Ok(s),
			Err(e) => Err(e.into()),
		}
	}
	pub fn to_cpon_bytes_indented(&self, indent: &[u8]) -> crate::Result<Vec<u8>> {
		let mut buff: Vec<u8> = Vec::new();
		let mut wr = CponWriter::new(&mut buff);
		wr.set_indent(indent);
		match wr.write(self) {
			Ok(_) => { Ok(buff) }
			Err(err) => { Err(err.into()) }
		}
	}
	pub fn to_chainpack(&self) -> Vec<u8> {
		let mut buff: Vec<u8> = Vec::new();
		let mut wr = ChainPackWriter::new(&mut buff);
		let r = wr.write(self);
		match r {
			Ok(_) => buff,
			Err(_) => Vec::new(),
		}
	}

	pub fn from_cpon(s: &str) -> ReadResult {
		let mut buff = s.as_bytes();
		let mut rd = CponReader::new(&mut buff);
		rd.read()
	}
	pub fn from_chainpack(b: &[u8]) -> ReadResult {
		let mut buff = b;
		let mut rd = ChainPackReader::new(&mut buff);
		rd.read()
	}

}
static NULL_RPCVALUE: OnceLock<RpcValue> = OnceLock::new();

impl Default for &RpcValue {
	fn default() -> Self {
		NULL_RPCVALUE.get_or_init(|| RpcValue::null())
	}
}
impl fmt::Debug for RpcValue {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		//write!(f, "RpcValue {{meta: {:?} value: {:?}}}", self.meta, self.value)
		write!(f, "{}", self.to_cpon())
	}
}
impl fmt::Display for RpcValue {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.to_cpon())
	}
}

#[cfg(test)]
mod test {
	use std::collections::BTreeMap;
	use std::mem::size_of;

	use chrono::Offset;

	use crate::{DateTime};
	use crate::Decimal;
	use crate::metamap::MetaMap;
	use crate::rpcvalue::{RpcValue, Value, Map};

	macro_rules! show_size {
		(header) => (
			log::debug!("{:<22} {:>4}    ", "Type", "T");
			log::debug!("------------------------------");
		);
		($t:ty) => (
			log::debug!("{:<22} {:4}", stringify!($t), size_of::<$t>())
		)
	}

	#[test]
	fn size() {
		show_size!(header);
		show_size!(usize);
		show_size!(MetaMap);
		show_size!(Box<MetaMap>);
		show_size!(Option<MetaMap>);
		show_size!(Option<Box<MetaMap>>);
		show_size!(Value);
		show_size!(Option<Value>);
		show_size!(RpcValue);
	}

	#[test]
	fn rpcval_new()
	{
		let rv = RpcValue::from(true);
		assert_eq!(rv.as_bool(), true);
		let rv = RpcValue::from("foo");
		assert_eq!(rv.as_str(), "foo");
		let rv = RpcValue::from(&b"bar"[..]);
		assert_eq!(rv.as_blob(), b"bar");
		let rv = RpcValue::from(123);
		assert_eq!(rv.as_i32(), 123);
		let rv = RpcValue::from(12.3);
		assert_eq!(rv.as_f64(), 12.3);

		let dt = DateTime::now();
		let rv = RpcValue::from(dt.clone());
		assert_eq!(rv.as_datetime(), dt);

		let dc = Decimal::new(123, -1);
		let rv = RpcValue::from(dc.clone());
		assert_eq!(rv.as_decimal(), dc);

		let dt = chrono::offset::Utc::now();
		let rv = RpcValue::from(dt.clone());
		assert_eq!(rv.as_datetime().epoch_msec(), dt.timestamp_millis());

		let dt = chrono::offset::Local::now();
		let rv = RpcValue::from(dt.clone());
		assert_eq!(rv.as_datetime().epoch_msec() + rv.as_datetime().utc_offset() as i64 * 1000
				   , dt.timestamp_millis() + dt.offset().fix().local_minus_utc() as i64 * 1000);

		let vec1 = vec![RpcValue::from(123), RpcValue::from("foo")];
		let rv = RpcValue::from(vec1.clone());
		assert_eq!(rv.as_list(), &vec1);

		let mut m: Map = BTreeMap::new();
		m.insert("foo".to_string(), RpcValue::from(123));
		m.insert("bar".to_string(), RpcValue::from("foo"));
		let rv = RpcValue::from(m.clone());
		assert_eq!(rv.as_map(), &m);

		let mut m: BTreeMap<i32, RpcValue> = BTreeMap::new();
		m.insert(1, RpcValue::from(123));
		m.insert(2, RpcValue::from("foo"));
		let rv = RpcValue::from(m.clone());
		assert_eq!(rv.as_imap(), &m);
	}

}

