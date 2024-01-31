use crate::rpcvalue::RpcValue;
use std::fmt;
use crate::{CponWriter, Writer};

#[derive(Debug, Clone, PartialEq)]
pub enum MetaKey {
    Int(i32),
    Str(String),
}
impl From<&GetKey<'_>> for MetaKey {
    fn from(k: &GetKey) -> Self {
        match k {
            GetKey::Int(i) => MetaKey::Int(*i),
            GetKey::Str(s) => MetaKey::Str((*s).to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MetaKeyVal {
    pub(crate) key: MetaKey,
    pub(crate) value: RpcValue
}

#[derive(Copy, Clone)]
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
pub struct MetaMap(pub(crate) Vec<MetaKeyVal>);

impl MetaMap {

    pub fn new() -> MetaMap {
        MetaMap(Vec::new())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn insert<I>(&mut self, key: I, value: RpcValue) -> &mut Self
        where I: GetIndex
    {
        match self.find(&key) {
            None => {
                let key = key.make_key();
                let key = MetaKey::from(&key);
                self.0.push(MetaKeyVal{key, value })
            },
            Some(ix) => self.0[ix].value = value,
        }
        self
    }
    pub fn remove<I>(&mut self, key: I) -> Option<RpcValue>
        where I: GetIndex
    {
        match self.find(&key) {
            None => None,
            Some(ix) => Some(self.0.remove(ix).value),
        }
    }
    pub fn get<I>(&self, key: I) -> Option<&RpcValue>
        where I: GetIndex
    {
        match self.find(&key) {
            Some(ix) => Some(&self.0[ix].value),
            None => None,
        }
    }
    fn find<I>(&self, key: &I) -> Option<usize>
        where I: GetIndex
    {
        let key_to_search = key.make_key();
        let mut ix = 0;
        for kv in &self.0 {
            match key_to_search {
                GetKey::Int(key_to_search) => {
                    match &kv.key {
                        MetaKey::Int(key) if *key == key_to_search => {return Some(ix)}
                        _ => { () }
                    }
                }
                GetKey::Str(key_to_search) => {
                    match &kv.key {
                        MetaKey::Str(key) if key == key_to_search => { return Some(ix) }
                        _ => { () }
                    }
                }
            }
            ix += 1;
        }
        None
    }
    /*
    pub fn value<Idx>(&self, ix: Idx) -> Option<&RpcValue>
        where Idx: IntoMetaKeyRef
    {
        match self.find1(ix) {
            Some(ix) => Some(&self.0[ix].value),
            None => None,
        }
    }
    pub fn value_or_default<'a, Idx>(&'a self, ix: Idx, def_val: &'a RpcValue) -> &'a RpcValue
        where Idx: IntoMetaKeyRef
    {
        match self.find1(ix) {
            Some(ix) => &self.0[ix].value,
            None => def_val,
        }
    }
    fn find1<I>(&self, key: I) -> Option<usize>
        where I: IntoMetaKeyRef
    {
        let mut ix = 0;
        for kv in self.0.iter() {
            let mk = key.to_metakeyref();
            match &kv.key {
                MetaKey::Str(k1) => {
                    match &mk {
                        MetaKeyRef::Str(k2) => if k1 == k2 {return Some(ix)}
                        _ 		=> (),
                    }
                },
                MetaKey::Int(k1) => {
                    match &mk {
                        MetaKeyRef::Int(k2) => if k1 == k2 {return Some(ix)}
                        _ 		=> (),
                    }
                },
            }
            ix = ix + 1;
        }
        None
    }
    */
}
impl fmt::Debug for MetaMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl fmt::Display for MetaMap {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut buff: Vec<u8> = Vec::new();
        let mut wr = CponWriter::new(&mut buff);
        let res = wr.write_meta(self);
        if let Err(e) = res {
            log::warn!("to_cpon write with error: {}", e);
            return write!(fmt, "<invalid>")
        }
        match String::from_utf8(buff) {
            Ok(s) => write!(fmt, "{}", s),
            Err(_) => write!(fmt, "<invalid>"),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::metamap::MetaMap;
    use crate::rpcvalue::RpcValue;
    use std::collections::BTreeMap;

    #[test]
    fn metamap_insert() {
        let mut mm = MetaMap::new();

        mm.insert(123, RpcValue::from(1.1));
        assert_eq!(mm.get(123).unwrap_or_default().as_f64(), 1.1);
        // let vv = mm.value(1234, Some(&RpcValue::from(123)));
        // println!("inserted and retrieved: {:?}", vv);
        // assert_eq!(123, vv.unwrap().to_i32().unwrap());

        mm.insert("foo", RpcValue::from("bar")).insert(123, RpcValue::from("baz"));
        assert_eq!(mm.get("foo").unwrap_or_default().as_str(), "bar");
        // println!("val: {:?}", mm.get_or_null(123]);
        assert_eq!(mm.get(123).unwrap_or_default().as_str(), "baz");

        let v1 = vec![RpcValue::from("foo"), RpcValue::from("bar"), RpcValue::from("baz")];
        let v2 = v1.clone();
        mm.insert("list", RpcValue::from(v1));
        assert_eq!(mm.get("list").unwrap_or_default().as_list(), &v2);

        let mut v1: BTreeMap<i32, RpcValue> = BTreeMap::new();
        v1.insert(1, RpcValue::from("foo"));
        v1.insert(2, RpcValue::from("bar"));
        v1.insert(3, RpcValue::from("baz"));
        let v2 = v1.clone();
        mm.insert("imap", RpcValue::from(v1));
        assert_eq!(mm.get("imap").unwrap_or_default().as_imap(), &v2);

        let mut v1: BTreeMap<String, RpcValue> = BTreeMap::new();
        v1.insert("a".to_string(), RpcValue::from("foo"));
        v1.insert("b".to_string(), RpcValue::from("bar"));
        v1.insert("c".to_string(), RpcValue::from("baz"));
        let v2 = v1.clone();
        mm.insert("map", RpcValue::from(v1));
        assert_eq!(mm.get("map").unwrap_or_default().as_map(), &v2);

    }
    /*
    #[test]
    #[should_panic]
    fn metamap_invalid_string_key() {
        let mm = MetaMap::new();
        let _a = &mm.get_or_null("abcd"];
    }

    #[test]
    #[should_panic]
    fn metamap_invalid_int_key() {
        let mut mm = MetaMap::new();
        mm.insert(123, RpcValue::from(()));
        let _a = &mm.get_or_null(1234];
    }
    */
}
