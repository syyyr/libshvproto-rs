use crate::{RpcValue, rpcvalue};

#[derive(Debug)]
pub enum Flag {
    None = 0,
    IsSignal = 1 << 0,
    IsGetter = 1 << 1,
    IsSetter = 1 << 2,
    LargeResultHint = 1 << 3,
}
impl Into<u32> for Flag {
    fn into(self) -> u32 {
        return self as u32;
    }
}
impl From<u8> for Flag {
    fn from(value: u8) -> Self {
        match value {
            0 => Flag::None,
            1 => Flag::IsSignal,
            2 => Flag::IsGetter,
            4 => Flag::IsSetter,
            8 => Flag::LargeResultHint,
            _ => Flag::None,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum Access { Browse = 0, Read, Write, Command, Config, Service, SuperService, Developer, Superuser }
impl Access {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "bws" => Some(Access::Browse),
            "rd" => Some(Access::Read),
            "wr" => Some(Access::Write),
            "cmd" => Some(Access::Command),
            "cfg" => Some(Access::Config),
            "srv" => Some(Access::Service),
            "ssrv" => Some(Access::SuperService),
            "dev" => Some(Access::Developer),
            "su" => Some(Access::Superuser),
            _ => None,
        }
    }
}
impl From<&str> for Access {
    fn from(value: &str) -> Self {
        match Self::from_str(value) {
            None => { Self::Browse }
            Some(acc) => { acc }
        }
    }
}

#[derive(Debug)]
pub struct MetaMethod {
    pub name: &'static str,
    pub flags: u32,
    pub access: Access,
    pub param: &'static str,
    pub result: &'static str,
    pub description: &'static str,
}
impl Default for MetaMethod {
    fn default() -> Self {
        MetaMethod {
            name: "",
            flags: 0,
            access: Access::Browse,
            param: "",
            result: "",
            description: "",
        }
    }
}
#[derive(Debug, Copy, Clone)]
pub enum DirFormat {
    IMap,
    Map,
}
impl MetaMethod {
    pub fn to_rpcvalue(&self, fmt: DirFormat) -> RpcValue {
        match fmt {
            DirFormat::IMap => {
                let mut m = rpcvalue::IMap::new();
                m.insert(DirAttribute::Name.into(), (self.name).into());
                m.insert(DirAttribute::Flags.into(), self.flags.into());
                m.insert(DirAttribute::Param.into(), (self.param).into());
                m.insert(DirAttribute::Result.into(), (self.result).into());
                m.insert(DirAttribute::Access.into(), (self.access as i32).into());
                m.into()
            }
            DirFormat::Map => {
                let mut m = rpcvalue::Map::new();
                m.insert(DirAttribute::Name.into(), (self.name).into());
                m.insert(DirAttribute::Flags.into(), self.flags.into());
                m.insert(DirAttribute::Param.into(), (self.param).into());
                m.insert(DirAttribute::Result.into(), (self.result).into());
                m.insert(DirAttribute::Access.into(), (self.access as i32).into());
                m.insert("description".into(), (self.description).into());
                m.into()
            }
        }
    }
}

// attributes for 'dir' command
#[derive(Debug, Copy, Clone)]
pub enum DirAttribute {
    Name = 1,
    Flags,
    Param,
    Result,
    Access,
}

impl Into<i32> for DirAttribute {
    fn into(self) -> i32 {
        self as i32
    }
}
impl Into<&str> for DirAttribute {
    fn into(self) -> &'static str {
        match self {
            DirAttribute::Name => {"name"}
            DirAttribute::Flags => {"flags"}
            DirAttribute::Param => {"param"}
            DirAttribute::Result => {"result"}
            DirAttribute::Access => {"access"}
        }
    }
}
impl Into<String> for DirAttribute {
    fn into(self) -> String {
        <DirAttribute as Into<&str>>::into(self).to_string()
    }
}

