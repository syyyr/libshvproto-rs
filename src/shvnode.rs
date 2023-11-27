use std::collections::{BTreeMap, HashSet};
use std::format;
use lazy_static::lazy_static;
use crate::metamethod::{Flag, MetaMethod};
use crate::{metamethod, RpcMessage, RpcMessageMetaTags, RpcValue, rpcvalue};

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
pub fn dir_ls<V>(mounts: &BTreeMap<String, V>, rpcmsg: RpcMessage) -> Result<RpcValue, String> {
    let shv_path = rpcmsg.shv_path().unwrap_or("");
    match rpcmsg.method() {
        None => {
            Err(format!("Shv call method missing."))
        }
        Some("dir") => {
            Ok(dir(DIR_LS_METHODS.iter().into_iter(), rpcmsg.param().into()))
        }
        Some("ls") => {
            ls(&mounts, shv_path, rpcmsg.param().into())
        }
        Some(method) => {
            Err(format!("Unknown method {}", method))
        }
    }
}
pub fn ls<V>(mounts: &BTreeMap<String, V>, shv_path: &str, param: LsParam) -> Result<RpcValue, String> {
    match param {
        LsParam::List => {
            match ls_mounts(mounts, shv_path) {
                None => {
                    Err(format!("Invalid shv path: {}", shv_path))
                }
                Some(dirs) => {
                    let res: rpcvalue::List = dirs.iter().map(|d| RpcValue::from(d)).collect();
                    Ok(res.into())
                }
            }
        }
        LsParam::Exists(path) => {
            Ok(mounts.contains_key(&path).into())
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
        mounts.insert("b/b/c".into(), ());
        mounts.insert("b/b/d".into(), ());
        assert_eq!(super::ls_mounts(&mounts, ""), Some(vec!["a".to_string(), "b".to_string()]));
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

pub type ProcessRequestResult = crate::Result<Option<RpcValue>>;
pub trait ShvNode {
    fn methods(&self) -> Vec<&MetaMethod>;
    fn process_request(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult;
    fn process_request_dir_ls(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        match rpcmsg.method() {
            Some(METH_DIR) => {
                Ok(Some(dir(self.methods().into_iter(), rpcmsg.param().into())))
            }
            Some(METH_LS) => {
                Ok(Some(rpcvalue::List::new().into()))
            }
            _ => {
                Err(format!("Method '{}:{}()' NIY", rpcmsg.shv_path().unwrap_or(""), rpcmsg.method().unwrap_or("")).into())
            }
        }
    }
}

pub const METH_DIR: &str = "dir";
pub const METH_LS: &str = "ls";
pub const METH_GET: &str = "get";
pub const METH_SET: &str = "set";
pub const METH_SHV_VERSION_MAJOR: &str = "shvVersionMajor";
pub const METH_SHV_VERSION_MINOR: &str = "shvVersionMinor";
pub const METH_NAME: &str = "name";
pub const METH_PING: &str = "ping";
pub const METH_VERSION: &str = "version";
pub const METH_SERIAL_NUMBER: &str = "serialNumber";

lazy_static! {
    static ref DIR_LS_METHODS: [MetaMethod; 2] = [
        MetaMethod { name: METH_DIR.into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
        MetaMethod { name: METH_LS.into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
    ];
    static ref APP_METHODS: [MetaMethod; 4] = [
        MetaMethod { name: METH_SHV_VERSION_MAJOR.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_SHV_VERSION_MINOR.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_NAME.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_PING.into(), ..Default::default() },
    ];
    static ref DEVICE_METHODS: [MetaMethod; 3] = [
        MetaMethod { name: METH_NAME.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_VERSION.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_SERIAL_NUMBER.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
    ];
}

pub struct AppNode {
    pub name: &'static str,
    pub shv_version_major: i32,
    pub shv_version_minor: i32,
}
impl Default for AppNode {
    fn default() -> Self {
        AppNode {
            name: "",
            shv_version_major: 3,
            shv_version_minor: 0,
        }
    }
}
impl ShvNode for AppNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(APP_METHODS.iter()).collect()
    }

    fn process_request(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        match rpcmsg.method() {
            Some(METH_PING) => {
                Ok(Some(RpcValue::from(())))
            }
            Some(METH_NAME) => {
                Ok(Some(RpcValue::from(self.name)))
            }
            Some(METH_SHV_VERSION_MAJOR) => {
                Ok(Some(RpcValue::from(self.shv_version_major)))
            }
            Some(METH_SHV_VERSION_MINOR) => {
                Ok(Some(RpcValue::from(self.shv_version_minor)))
            }
            _ => {
                ShvNode::process_request_dir_ls(self, rpcmsg)
            }
        }
    }
}

pub struct DeviceNode {
    pub name: &'static str,
    pub version: &'static str,
    pub serial_number: Option<String>,
}
impl Default for DeviceNode {
    fn default() -> Self {
        DeviceNode {
            name: "",
            version: "",
            serial_number: None,
        }
    }
}
impl ShvNode for DeviceNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS_METHODS.iter().chain(DEVICE_METHODS.iter()).collect()
    }
    fn process_request(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        match rpcmsg.method() {
            Some(METH_NAME) => {
                Ok(Some(RpcValue::from(self.name)))
            }
            Some(METH_VERSION) => {
                Ok(Some(RpcValue::from(self.version)))
            }
            Some(METH_SERIAL_NUMBER) => {
                match &self.serial_number {
                    None => {Ok(None)}
                    Some(sn) => {Ok(Some(RpcValue::from(sn)))}
                }
            }
            _ => {
                ShvNode::process_request_dir_ls(self, rpcmsg)
            }
        }
    }
}
