use std::collections::{BTreeMap, HashSet};
use std::format;
use lazy_static::lazy_static;
use log::warn;
use crate::metamethod::{Flag, MetaMethod};
use crate::{metamethod, RpcMessage, RpcMessageMetaTags, RpcValue, rpcvalue};
use crate::rpcmessage::{RpcError, RpcErrorCode};

pub enum DirParam {
    Brief,
    Full,
    BriefMethod(String),
}
impl From<Option<&RpcValue>> for DirParam {
    fn from(value: Option<&RpcValue>) -> Self {
        match value {
            Some(rpcval) => {
                if rpcval.is_string() {
                    DirParam::BriefMethod(rpcval.as_str().into())
                } else {
                    if rpcval.as_bool() {
                        DirParam::Full
                    } else {
                        DirParam::Brief
                    }
                }
            }
            None => {
                DirParam::Brief
            }
        }
    }
}

pub fn dir<'a>(methods: impl Iterator<Item=&'a MetaMethod>, param: DirParam) -> RpcValue {
    let mut result = RpcValue::null();
    let mut lst = rpcvalue::List::new();
    for mm in methods {
        match param {
            DirParam::Brief => {
                lst.push(mm.to_rpcvalue(metamethod::DirFormat::IMap));
            }
            DirParam::Full => {
                lst.push(mm.to_rpcvalue(metamethod::DirFormat::Map));
            }
            DirParam::BriefMethod(ref method_name) => {
                if &mm.name == method_name {
                    result = mm.to_rpcvalue(metamethod::DirFormat::IMap);
                    break;
                }
            }
        }
    }
    if result.is_null() {
        lst.into()
    } else {
        result
    }
}

