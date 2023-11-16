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
#[derive(Debug, Copy, Clone)]
pub enum Access { Bws = 0, Rd, Wr, Cmd, Cfg, Srv, Ssrv, Dev, Su }
impl From<&str> for Access {
    fn from(value: &str) -> Self {
        match value {
            "bws" => Access::Bws,
            "rd" => Access::Rd,
            "wr" => Access::Wr,
            "cmd" => Access::Cmd,
            "cfg" => Access::Cfg,
            "srv" => Access::Srv,
            "ssrv" => Access::Ssrv,
            "dev" => Access::Dev,
            "su" => Access::Su,
            _ => Access::Bws,
        }
    }
}

#[derive(Debug)]
pub struct MetaMethod {
    pub name: String,
    pub flags: u32,
    pub access: Access,
    pub param: String,
    pub result: String,
    pub description: String,
}
impl Default for MetaMethod {
    fn default() -> Self {
        MetaMethod {
            name: "".to_string(),
            flags: 0,
            access: Access::Bws,
            param: "".to_string(),
            result: "".to_string(),
            description: "".to_string(),
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
                m.insert(DirAttribute::Name.into(), (&self.name).into());
                m.insert(DirAttribute::Flags.into(), self.flags.into());
                m.insert(DirAttribute::Param.into(), (&self.param).into());
                m.insert(DirAttribute::Result.into(), (&self.result).into());
                m.insert(DirAttribute::Access.into(), (self.access as i32).into());
                m.into()
            }
            DirFormat::Map => {
                let mut m = rpcvalue::Map::new();
                m.insert(DirAttribute::Name.into(), (&self.name).into());
                m.insert(DirAttribute::Flags.into(), self.flags.into());
                m.insert(DirAttribute::Param.into(), (&self.param).into());
                m.insert(DirAttribute::Result.into(), (&self.result).into());
                m.insert(DirAttribute::Access.into(), (self.access as i32).into());
                m.insert("description".into(), (&self.description).into());
                m.into()
            }
        }
    }
}

// attributes for 'dir' command
#[derive(Debug, Copy, Clone)]
enum DirAttribute {
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

