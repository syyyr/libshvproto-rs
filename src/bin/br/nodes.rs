use lazy_static::lazy_static;
use shv::metamethod::MetaMethod;
use shv::shvnode::ShvNode;

lazy_static! {
    static ref DIR_LS: [MetaMethod; 2] = [
        MetaMethod { name: "dir".into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
        MetaMethod { name: "ls".into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
    ];
}

struct DirNode {
    // methods: Vec<MetaMethod>
}
impl ShvNode for DirNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS.iter().collect()
    }
}

lazy_static! {
    static ref APP_METHODS: [MetaMethod; 1] = [
        MetaMethod { name: "ping".into(), ..Default::default() },
    ];
}
pub(crate) struct AppNode {
}
impl ShvNode for AppNode {
    fn methods(&self) -> Vec<&MetaMethod> {
        DIR_LS.iter().chain(APP_METHODS.iter()).collect()
    }
}