pub enum LsParam {
    List,
    Exists(String),
}
impl From<Option<&RpcValue>> for LsParam {
    fn from(value: Option<&RpcValue>) -> Self {
        match value {
            Some(rpcval) => {
                if rpcval.is_string() {
                    LsParam::Exists(rpcval.as_str().into())
                } else {
                    LsParam::List
                }
            }
            None => {
                LsParam::List
            }
        }
    }
}
pub fn dir_ls<V>(mounts: &BTreeMap<String, V>, rpcmsg: RpcMessage) -> ProcessRequestResult {
    let shv_path = rpcmsg.shv_path_or_empty();
    match rpcmsg.method() {
        None => {
            Err(RpcError::new(RpcErrorCode::InvalidParam, &format!("Shv call method missing.")))
        }
        Some("dir") => {
            Ok((dir(DIR_LS_METHODS.iter().into_iter(), rpcmsg.param().into()), None))
        }
        Some("ls") => {
            ls(&mounts, shv_path, rpcmsg.param().into())
        }
        Some(method) => {
            Err(RpcError::new(RpcErrorCode::MethodCallException, &format!("Unknown method {}", method)))
        }
    }
}
pub fn ls<V>(mounts: &BTreeMap<String, V>, shv_path: &str, param: LsParam) -> ProcessRequestResult {
    match param {
        LsParam::List => {
            match ls_mounts(mounts, shv_path) {
                None => {
                    Err(RpcError::new(RpcErrorCode::MethodCallException, &format!("Invalid shv path: {}", shv_path)))
                }
                Some(dirs) => {
                    let res: rpcvalue::List = dirs.iter().map(|d| RpcValue::from(d)).collect();
                    Ok((res.into(), None))
                }
            }
        }
        LsParam::Exists(path) => {
            Ok((mounts.contains_key(&path).into(), None))
        }
    }
}
fn ls_mounts<V>(mounts: &BTreeMap<String, V>, path: &str) -> Option<Vec<String>> {
    let mut dirs: Vec<String> = Vec::new();
    let mut unique_dirs: HashSet<String> = HashSet::new();
    let mut dir_exists = false;
    for (key, _) in mounts.range(path.to_string()..) {
        if key.starts_with(path) {
            dir_exists = true;
            if path.is_empty()
                || key.len() == path.len()
                || key.as_bytes()[path.len()] == ('/' as u8)
            {
                if key.len() > path.len() {
                    let dir_rest_start = if path.is_empty() { 0 } else { path.len() + 1 };
                    let mut updirs = key[dir_rest_start..].split('/');
                    if let Some(dir) = updirs.next() {
                        if !unique_dirs.contains(dir) {
                            dirs.push(dir.to_string());
                            unique_dirs.insert(dir.to_string());
                        }
                    }
                }
            }
        } else {
            break;
        }
    }
    if dir_exists {
        Some(dirs)
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_mounts() {
        let mut mounts = BTreeMap::new();
        mounts.insert("a".into(), ());
        mounts.insert("a/1".into(), ());
        mounts.insert("b/2/C".into(), ());
        mounts.insert("b/2/D".into(), ());
        mounts.insert("b/3/E".into(), ());
        assert_eq!(super::ls_mounts(&mounts, ""), Some(vec!["a".to_string(), "b".to_string()]));
        assert_eq!(super::ls_mounts(&mounts, "a"), Some(vec!["1".to_string()]));
        assert_eq!(super::ls_mounts(&mounts, "b/2"), Some(vec!["C".to_string(), "D".to_string()]));
    }
}
pub fn find_longest_prefix<'a, 'b, V>(map: &'a BTreeMap<String, V>, shv_path: &'b str) -> Option<(&'b str, &'b str)> {
    let mut path = &shv_path[..];
    let mut rest = "";
    loop {
        if map.contains_key(path) {
            return Some((path, rest))
        }
        if path.is_empty() {
            break;
        }
        if let Some(slash_ix) = path.rfind('/') {
            path = &shv_path[..slash_ix];
            rest = &shv_path[(slash_ix + 1)..];
        } else {
            path = "";
            rest = shv_path;
        };
    }
    None
}

// fn process_request_dir_ls<'a, K>(methods: impl Iterator<Item=&'a MetaMethod<K>>, rpcmsg: &RpcMessage) -> ProcessRequestResult {
//     let shv_path = rpcmsg.shv_path_or_empty();
//     if shv_path.is_empty() {
//         match rpcmsg.method() {
//             Some(METH_DIR) => {
//                 Ok((dir(methods, rpcmsg.param().into()), None))
//             }
//             Some(METH_LS) => {
//                 match LsParam::from(rpcmsg.param()) {
//                     LsParam::List => {
//                         Ok((rpcvalue::List::new().into(), None))
//                     }
//                     LsParam::Exists(_path) => {
//                         Ok((false.into(), None))
//                     }
//                 }
//             }
//             _ => {
//                 let errmsg = format!("Unknown method '{}:{}()'", rpcmsg.shv_path_or_empty(), rpcmsg.method().unwrap_or(""));
//                 warn!("{}", &errmsg);
//                 Err(RpcError::new(RpcErrorCode::MethodNotFound, &errmsg))
//             }
//         }
//     } else {
//         let errmsg = format!("Unknown method '{}:{}()', invalid path.", rpcmsg.shv_path_or_empty(), rpcmsg.method().unwrap_or(""));
//         warn!("{}", &errmsg);
//         Err(RpcError::new(RpcErrorCode::MethodNotFound, &errmsg))
//     }
// }
pub struct Signal {
    pub value: RpcValue,
    pub method: &'static str,
}
pub type ProcessRequestResult = std::result::Result<(RpcValue, Option<Signal>), RpcError>;
// type RequestHandler<S, K> = fn(this: &mut S, rq: &RpcMessage, state: &mut K) -> ProcessRequestResult;
pub trait ShvNode<K> {
    fn methods(&self) -> Vec<&MetaMethod>;
    fn process_request(&mut self, rq: &RpcMessage, state: &mut K) -> ProcessRequestResult;
    fn process_dir_ls(&mut self, rq: &RpcMessage) -> ProcessRequestResult {
        match rq.method() {
            Some(METH_DIR) => {
                self.process_dir(rq)
            }
            Some(METH_LS) => {
                self.process_ls(rq)
            }
            _ => {
                let errmsg = format!("Unknown method '{}:{}()', path.", rq.shv_path_or_empty(), rq.method().unwrap_or(""));
                warn!("{}", &errmsg);
                Err(RpcError::new(RpcErrorCode::MethodNotFound, &errmsg))
            }
        }
    }
    fn process_dir(&mut self, rq: &RpcMessage) -> ProcessRequestResult {
        let shv_path = rq.shv_path_or_empty();
        if shv_path.is_empty() {
            Ok((dir(DIR_LS_METHODS.iter().chain(self.methods().into_iter()), rq.param().into()), None))
        } else {
            let errmsg = format!("Unknown method '{}:{}()', invalid path.", rq.shv_path_or_empty(), rq.method().unwrap_or(""));
            warn!("{}", &errmsg);
            Err(RpcError::new(RpcErrorCode::MethodNotFound, &errmsg))
        }
    }
    fn process_ls(&mut self, rq: &RpcMessage) -> ProcessRequestResult {
        let shv_path = rq.shv_path_or_empty();
        if shv_path.is_empty() {
            match LsParam::from(rq.param()) {
                LsParam::List => {
                    Ok((rpcvalue::List::new().into(), None))
                }
                LsParam::Exists(_path) => {
                    Ok((false.into(), None))
                }
            }
        } else {
            let errmsg = format!("Unknown method '{}:{}()', invalid path.", rq.shv_path_or_empty(), rq.method().unwrap_or(""));
            warn!("{}", &errmsg);
            Err(RpcError::new(RpcErrorCode::MethodNotFound, &errmsg))
        }
    }
}

pub const METH_DIR: &str = "dir";
pub const METH_LS: &str = "ls";
pub const METH_GET: &str = "get";
pub const METH_SET: &str = "set";
pub const SIG_CHNG: &str = "chng";
pub const METH_SHV_VERSION_MAJOR: &str = "shvVersionMajor";
pub const METH_SHV_VERSION_MINOR: &str = "shvVersionMinor";
pub const METH_NAME: &str = "name";
pub const METH_PING: &str = "ping";
pub const METH_VERSION: &str = "version";
pub const METH_SERIAL_NUMBER: &str = "serialNumber";

lazy_static! {
    pub static ref DIR_LS_METHODS: [MetaMethod; 2] = [
        MetaMethod { name: METH_DIR.into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
        MetaMethod { name: METH_LS.into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
    ];
    static ref PROPERTY_METHODS: [MetaMethod; 3] = [
        MetaMethod { name: METH_GET.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_SET.into(), flags: Flag::IsSetter.into(),  ..Default::default() },
        MetaMethod { name: SIG_CHNG.into(), flags: Flag::IsSignal.into(),  ..Default::default() },
    ];
    static ref DEVICE_METHODS: [MetaMethod; 3] = [
        MetaMethod { name: METH_NAME.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_VERSION.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_SERIAL_NUMBER.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
    ];
}
pub struct AppNode {
    pub app_name: &'static str,
    pub shv_version_major: i32,
    pub shv_version_minor: i32,
}
impl Default for AppNode {
    fn default() -> Self {
        AppNode {
            app_name: "",
            shv_version_major: 3,
            shv_version_minor: 0,
        }
    }
}
lazy_static! {
    static ref APP_METHODS: [MetaMethod; 4] = [
        MetaMethod { name: METH_SHV_VERSION_MAJOR.into(), flags: Flag::IsGetter.into(), ..Default::default() },
        MetaMethod { name: METH_SHV_VERSION_MINOR.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_NAME.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_PING.into(), ..Default::default() },
    ];
}

impl<K> ShvNode<K> for AppNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        APP_METHODS.iter().collect()
    }

    fn process_request(&mut self, rq: &RpcMessage, _state: &mut K) -> ProcessRequestResult {
        if rq.shv_path_or_empty().is_empty() {
            match rq.method() {
                Some(METH_SHV_VERSION_MAJOR) => { return Ok((self.shv_version_major.into(), None))  }
                Some(METH_SHV_VERSION_MINOR) => { return Ok((self.shv_version_minor.into(), None))  }
                Some(METH_NAME) => { return Ok((RpcValue::from(self.app_name), None))  }
                Some(METH_PING) => { return Ok((().into(), None))  }
                _ => {}
            }
        }
        <AppNode as ShvNode<K>>::process_dir_ls(self, rq)
    }
}
pub struct AppDeviceNode {
    pub device_name: &'static str,
    pub version: &'static str,
    pub serial_number: Option<String>,
}
impl Default for AppDeviceNode {
    fn default() -> Self {
        AppDeviceNode {
            device_name: "",
            version: "",
            serial_number: None,
        }
    }
}
impl<K> ShvNode<K> for AppDeviceNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DEVICE_METHODS.iter().collect()
    }

    fn process_request(&mut self, rq: &RpcMessage, _state: &mut K) -> ProcessRequestResult {
        if rq.shv_path_or_empty().is_empty() {
            match rq.method() {
                Some(METH_NAME) => {
                    return Ok((RpcValue::from(self.device_name), None))
                }
                Some(METH_VERSION) => {
                    return Ok((RpcValue::from(self.version), None))
                }
                Some(METH_SERIAL_NUMBER) => {
                    match &self.serial_number {
                        None => {return Ok((RpcValue::null(), None))}
                        Some(sn) => {return Ok((RpcValue::from(sn), None))}
                    }
                }
                _ => {
                }
            }
        }
        ShvNode::<K>::process_dir_ls(self, rq)
    }
}
