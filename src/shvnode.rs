use std::collections::BTreeMap;
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
    let mut dir_exists = false;
    for (key, _) in mounts.range(path.to_string()..) {
        if key.starts_with(path) {
            dir_exists = true;
            if path.is_empty()
                || key.len() == path.len()
                || key.as_bytes()[path.len()] == ('/' as u8)
            {
                if key.len() > path.len() {
                    let dir_rest_start = if path.is_empty() {
                        0
                    } else {
                        path.len() + 1
                    };
                    let mut updirs = key[dir_rest_start..].split('/');
                    if let Some(dir) = updirs.next() {
                        dirs.push(dir.to_string());
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
    fn process_request_dir(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        match rpcmsg.method() {
            Some(METH_DIR) => {
                Ok(Some(dir(self.methods().into_iter(), rpcmsg.param().into())))
            }
            _ => {
                Err(format!("NIY {}", rpcmsg).into())
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

lazy_static! {
    static ref APP_METHODS: [MetaMethod; 6] = [
        MetaMethod { name: METH_DIR.into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
        MetaMethod { name: METH_LS.into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
        MetaMethod { name: METH_SHV_VERSION_MAJOR.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_SHV_VERSION_MINOR.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_NAME.into(), flags: Flag::IsGetter.into(),  ..Default::default() },
        MetaMethod { name: METH_PING.into(), ..Default::default() },
    ];
}

pub fn shv_dir_methods() -> Vec<&'static MetaMethod> {
    APP_METHODS.iter().take(2).collect()
}
pub struct AppNode {
    pub app_name: String,
    pub shv_version_major: i32,
    pub shv_version_minor: i32,
}
impl Default for AppNode {
    fn default() -> Self {
        AppNode {
            app_name: "".to_string(),
            shv_version_major: 3,
            shv_version_minor: 0,
        }
    }
}
impl ShvNode for AppNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        APP_METHODS.iter().collect()
    }

    fn process_request(&mut self, rpcmsg: &RpcMessage) -> ProcessRequestResult {
        match rpcmsg.method() {
            Some(METH_PING) => {
                Ok(Some(RpcValue::from(())))
            }
            Some(METH_NAME) => {
                Ok(Some(RpcValue::from(&self.app_name)))
            }
            Some(METH_SHV_VERSION_MAJOR) => {
                Ok(Some(RpcValue::from(self.shv_version_major)))
            }
            Some(METH_SHV_VERSION_MINOR) => {
                Ok(Some(RpcValue::from(self.shv_version_minor)))
            }
            _ => {
                ShvNode::process_request_dir(self, rpcmsg)
            }
        }
    }
}
