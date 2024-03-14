use crate::{RpcValue, rpcvalue};

#[derive(Debug)]
pub enum Flag {
    None = 0,
    IsSignal = 1 << 0,
    IsGetter = 1 << 1,
    IsSetter = 1 << 2,
    LargeResultHint = 1 << 3,
}
impl From<Flag> for u32 {
    fn from(val: Flag) -> Self {
        val as u32
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
pub enum Access {
    Browse = 1,
    Read = 8,
    Write = 16,
    Command = 24,
    Config = 32,
    Service = 40,
    SuperService = 48,
    Developer = 56,
    Superuser = 63
}

impl Access {
    // It makes sense to return Option rather than Result as the `FromStr` trait does.
    #[allow(clippy::should_implement_trait)]
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

    pub fn as_str(&self) -> &'static str {
        match self {
            Access::Browse => "bws",
            Access::Read => "rd",
            Access::Write => "wr",
            Access::Command => "cmd",
            Access::Config => "cfg",
            Access::Service => "srv",
            Access::SuperService => "ssrv",
            Access::Developer => "dev",
            Access::Superuser => "su",
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

impl TryFrom<i32> for Access {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            value if value == Access::Browse as i32 => Ok(Access::Browse),
            value if value == Access::Read as i32 => Ok(Access::Read),
            value if value == Access::Write as i32 => Ok(Access::Write),
            value if value == Access::Command as i32 => Ok(Access::Command),
            value if value == Access::Config as i32 => Ok(Access::Config),
            value if value == Access::Service as i32 => Ok(Access::Service),
            value if value == Access::SuperService as i32 => Ok(Access::SuperService),
            value if value == Access::Developer as i32 => Ok(Access::Developer),
            value if value == Access::Superuser as i32 => Ok(Access::Superuser),
            _ => Err(()),
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
impl From<DirAttribute> for i32 {
    fn from(val: DirAttribute) -> Self {
        val as i32
    }
}
impl From<DirAttribute> for &str {
    fn from(val: DirAttribute) -> Self {
        match val {
            DirAttribute::Name => {"name"}
            DirAttribute::Flags => {"flags"}
            DirAttribute::Param => {"param"}
            DirAttribute::Result => {"result"}
            DirAttribute::Access => {"access"}
        }
    }
}
impl From<DirAttribute> for String {
    fn from(val: DirAttribute) -> Self {
        <DirAttribute as Into<&str>>::into(val).to_string()
    }
}

