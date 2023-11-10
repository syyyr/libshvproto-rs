use crate::RpcValue;
use crate::rpcvalue::{List};

#[derive(Copy, Clone, Debug)]
pub enum Signature {
    VoidVoid = 0,
    VoidParam,
    RetVoid,
    RetParam,
}

#[derive(Debug)]
pub enum Flag {
    None = 0,
    IsSignal = 1 << 0,
    IsGetter = 1 << 1,
    IsSetter = 1 << 2,
    LargeResultHint = 1 << 3,
}
impl Into<u8> for Flag {
    fn into(self) -> u8 {
        return self as u8;
    }
}

#[derive(Debug)]
pub struct MetaMethod {
    pub name: String,
    pub signature: Signature,
    pub flags: u8,
    pub access_grant: RpcValue,
    pub description: String,
}

impl MetaMethod {
    pub fn to_rpcvalue(&self, mask: u8) -> RpcValue {
        if mask == 0 {
            return self.name.clone().into();
        }
        let mut lst = List::new();
        if (mask & DirAttribute::Signature as u8) != 0 {
            lst.push(RpcValue::from(self.signature as u32));
        }
        if (mask & DirAttribute::Flags as u8) != 0 {
            lst.push(RpcValue::from(self.flags as u32));
        }
        if (mask & DirAttribute::AccessGrant as u8) != 0 {
            lst.push(self.access_grant.clone());
        }
        if (mask & DirAttribute::Description as u8) != 0 {
            lst.push(RpcValue::from(&self.description));
        }
        if lst.is_empty() {
            return RpcValue::from(&self.name);
        }
        lst.insert(0, RpcValue::from(&self.name));
        return RpcValue::from(lst);
    }
}

// attributes for 'dir' command
enum DirAttribute {
    Signature = 1 << 0,
    Flags = 1 << 1,
    AccessGrant = 1 << 2,
    Description = 1 << 3,
}
// attributes for 'ls' command
pub enum LsAttribute {
    HasChildren = 1 << 0,
}

impl Into<u8> for DirAttribute {
    fn into(self) -> u8 {
        return self as u8;
    }
}

