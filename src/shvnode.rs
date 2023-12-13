use std::collections::{BTreeMap, HashSet};
use std::format;
use log::warn;
use crate::metamethod::{Flag, MetaMethod};
use crate::{metamethod, RpcMessage, RpcMessageMetaTags, RpcValue, rpcvalue};
use crate::metamethod::Access;
use crate::rpcframe::RpcFrame;
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

fn dir<'a>(methods: impl Iterator<Item=&'a MetaMethod>, param: DirParam) -> RpcValue {
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

pub fn process_local_dir_ls<V>(mounts: &BTreeMap<String, V>, frame: &RpcFrame) -> Option<ProcessRequestResult> {
    let shv_path = frame.shv_path().unwrap_or_default();
    let method = frame.method().unwrap_or_default();
    let children_on_path = children_on_path(&mounts, shv_path);
    let mount = find_longest_prefix(mounts, &shv_path);
    let is_mount_point = mount.is_some();
    match children_on_path {
        None => {
            if is_mount_point {
                // remote request
                None
            } else {
                // path doesn't exist
                Some(Err(RpcError::new(RpcErrorCode::MethodNotFound, &format!("Invalid shv path: {}", shv_path))))
            }
        }
        Some(children) => {
            if !children.is_empty() {
                if method == METH_LS {
                    // ls on not-leaf node must be resolved locally
                    if let Ok(rpcmsg) = frame.to_rpcmesage() {
                        let ls = ls(&mounts, shv_path, rpcmsg.param().into());
                        Some(ls)
                    } else {
                        Some(Err(RpcError::new(RpcErrorCode::InvalidRequest, &format!("Cannot convert RPC frame to Rpc message"))))
                    }
                } else if !is_mount_point {
                    if method == METH_DIR {
                        // dir in the middle of the tree must be resolved locally
                        if let Ok(rpcmsg) = frame.to_rpcmesage() {
                            let dir = Ok((dir(DIR_LS_METHODS.iter().into_iter(), rpcmsg.param().into()), None));
                            Some(dir)
                        } else {
                            Some(Err(RpcError::new(RpcErrorCode::InvalidRequest, &format!("Cannot convert RPC frame to Rpc message"))))
                        }
                    } else { None }
                } else { None }
            } else { None }
        }
    }
}
fn ls<V>(mounts: &BTreeMap<String, V>, shv_path: &str, param: LsParam) -> ProcessRequestResult {
    ls_children_to_result(children_on_path(mounts, shv_path), param)
}
fn ls_children_to_result(children: Option<Vec<String>>, param: LsParam) -> ProcessRequestResult {
    match param {
        LsParam::List => {
            match children {
                None => {
                    Err(RpcError::new(RpcErrorCode::MethodCallException, "Invalid shv path"))
                }
                Some(dirs) => {
                    let res: rpcvalue::List = dirs.iter().map(|d| RpcValue::from(d)).collect();
                    Ok((res.into(), None))
                }
            }
        }
        LsParam::Exists(path) => {
            match children {
                None => {
                    Ok((false.into(), None))
                }
                Some(children) => {
                    Ok((children.contains(&path).into(), None))
                }
            }
        }
    }
}
pub fn children_on_path<V>(mounts: &BTreeMap<String, V>, path: &str) -> Option<Vec<String>> {
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
        assert_eq!(super::children_on_path(&mounts, ""), Some(vec!["a".to_string(), "b".to_string()]));
        assert_eq!(super::children_on_path(&mounts, "a"), Some(vec!["1".to_string()]));
        assert_eq!(super::children_on_path(&mounts, "b/2"), Some(vec!["C".to_string(), "D".to_string()]));
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
                let errmsg = format!("Unknown method '{}:{}()', path.", rq.shv_path().unwrap_or_default(), rq.method().unwrap_or_default());
                warn!("{}", &errmsg);
                Err(RpcError::new(RpcErrorCode::MethodNotFound, &errmsg))
            }
        }
    }
    fn process_dir(&mut self, rq: &RpcMessage) -> ProcessRequestResult {
        let shv_path = rq.shv_path().unwrap_or_default();
        if shv_path.is_empty() {
            Ok((dir(DIR_LS_METHODS.iter().chain(self.methods().into_iter()), rq.param().into()), None))
        } else {
            let errmsg = format!("Unknown method '{}:{}()', invalid path.", rq.shv_path().unwrap_or_default(), rq.method().unwrap_or_default());
            warn!("{}", &errmsg);
            Err(RpcError::new(RpcErrorCode::MethodNotFound, &errmsg))
        }
    }
    fn process_ls(&mut self, rq: &RpcMessage) -> ProcessRequestResult {
        let shv_path = rq.shv_path().unwrap_or_default();
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
            let errmsg = format!("Unknown method '{}:{}()', invalid path.", rq.shv_path().unwrap_or_default(), rq.method().unwrap_or(""));
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

pub const DIR_LS_METHODS: [MetaMethod; 2] = [
    MetaMethod { name: METH_DIR, flags: Flag::None as u32, access: Access::Browse, param: "DirParam", result: "DirResult", description: "" },
    MetaMethod { name: METH_LS, flags: Flag::None as u32, access: Access::Browse, param: "LsParam", result: "LsResult", description: "" },
];
pub const PROPERTY_METHODS: [MetaMethod; 3] = [
    MetaMethod { name: METH_GET, flags: Flag::IsGetter as u32, access: Access::Browse, param: "", result: "", description: "" },
    MetaMethod { name: METH_SET, flags: Flag::IsSetter as u32, access: Access::Browse, param: "", result: "", description: "" },
    MetaMethod { name: SIG_CHNG, flags: Flag::IsSignal as u32, access: Access::Browse, param: "", result: "", description: "" },
];
const DEVICE_METHODS: [MetaMethod; 3] = [
    MetaMethod { name: METH_NAME, flags: Flag::IsGetter as u32, access: Access::Browse, param: "", result: "", description: "" },
    MetaMethod { name: METH_VERSION, flags: Flag::IsGetter as u32, access: Access::Browse, param: "", result: "", description: "" },
    MetaMethod { name: METH_SERIAL_NUMBER, flags: Flag::IsGetter as u32, access: Access::Browse, param: "", result: "", description: "" },
];
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
const APP_METHODS: [MetaMethod; 4] = [
    MetaMethod { name: METH_SHV_VERSION_MAJOR, flags: Flag::IsGetter as u32, access: Access::Browse, param: "", result: "", description: "" },
    MetaMethod { name: METH_SHV_VERSION_MINOR, flags: Flag::IsGetter as u32, access: Access::Browse, param: "", result: "", description: "" },
    MetaMethod { name: METH_NAME, flags: Flag::IsGetter as u32, access: Access::Browse, param: "", result: "", description: "" },
    MetaMethod { name: METH_PING, flags: Flag::None as u32, access: Access::Browse, param: "", result: "", description: "" },
];

impl<K> ShvNode<K> for AppNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        APP_METHODS.iter().collect()
    }

    fn process_request(&mut self, rq: &RpcMessage, _state: &mut K) -> ProcessRequestResult {
        if rq.shv_path().unwrap_or_default().is_empty() {
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
        if rq.shv_path().unwrap_or_default().is_empty() {
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
