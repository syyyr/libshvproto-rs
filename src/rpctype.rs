#[allow(dead_code)]
pub enum Tag {
    MetaTypeId = 1,
    MetaTypeNameSpaceId,
    USER = 8
}

#[allow(dead_code)]
pub enum NameSpaceID
{
    Global = 0,
    Elesys,
    Eyas,
}

#[allow(non_snake_case)]
pub mod GlobalNS
{
    #[allow(dead_code)]
    pub enum MetaTypeID
    {
        ChainPackRpcMessage = 1,
        RpcConnectionParams,
        TunnelCtl,
        AccessGrantLogin,
        ValueChange,
    }
}
