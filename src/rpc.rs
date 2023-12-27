use std::fmt::{Display, Formatter};
use glob::Pattern;
use crate::RpcValue;
use crate::Map;

#[derive(Debug, PartialEq)]
pub struct Subscription {
    pub paths: Pattern,
    pub methods: Pattern,
}
impl Subscription {
    pub fn new(paths: &str, methods: &str) -> crate::Result<Self> {
        let paths = if paths.is_empty() { "**" } else { paths };
        let methods = if methods.is_empty() { "?*" } else { methods };
        match Pattern::new(methods) {
            Ok(methods) => {
                match Pattern::new(paths) {
                    Ok(paths) => {
                        Ok(Self {
                            methods,
                            paths,
                        })
                    }
                    Err(err) => { Err(format!("{}", &err).into()) }
                }
            }
            Err(err) => { Err(format!("{}", &err).into()) }
        }
    }
    pub fn match_signal(&self, path: &str, method: &str) -> bool {
        self.paths.matches(path) && self.methods.matches(method)
    }
    pub fn from_rpcvalue(value: &RpcValue) -> crate::Result<Subscription> {
        let m = value.as_map();
        let methods = m.get("method").unwrap_or(m.get("methods").unwrap_or_default()).as_str();
        let paths = m.get("path").unwrap_or(m.get("paths").unwrap_or_default()).as_str();
        Self::new(paths, methods)
    }
    pub fn to_rpcvalue(&self) -> RpcValue {
        let mut m = Map::new();
        m.insert("paths".into(), self.paths.as_str().into());
        m.insert("methods".into(), self.methods.as_str().into());
        RpcValue::from(m)
    }
}
impl Display for Subscription {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{paths: {}, methods: {}}}", self.paths.as_str(), self.methods.as_str())
    }
}
impl Into<RpcValue> for Subscription {
    fn into(self) -> RpcValue {
        let mut map = Map::new();
        map.insert("paths".to_string(), self.paths.as_str().into());
        map.insert("methods".to_string(), self.methods.as_str().into());
        RpcValue::from(map)
    }
}
